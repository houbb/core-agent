use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{ConfigError, ConfigResult};

pub const CONFIG_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub version: u32,
    pub model: ConfigModel,
    pub permissions: ConfigPermissions,
    pub memory: ConfigMemory,
    pub session: ConfigSession,
    pub context: ConfigContext,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_SCHEMA_VERSION,
            model: ConfigModel::default(),
            permissions: ConfigPermissions::default(),
            memory: ConfigMemory::default(),
            session: ConfigSession::default(),
            context: ConfigContext::default(),
        }
    }
}

impl std::fmt::Debug for AgentConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentConfig")
            .field("redacted", &self.redacted())
            .finish()
    }
}

impl AgentConfig {
    pub fn validate(&self) -> ConfigResult<()> {
        if self.version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::Validation(format!(
                "unsupported schema version {}; expected {CONFIG_SCHEMA_VERSION}",
                self.version
            )));
        }
        for (label, value, maximum) in [
            ("model provider", self.model.provider.as_str(), 128),
            ("model endpoint", self.model.endpoint.as_str(), 2_048),
            ("model name", self.model.name.as_str(), 256),
            ("model profile", self.model.profile.as_str(), 128),
            ("permission mode", self.permissions.mode.as_str(), 32),
        ] {
            validate_text(label, value, maximum)?;
        }
        if !(self.model.endpoint.starts_with("http://")
            || self.model.endpoint.starts_with("https://"))
            || !matches!(
                self.permissions.mode.as_str(),
                "strict" | "risk-based" | "auto"
            )
            || self.context.max_mentions == 0
            || self.context.max_mentions > 64
            || self.context.max_files == 0
            || self.context.max_files > 2_000
            || self.context.max_file_bytes == 0
            || self.context.max_file_bytes > 1024 * 1024
            || self.context.max_total_bytes < self.context.max_file_bytes
            || self.context.max_total_bytes > 4 * 1024 * 1024
            || self.context.max_directory_depth == 0
            || self.context.max_directory_depth > 32
        {
            return Err(ConfigError::Validation(
                "model, permission or context limits are invalid".into(),
            ));
        }
        if self.model.api_key.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 16 * 1024 || value.chars().any(char::is_control)
        }) {
            return Err(ConfigError::Validation("model apiKey is invalid".into()));
        }
        if self.model.api_key_ref.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 512 || value.chars().any(char::is_control)
        }) {
            return Err(ConfigError::Validation("model apiKeyRef is invalid".into()));
        }
        Ok(())
    }

    pub fn redacted(&self) -> Value {
        json!({
            "version": self.version,
            "model": {
                "provider": self.model.provider,
                "endpoint": self.model.endpoint,
                "name": self.model.name,
                "profile": self.model.profile,
                "apiKeyConfigured": self.model.api_key.is_some(),
                "apiKeyRef": self.model.api_key_ref,
            },
            "permissions": {"mode": self.permissions.mode},
            "memory": {"enabled": self.memory.enabled},
            "session": {"resumeLast": self.session.resume_last},
            "context": {
                "maxMentions": self.context.max_mentions,
                "maxFiles": self.context.max_files,
                "maxFileBytes": self.context.max_file_bytes,
                "maxTotalBytes": self.context.max_total_bytes,
                "maxDirectoryDepth": self.context.max_directory_depth,
            }
        })
    }

    pub(crate) fn apply(&mut self, patch: AgentConfigPatch) {
        if let Some(version) = patch.version {
            self.version = version;
        }
        if let Some(model) = patch.model {
            model.apply(&mut self.model);
        }
        if let Some(permissions) = patch.permissions {
            permissions.apply(&mut self.permissions);
        }
        if let Some(memory) = patch.memory {
            memory.apply(&mut self.memory);
        }
        if let Some(session) = patch.session {
            session.apply(&mut self.session);
        }
        if let Some(context) = patch.context {
            context.apply(&mut self.context);
        }
    }
}

