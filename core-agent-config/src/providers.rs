use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::{
    AgentConfigPatch, ConfigError, ConfigLayer, ConfigModelPatch, ConfigPermissionsPatch,
    ConfigProvider, ConfigRequest, ConfigResult, ConfigSourceInfo, SecretResolver,
};

const MAX_CONFIG_BYTES: u64 = 256 * 1024;
const USER_FILENAMES: [&str; 3] = [
    "core-agent-config.yaml",
    "core-agent-config.yml",
    "core-agent-config.json",
];
const PROJECT_FILENAMES: [&str; 3] = ["config.yaml", "config.yml", "config.json"];

pub struct DefaultsConfigProvider;

#[async_trait]
impl ConfigProvider for DefaultsConfigProvider {
    fn key(&self) -> &str {
        "builtin-defaults"
    }

    fn priority(&self) -> u16 {
        0
    }

    async fn load(&self, _request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>> {
        Ok(Some(ConfigLayer {
            source: source(self, None),
            patch: AgentConfigPatch::defaults(),
        }))
    }
}

pub struct UserFileConfigProvider {
    directory: Option<PathBuf>,
    explicit: Option<PathBuf>,
}

impl UserFileConfigProvider {
    pub fn discover() -> Self {
        Self {
            directory: std::env::var_os("CORE_AGENT_HOME")
                .map(PathBuf::from)
                .or_else(default_user_config_directory),
            explicit: std::env::var_os("CORE_AGENT_CONFIG").map(PathBuf::from),
        }
    }

    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: Some(directory.into()),
            explicit: None,
        }
    }

    pub fn explicit(path: impl Into<PathBuf>) -> Self {
        Self {
            directory: None,
            explicit: Some(path.into()),
        }
    }

    pub fn default_directory() -> ConfigResult<PathBuf> {
        std::env::var_os("CORE_AGENT_HOME")
            .map(PathBuf::from)
            .or_else(default_user_config_directory)
            .ok_or_else(|| {
                ConfigError::Source("cannot locate the user directory; set CORE_AGENT_HOME".into())
            })
    }

    fn locate(&self) -> ConfigResult<Option<PathBuf>> {
        if let Some(explicit) = &self.explicit {
            return Ok(Some(explicit.clone()));
        }
        let Some(directory) = &self.directory else {
            return Ok(None);
        };
        locate_one(directory, &USER_FILENAMES)
    }
}

#[async_trait]
impl ConfigProvider for UserFileConfigProvider {
    fn key(&self) -> &str {
        "user-file"
    }

    fn priority(&self) -> u16 {
        100
    }

    async fn load(&self, _request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>> {
        let Some(path) = self.locate()? else {
            return Ok(None);
        };
        Ok(Some(ConfigLayer {
            patch: read_patch(&path)?,
            source: source(self, Some(&path)),
        }))
    }
}

pub struct ProjectFileConfigProvider;

#[async_trait]
impl ConfigProvider for ProjectFileConfigProvider {
    fn key(&self) -> &str {
        "project-file"
    }

    fn priority(&self) -> u16 {
        200
    }

    async fn load(&self, request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>> {
        let Some(workspace) = &request.workspace else {
            return Ok(None);
        };
        let Some(path) = locate_one(&workspace.join(".agent"), &PROJECT_FILENAMES)? else {
            return Ok(None);
        };
        Ok(Some(ConfigLayer {
            patch: read_patch(&path)?,
            source: source(self, Some(&path)),
        }))
    }
}

pub struct EnvironmentConfigProvider {
    values: BTreeMap<String, String>,
}

impl EnvironmentConfigProvider {
    pub fn current() -> Self {
        Self {
            values: std::env::vars()
                .filter(|(key, _)| key.starts_with("CORE_AGENT_"))
                .collect(),
        }
    }

    pub fn new(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }
}

#[async_trait]
impl ConfigProvider for EnvironmentConfigProvider {
    fn key(&self) -> &str {
        "environment"
    }

    fn priority(&self) -> u16 {
        300
    }

    async fn load(&self, _request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>> {
        let model = ConfigModelPatch {
            provider: self.values.get("CORE_AGENT_MODEL_PROVIDER").cloned(),
            endpoint: self.values.get("CORE_AGENT_MODEL_ENDPOINT").cloned(),
            name: self.values.get("CORE_AGENT_MODEL").cloned(),
            profile: self.values.get("CORE_AGENT_MODEL_PROFILE").cloned(),
            api_key: self.values.get("CORE_AGENT_API_KEY").cloned(),
            api_key_ref: None,
            api_key_env: None,
        };
        let permissions = self
            .values
            .get("CORE_AGENT_PERMISSION_MODE")
            .cloned()
            .map(|mode| ConfigPermissionsPatch { mode: Some(mode) });
        let has_model = model.provider.is_some()
            || model.endpoint.is_some()
            || model.name.is_some()
            || model.profile.is_some()
            || model.api_key.is_some();
        if !has_model && permissions.is_none() {
            return Ok(None);
        }
        Ok(Some(ConfigLayer {
            source: source(self, None),
            patch: AgentConfigPatch {
                model: has_model.then_some(model),
                permissions,
                ..AgentConfigPatch::default()
            },
        }))
    }
}

pub struct EnvironmentSecretResolver {
    values: BTreeMap<String, String>,
}

impl EnvironmentSecretResolver {
    pub fn current() -> Self {
        Self {
            values: std::env::vars().collect(),
        }
    }

    pub fn new(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }
}

#[async_trait]
impl SecretResolver for EnvironmentSecretResolver {
    fn key(&self) -> &str {
        "environment-secret"
    }

    fn supports(&self, reference: &str) -> bool {
        reference.starts_with("env:")
    }

    async fn resolve(&self, reference: &str) -> ConfigResult<Option<String>> {
        let name = reference.strip_prefix("env:").ok_or_else(|| {
            ConfigError::Secret("environment reference must start with env:".into())
        })?;
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ConfigError::Secret(
                "environment secret name is invalid".into(),
            ));
        }
        Ok(self.values.get(name).cloned())
    }
}

fn source(provider: &dyn ConfigProvider, path: Option<&Path>) -> ConfigSourceInfo {
    ConfigSourceInfo {
        provider: provider.key().into(),
        priority: provider.priority(),
        location: path.map(|value| value.to_string_lossy().into_owned()),
    }
}

fn default_user_config_directory() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .map(|path| path.join("core-agent"))
}

fn locate_one(directory: &Path, names: &[&str]) -> ConfigResult<Option<PathBuf>> {
    if !directory.exists() {
        return Ok(None);
    }
    let matches = names
        .iter()
        .map(|name| directory.join(name))
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [path] => Ok(Some(path.clone())),
        _ => Err(ConfigError::Ambiguous(
            matches
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
        )),
    }
}

fn read_patch(path: &Path) -> ConfigResult<AgentConfigPatch> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        ConfigError::Source(format!("cannot inspect {}: {error}", path.display()))
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_CONFIG_BYTES
    {
        return Err(ConfigError::Source(format!(
            "{} must be a regular file no larger than 256 KiB",
            path.display()
        )));
    }
    let bytes = fs::read(path)
        .map_err(|error| ConfigError::Source(format!("cannot read {}: {error}", path.display())))?;
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("json") => serde_json::from_slice(&bytes).map_err(ConfigError::from),
        Some("yaml" | "yml") => serde_yaml::from_slice(&bytes).map_err(ConfigError::from),
        _ => Err(ConfigError::Source(format!(
            "{} must use .json, .yaml or .yml",
            path.display()
        ))),
    }
}
