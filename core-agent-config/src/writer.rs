use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{
    AgentConfig, ConfigCompression, ConfigError, ConfigModel, ConfigResult, UserFileConfigProvider,
    CONFIG_SCHEMA_VERSION,
};

const MAX_CONFIG_BYTES: usize = 256 * 1024;
const USER_FILENAMES: [&str; 3] = [
    "core-agent-config.yaml",
    "core-agent-config.yml",
    "core-agent-config.json",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserConfigSnapshot {
    pub path: PathBuf,
    pub fingerprint: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct UserConfigUpdate {
    pub active_model: String,
    pub models: Vec<ConfigModel>,
    pub compression: ConfigCompression,
}

impl std::fmt::Debug for UserConfigUpdate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserConfigUpdate")
            .field("active_model", &self.active_model)
            .field(
                "models",
                &self
                    .models
                    .iter()
                    .map(ConfigModel::redacted)
                    .collect::<Vec<_>>(),
            )
            .field("compression", &self.compression)
            .finish()
    }
}

/// Atomic, compare-and-swap writer for the same user configuration consumed by
/// Terminal and Desktop. Project and environment layers remain read-only.
pub struct UserConfigWriter {
    path: PathBuf,
}

impl UserConfigWriter {
    pub fn discover() -> ConfigResult<Self> {
        if let Some(explicit) = std::env::var_os("CORE_AGENT_CONFIG") {
            return Ok(Self {
                path: PathBuf::from(explicit),
            });
        }
        Self::new(UserFileConfigProvider::default_directory()?)
    }

    pub fn new(directory: impl Into<PathBuf>) -> ConfigResult<Self> {
        let directory = directory.into();
        let existing = USER_FILENAMES
            .iter()
            .map(|name| directory.join(name))
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        let path = match existing.as_slice() {
            [] => directory.join(USER_FILENAMES[0]),
            [path] => path.clone(),
            _ => {
                return Err(ConfigError::Ambiguous(
                    existing
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ))
            }
        };
        Ok(Self { path })
    }

    pub fn snapshot(&self) -> ConfigResult<UserConfigSnapshot> {
        let fingerprint = if self.path.exists() {
            Some(fingerprint(&read_regular_file(&self.path)?))
        } else {
            None
        };
        Ok(UserConfigSnapshot {
            path: self.path.clone(),
            fingerprint,
        })
    }

    pub fn save(
        &self,
        update: &UserConfigUpdate,
        expected_fingerprint: Option<&str>,
    ) -> ConfigResult<UserConfigSnapshot> {
        let mut update = update.clone();
        update.active_model = update.active_model.trim().to_owned();
        for model in &mut update.models {
            model.normalize();
        }
        validate_update(&update)?;
        let (mut document, current_fingerprint) = if self.path.exists() {
            let bytes = read_regular_file(&self.path)?;
            let fingerprint = fingerprint(&bytes);
            (parse_document(&self.path, &bytes)?, Some(fingerprint))
        } else {
            (Value::Object(Map::new()), None)
        };
        if current_fingerprint.as_deref() != expected_fingerprint {
            return Err(ConfigError::Source(
                "user configuration changed; reload before saving".into(),
            ));
        }
        merge_update(&mut document, &update)?;
        let bytes = serialize_document(&self.path, &document)?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError::Source(
                "user configuration exceeds 256 KiB".into(),
            ));
        }
        atomic_replace(&self.path, &bytes)?;
        self.snapshot()
    }
}

fn validate_update(update: &UserConfigUpdate) -> ConfigResult<()> {
    let Some(active) = update
        .models
        .iter()
        .find(|model| model.name == update.active_model)
        .cloned()
    else {
        return Err(ConfigError::Validation(
            "activeModel must reference a configured model".into(),
        ));
    };
    let mut config = AgentConfig::default();
    config.version = CONFIG_SCHEMA_VERSION;
    config.active_model = update.active_model.clone();
    config.models = update.models.clone();
    config.model = active;
    config.context.compression = update.compression.clone();
    config.validate()
}

fn merge_update(document: &mut Value, update: &UserConfigUpdate) -> ConfigResult<()> {
    let object = document.as_object_mut().ok_or_else(|| {
        ConfigError::Validation("user configuration root must be an object".into())
    })?;
    let old_secrets = extract_secrets(object);
    object.insert("version".into(), json!(CONFIG_SCHEMA_VERSION));
    object.insert("activeModel".into(), json!(update.active_model));
    object.remove("model");
    object.insert(
        "models".into(),
        Value::Array(
            update
                .models
                .iter()
                .map(|model| model_value(model, old_secrets.get(&model.name)))
                .collect(),
        ),
    );
    let context = object
        .entry("context")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| ConfigError::Validation("context must be an object".into()))?;
    context.insert("compression".into(), json!(update.compression));
    Ok(())
}