fn validate_text(label: &str, value: &str, maximum: usize) -> ConfigResult<()> {
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(|character| character == '\0')
    {
        return Err(ConfigError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConfigModel {
    pub provider: String,
    pub endpoint: String,
    pub name: String,
    pub profile: String,
    pub api_key: Option<String>,
    pub api_key_ref: Option<String>,
}

impl Default for ConfigModel {
    fn default() -> Self {
        Self {
            provider: "deepseek".into(),
            endpoint: "https://api.deepseek.com".into(),
            name: "deepseek-v4-flash".into(),
            profile: "default".into(),
            api_key: None,
            api_key_ref: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPermissions {
    pub mode: String,
}

impl Default for ConfigPermissions {
    fn default() -> Self {
        Self {
            mode: "risk-based".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMemory {
    pub enabled: bool,
}

impl Default for ConfigMemory {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSession {
    pub resume_last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigContext {
    pub max_mentions: usize,
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_directory_depth: usize,
}

impl Default for ConfigContext {
    fn default() -> Self {
        Self {
            max_mentions: 16,
            max_files: 128,
            max_file_bytes: 256 * 1024,
            max_total_bytes: 1024 * 1024,
            max_directory_depth: 8,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfigPatch {
    pub version: Option<u32>,
    pub model: Option<ConfigModelPatch>,
    pub permissions: Option<ConfigPermissionsPatch>,
    pub memory: Option<ConfigMemoryPatch>,
    pub session: Option<ConfigSessionPatch>,
    pub context: Option<ConfigContextPatch>,
    #[serde(default)]
    pub server: Option<Value>,
    #[serde(default)]
    pub workspace: Option<Value>,
}

impl AgentConfigPatch {
    pub fn defaults() -> Self {
        let config = AgentConfig::default();
        Self {
            version: Some(config.version),
            model: Some(ConfigModelPatch {
                provider: Some(config.model.provider),
                endpoint: Some(config.model.endpoint),
                name: Some(config.model.name),
                profile: Some(config.model.profile),
                api_key: None,
                api_key_ref: None,
                api_key_env: None,
            }),
            permissions: Some(ConfigPermissionsPatch {
                mode: Some(config.permissions.mode),
            }),
            memory: Some(ConfigMemoryPatch {
                enabled: Some(config.memory.enabled),
            }),
            session: Some(ConfigSessionPatch {
                resume_last: Some(config.session.resume_last),
            }),
            context: Some(ConfigContextPatch {
                max_mentions: Some(config.context.max_mentions),
                max_files: Some(config.context.max_files),
                max_file_bytes: Some(config.context.max_file_bytes),
                max_total_bytes: Some(config.context.max_total_bytes),
                max_directory_depth: Some(config.context.max_directory_depth),
            }),
            server: None,
            workspace: None,
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigModelPatch {
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub name: Option<String>,
    pub profile: Option<String>,
    #[serde(alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(alias = "api_key_ref")]
    pub api_key_ref: Option<String>,
    #[serde(alias = "api_key_env")]
    pub api_key_env: Option<String>,
}

impl ConfigModelPatch {
    fn apply(self, target: &mut ConfigModel) {
        if let Some(value) = self.provider {
            target.provider = value;
        }
        if let Some(value) = self.endpoint {
            target.endpoint = value;
        }
        if let Some(value) = self.name {
            target.name = value;
        }
        if let Some(value) = self.profile {
            target.profile = value;
        }
        if let Some(value) = self.api_key {
            target.api_key = Some(value);
            target.api_key_ref = None;
        }
        if let Some(value) = self.api_key_ref {
            target.api_key_ref = Some(value);
            target.api_key = None;
        }
        if let Some(value) = self.api_key_env {
            target.api_key_ref = Some(format!("env:{value}"));
            target.api_key = None;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPermissionsPatch {
    pub mode: Option<String>,
}

impl ConfigPermissionsPatch {
    fn apply(self, target: &mut ConfigPermissions) {
        if let Some(value) = self.mode {
            target.mode = value;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigMemoryPatch {
    pub enabled: Option<bool>,
}

impl ConfigMemoryPatch {
    fn apply(self, target: &mut ConfigMemory) {
        if let Some(value) = self.enabled {
            target.enabled = value;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigSessionPatch {
    #[serde(alias = "resume_last")]
    pub resume_last: Option<bool>,
}

impl ConfigSessionPatch {
    fn apply(self, target: &mut ConfigSession) {
        if let Some(value) = self.resume_last {
            target.resume_last = value;
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigContextPatch {
    #[serde(alias = "max_mentions")]
    pub max_mentions: Option<usize>,
    #[serde(alias = "max_files")]
    pub max_files: Option<usize>,
    #[serde(alias = "max_file_bytes")]
    pub max_file_bytes: Option<usize>,
    #[serde(alias = "max_total_bytes")]
    pub max_total_bytes: Option<usize>,
    #[serde(alias = "max_directory_depth")]
    pub max_directory_depth: Option<usize>,
}

impl ConfigContextPatch {
    fn apply(self, target: &mut ConfigContext) {
        if let Some(value) = self.max_mentions {
            target.max_mentions = value;
        }
        if let Some(value) = self.max_files {
            target.max_files = value;
        }
        if let Some(value) = self.max_file_bytes {
            target.max_file_bytes = value;
        }
        if let Some(value) = self.max_total_bytes {
            target.max_total_bytes = value;
        }
        if let Some(value) = self.max_directory_depth {
            target.max_directory_depth = value;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRequest {
    pub workspace: Option<PathBuf>,
}

impl ConfigRequest {
    pub fn new(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: Some(workspace.into()),
        }
    }

    pub fn global() -> Self {
        Self { workspace: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSourceInfo {
    pub provider: String,
    pub priority: u16,
    pub location: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ConfigLayer {
    pub source: ConfigSourceInfo,
    pub patch: AgentConfigPatch,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub config: AgentConfig,
    pub sources: Vec<ConfigSourceInfo>,
}

impl std::fmt::Debug for ResolvedConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedConfig")
            .field("config", &self.config)
            .field("sources", &self.sources)
            .finish()
    }
}

impl ResolvedConfig {
    pub fn redacted(&self) -> Value {
        json!({"config": self.config.redacted(), "sources": self.sources})
    }
}
