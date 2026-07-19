use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{ConfigError, ConfigResult};

pub const CONFIG_SCHEMA_VERSION: u32 = 2;
pub const LEGACY_CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_CONTEXT_TOKENS: u64 = 128_000;

#[derive(Clone, PartialEq, Eq)]
pub struct AgentConfig {
    pub version: u32,
    pub active_model: String,
    pub models: Vec<ConfigModel>,
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
            active_model: ConfigModel::default().name.clone(),
            models: vec![ConfigModel::default()],
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
        if !matches!(
            self.version,
            LEGACY_CONFIG_SCHEMA_VERSION | CONFIG_SCHEMA_VERSION
        ) {
            return Err(ConfigError::Validation(format!(
                "unsupported schema version {}; expected {LEGACY_CONFIG_SCHEMA_VERSION} or {CONFIG_SCHEMA_VERSION}",
                self.version
            )));
        }
        validate_text("active model", &self.active_model, 256)?;
        if self.models.is_empty() || self.models.len() > 64 {
            return Err(ConfigError::Validation(
                "models must contain between 1 and 64 entries".into(),
            ));
        }
        let mut names = BTreeSet::new();
        for model in &self.models {
            model.validate()?;
            let normalized = model.name.trim().to_ascii_lowercase();
            if !names.insert(normalized) {
                return Err(ConfigError::Validation(
                    "model names must be unique (case-insensitive)".into(),
                ));
            }
        }
        if !self
            .models
            .iter()
            .any(|model| model.name == self.active_model)
        {
            return Err(ConfigError::Validation(format!(
                "active model {} is not configured",
                self.active_model
            )));
        }
        self.model.validate()?;
        validate_text("permission mode", &self.permissions.mode, 32)?;
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
            || !matches!(
                self.context.compression.strategy.as_str(),
                "recent-window" | "extractive-summary"
            )
            || !(1..=100).contains(&self.context.compression.trigger_percent)
            || self.context.compression.keep_recent_messages == 0
            || self.context.compression.keep_recent_messages > 10_000
        {
            return Err(ConfigError::Validation(
                "model, permission or context limits are invalid".into(),
            ));
        }
        Ok(())
    }

    pub fn redacted(&self) -> Value {
        json!({
            "version": self.version,
            "activeModel": self.active_model,
            "models": self.models.iter().map(ConfigModel::redacted).collect::<Vec<_>>(),
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
                "compression": self.context.compression,
            }
        })
    }

    pub(crate) fn apply(&mut self, patch: AgentConfigPatch) {
        let AgentConfigPatch {
            version,
            active_model,
            models,
            model,
            permissions,
            memory,
            session,
            context,
            server: _,
            workspace: _,
        } = patch;
        let selection_changed = models.is_some() || active_model.is_some();
        if let Some(version) = version {
            self.version = version;
        }
        if let Some(mut models) = models {
            for model in &mut models {
                model.normalize();
            }
            self.models = models;
        }
        if let Some(active_model) = active_model {
            self.active_model = active_model.trim().to_owned();
        }
        if selection_changed {
            if let Some(active) = self
                .models
                .iter()
                .find(|model| model.name == self.active_model)
            {
                self.model = active.clone();
            }
        }
        if let Some(model) = model {
            model.apply(&mut self.model);
        }
        if let Some(permissions) = permissions {
            permissions.apply(&mut self.permissions);
        }
        if let Some(memory) = memory {
            memory.apply(&mut self.memory);
        }
        if let Some(session) = session {
            session.apply(&mut self.session);
        }
        if let Some(context) = context {
            context.apply(&mut self.context);
        }
    }

    pub(crate) fn normalize_legacy(&mut self) {
        if self.version == LEGACY_CONFIG_SCHEMA_VERSION {
            self.model.normalize();
            self.active_model = self.model.name.clone();
            self.models = vec![self.model.clone()];
            self.version = CONFIG_SCHEMA_VERSION;
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

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigModel {
    #[serde(default = "default_model_provider")]
    pub provider: String,
    #[serde(rename = "baseURL", alias = "endpoint")]
    pub endpoint: String,
    pub name: String,
    #[serde(default)]
    pub profile: String,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    #[serde(default, alias = "api_key")]
    pub api_key: Option<String>,
    #[serde(default, alias = "api_key_ref")]
    pub api_key_ref: Option<String>,
}

impl Default for ConfigModel {
    fn default() -> Self {
        Self {
            provider: "deepseek".into(),
            endpoint: "https://api.deepseek.com".into(),
            name: "deepseek-v4-flash".into(),
            profile: "default".into(),
            max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
            api_key: None,
            api_key_ref: None,
        }
    }
}

fn default_model_provider() -> String {
    "openai-compatible".into()
}

fn default_max_context_tokens() -> u64 {
    DEFAULT_MAX_CONTEXT_TOKENS
}

impl ConfigModel {
    pub(crate) fn normalize(&mut self) {
        self.provider = self.provider.trim().to_owned();
        self.endpoint = self.endpoint.trim().to_owned();
        self.name = self.name.trim().to_owned();
        if self.profile.trim().is_empty() {
            self.profile = self.name.clone();
        } else {
            self.profile = self.profile.trim().to_owned();
        }
    }

    fn validate(&self) -> ConfigResult<()> {
        for (label, value, maximum) in [
            ("model provider", self.provider.as_str(), 128),
            ("model baseURL", self.endpoint.as_str(), 2_048),
            ("model name", self.name.as_str(), 256),
            ("model profile", self.profile.as_str(), 256),
        ] {
            validate_text(label, value, maximum)?;
        }
        if !(self.endpoint.starts_with("http://") || self.endpoint.starts_with("https://"))
            || self.max_context_tokens == 0
            || self.max_context_tokens > 10_000_000
            || self.api_key.is_some() && self.api_key_ref.is_some()
            || self.api_key.as_ref().is_some_and(|value| {
                value.is_empty() || value.len() > 16 * 1024 || value.chars().any(char::is_control)
            })
            || self.api_key_ref.as_ref().is_some_and(|value| {
                value.is_empty() || value.len() > 512 || value.chars().any(char::is_control)
            })
        {
            return Err(ConfigError::Validation(format!(
                "model {} is invalid",
                self.name
            )));
        }
        Ok(())
    }

    pub fn redacted(&self) -> Value {
        json!({
            "provider": self.provider,
            "baseURL": self.endpoint,
            "name": self.name,
            "profile": self.profile,
            "maxContextTokens": self.max_context_tokens,
            "apiKeyConfigured": self.api_key.is_some(),
            "apiKeyRef": self.api_key_ref,
        })
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
    #[serde(default)]
    pub compression: ConfigCompression,
}

impl Default for ConfigContext {
    fn default() -> Self {
        Self {
            max_mentions: 16,
            max_files: 128,
            max_file_bytes: 256 * 1024,
            max_total_bytes: 1024 * 1024,
            max_directory_depth: 8,
            compression: ConfigCompression::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigCompression {
    #[serde(default = "default_compression_strategy")]
    pub strategy: String,
    #[serde(default = "default_compression_trigger")]
    pub trigger_percent: u8,
    #[serde(default = "default_keep_recent_messages")]
    pub keep_recent_messages: usize,
}

impl Default for ConfigCompression {
    fn default() -> Self {
        Self {
            strategy: default_compression_strategy(),
            trigger_percent: default_compression_trigger(),
            keep_recent_messages: default_keep_recent_messages(),
        }
    }
}

fn default_compression_strategy() -> String {
    "recent-window".into()
}

fn default_compression_trigger() -> u8 {
    80
}

fn default_keep_recent_messages() -> usize {
    20
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfigPatch {
    pub version: Option<u32>,
    pub active_model: Option<String>,
    pub models: Option<Vec<ConfigModel>>,
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
            active_model: Some(config.active_model),
            models: Some(config.models),
            model: None,
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
                compression: Some(ConfigCompressionPatch {
                    strategy: Some(config.context.compression.strategy),
                    trigger_percent: Some(config.context.compression.trigger_percent),
                    keep_recent_messages: Some(config.context.compression.keep_recent_messages),
                }),
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
    #[serde(rename = "baseURL", alias = "endpoint")]
    pub endpoint: Option<String>,
    pub name: Option<String>,
    pub profile: Option<String>,
    pub max_context_tokens: Option<u64>,
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
        if let Some(value) = self.max_context_tokens {
            target.max_context_tokens = value;
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
    pub compression: Option<ConfigCompressionPatch>,
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
        if let Some(value) = self.compression {
            value.apply(&mut target.compression);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigCompressionPatch {
    pub strategy: Option<String>,
    pub trigger_percent: Option<u8>,
    pub keep_recent_messages: Option<usize>,
}

impl ConfigCompressionPatch {
    fn apply(self, target: &mut ConfigCompression) {
        if let Some(value) = self.strategy {
            target.strategy = value;
        }
        if let Some(value) = self.trigger_percent {
            target.trigger_percent = value;
        }
        if let Some(value) = self.keep_recent_messages {
            target.keep_recent_messages = value;
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