fn model_value(
    model: &ConfigModel,
    old_secret: Option<&(Option<String>, Option<String>)>,
) -> Value {
    let mut value = Map::from_iter([
        ("name".into(), json!(model.name)),
        ("baseURL".into(), json!(model.endpoint)),
        ("provider".into(), json!(model.provider)),
        ("profile".into(), json!(model.profile)),
        ("maxContextTokens".into(), json!(model.max_context_tokens)),
    ]);
    let api_key = model
        .api_key
        .clone()
        .or_else(|| old_secret.and_then(|secret| secret.0.clone()));
    let api_key_ref = model
        .api_key_ref
        .clone()
        .or_else(|| old_secret.and_then(|secret| secret.1.clone()));
    if let Some(secret) = api_key {
        value.insert("apiKey".into(), json!(secret));
    } else if let Some(reference) = api_key_ref {
        value.insert("apiKeyRef".into(), json!(reference));
    }
    Value::Object(value)
}

fn extract_secrets(
    object: &Map<String, Value>,
) -> std::collections::BTreeMap<String, (Option<String>, Option<String>)> {
    let mut secrets = std::collections::BTreeMap::new();
    if let Some(models) = object.get("models").and_then(Value::as_array) {
        for model in models {
            if let (Some(name), Some(value)) =
                (model.get("name").and_then(Value::as_str), model.as_object())
            {
                secrets.insert(
                    name.to_owned(),
                    (
                        value
                            .get("apiKey")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        value
                            .get("apiKeyRef")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                    ),
                );
            }
        }
    } else if let Some(model) = object.get("model").and_then(Value::as_object) {
        if let Some(name) = model.get("name").and_then(Value::as_str) {
            secrets.insert(
                name.to_owned(),
                (
                    model
                        .get("apiKey")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    model
                        .get("apiKeyRef")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                ),
            );
        }
    }
    secrets
}

fn read_regular_file(path: &Path) -> ConfigResult<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ConfigError::Source(format!("cannot inspect {}: {error}", path.display()))
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_CONFIG_BYTES as u64
    {
        return Err(ConfigError::Source(format!(
            "{} must be a regular file no larger than 256 KiB",
            path.display()
        )));
    }
    fs::read(path).map_err(ConfigError::from)
}

fn parse_document(path: &Path, bytes: &[u8]) -> ConfigResult<Value> {
    match extension(path) {
        "json" => serde_json::from_slice(bytes).map_err(ConfigError::from),
        "yaml" | "yml" => serde_yaml::from_slice(bytes).map_err(ConfigError::from),
        _ => Err(ConfigError::Source(
            "user configuration must use .json, .yaml or .yml".into(),
        )),
    }
}

fn serialize_document(path: &Path, document: &Value) -> ConfigResult<Vec<u8>> {
    match extension(path) {
        "json" => serde_json::to_vec_pretty(document).map_err(ConfigError::from),
        "yaml" | "yml" => serde_yaml::to_string(document)
            .map(String::into_bytes)
            .map_err(ConfigError::from),
        _ => Err(ConfigError::Source(
            "user configuration must use .json, .yaml or .yml".into(),
        )),
    }
}

fn extension(path: &Path) -> &str {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
}

fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> ConfigResult<()> {
    let parent = path.parent().ok_or_else(|| {
        ConfigError::Source("user configuration path has no parent directory".into())
    })?;
    fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ConfigError::Source("user configuration filename is invalid".into()))?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    let backup = parent.join(format!(".{file_name}.replace-backup"));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(ConfigError::Io(error));
    }
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_MAX_CONTEXT_TOKENS;

    #[test]
    fn writer_migrates_v1_preserves_secret_and_rejects_stale_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(USER_FILENAMES[0]);
        fs::write(
            &path,
            "version: 1\nmodel:\n  provider: deepseek\n  endpoint: https://api.deepseek.com\n  name: first\n  profile: first\n  apiKey: private\n",
        )
        .unwrap();
        let writer = UserConfigWriter::new(directory.path()).unwrap();
        let snapshot = writer.snapshot().unwrap();
        let update = UserConfigUpdate {
            active_model: "first".into(),
            models: vec![ConfigModel {
                provider: "deepseek".into(),
                endpoint: "https://api.deepseek.com".into(),
                name: "first".into(),
                profile: "first".into(),
                max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
                api_key: None,
                api_key_ref: None,
                stream: true,
            }],
            compression: ConfigCompression::default(),
        };
        let saved = writer
            .save(&update, snapshot.fingerprint.as_deref())
            .unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("version: 2"));
        assert!(contents.contains("private"));
        assert!(writer
            .save(&update, snapshot.fingerprint.as_deref())
            .is_err());
        assert_ne!(saved.fingerprint, snapshot.fingerprint);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
