use std::fs;
use std::path::{Path, PathBuf};

use core_agent::{standard_config_manager, ConfigManager, ConfigRequest, ConfigSourceInfo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{CliError, CliResult};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliConfig {
    pub server: ServerConfig,
    pub model: ModelConfig,
    pub workspace: WorkspaceConfig,
    pub memory: MemoryConfig,
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub session: SessionConfig,
    #[serde(default)]
    pub context: ContextConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<ConfigSourceInfo>,
    #[serde(skip)]
    api_key: Option<String>,
}

impl std::fmt::Debug for CliConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CliConfig")
            .field("redacted", &self.redacted())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_server_mode")]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String,
    #[serde(default = "default_model_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_model_name")]
    pub name: String,
    #[serde(default = "default_model_profile")]
    pub profile: String,
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionsConfig {
    pub mode: String,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            mode: "risk-based".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub resume_last: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextConfig {
    pub max_mentions: usize,
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_directory_depth: usize,
    #[serde(default = "default_compression_strategy")]
    pub compression_strategy: String,
    #[serde(default = "default_compression_trigger_percent")]
    pub compression_trigger_percent: u8,
    #[serde(default = "default_keep_recent_messages")]
    pub keep_recent_messages: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_mentions: 16,
            max_files: 128,
            max_file_bytes: 256 * 1024,
            max_total_bytes: 1024 * 1024,
            max_directory_depth: 8,
            compression_strategy: default_compression_strategy(),
            compression_trigger_percent: default_compression_trigger_percent(),
            keep_recent_messages: default_keep_recent_messages(),
        }
    }
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                mode: default_server_mode(),
                url: None,
            },
            model: ModelConfig {
                provider: "deepseek".into(),
                endpoint: default_model_endpoint(),
                name: default_model_name(),
                profile: default_model_profile(),
                max_context_tokens: default_max_context_tokens(),
                api_key_env: None,
            },
            workspace: WorkspaceConfig { root: ".".into() },
            memory: MemoryConfig { enabled: true },
            permissions: PermissionsConfig::default(),
            session: SessionConfig::default(),
            context: ContextConfig::default(),
            sources: Vec::new(),
            api_key: None,
        }
    }
}

impl CliConfig {
    pub fn initialize(root: &Path) -> CliResult<Self> {
        let directory = agent_directory(root);
        let config_path = directory.join("config.yaml");
        if config_path.exists() {
            return Err(CliError::Configuration(format!(
                "{} already exists",
                config_path.display()
            )));
        }
        fs::create_dir_all(directory.join("memory"))?;
        let project = ProjectEntryConfig {
            server: Some(ServerConfig {
                mode: default_server_mode(),
                url: None,
            }),
            workspace: Some(WorkspaceConfig { root: ".".into() }),
        };
        fs::write(&config_path, serde_yaml::to_string(&project)?)?;
        fs::write(
            directory.join("context.yaml"),
            "version: 1\ninclude: []\nexclude: []\n",
        )?;
        Ok(Self::default())
    }

    pub async fn load(root: &Path) -> CliResult<Self> {
        let manager = standard_config_manager().map_err(config_error)?;
        Self::resolve(root, &manager).await
    }

    pub async fn resolve(root: &Path, manager: &ConfigManager) -> CliResult<Self> {
        let resolved = manager
            .resolve(&ConfigRequest::new(root))
            .await
            .map_err(config_error)?;
        let entry = load_project_entry(root)?;
        let config = Self {
            server: entry.server.unwrap_or_else(|| Self::default().server),
            workspace: entry.workspace.unwrap_or_else(|| Self::default().workspace),
            model: ModelConfig {
                provider: resolved.config.model.provider.clone(),
                endpoint: resolved.config.model.endpoint.clone(),
                name: resolved.config.model.name.clone(),
                profile: resolved.config.model.profile.clone(),
                max_context_tokens: resolved.config.model.max_context_tokens,
                api_key_env: resolved
                    .config
                    .model
                    .api_key_ref
                    .as_deref()
                    .and_then(|value| value.strip_prefix("env:"))
                    .map(str::to_owned),
            },
            memory: MemoryConfig {
                enabled: resolved.config.memory.enabled,
            },
            permissions: PermissionsConfig {
                mode: resolved.config.permissions.mode.clone(),
            },
            session: SessionConfig {
                resume_last: resolved.config.session.resume_last,
            },
            context: ContextConfig {
                max_mentions: resolved.config.context.max_mentions,
                max_files: resolved.config.context.max_files,
                max_file_bytes: resolved.config.context.max_file_bytes,
                max_total_bytes: resolved.config.context.max_total_bytes,
                max_directory_depth: resolved.config.context.max_directory_depth,
                compression_strategy: resolved.config.context.compression.strategy.clone(),
                compression_trigger_percent: resolved.config.context.compression.trigger_percent,
                keep_recent_messages: resolved.config.context.compression.keep_recent_messages,
            },
            sources: resolved.sources,
            api_key: resolved.config.model.api_key,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone()
    }

    pub fn redacted(&self) -> Value {
        json!({
            "server": self.server,
            "model": {
                "provider": self.model.provider,
                "endpoint": self.model.endpoint,
                "name": self.model.name,
                "profile": self.model.profile,
                "maxContextTokens": self.model.max_context_tokens,
                "apiKeyConfigured": self.api_key.is_some(),
                "apiKeyEnv": self.model.api_key_env,
            },
            "workspace": self.workspace,
            "memory": self.memory,
            "permissions": self.permissions,
            "session": self.session,
            "context": self.context,
            "sources": self.sources,
        })
    }

    pub fn validate(&self) -> CliResult<()> {
        let url = self
            .server
            .url
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/');
        if !matches!(self.server.mode.as_str(), "embedded" | "remote")
            || (self.server.mode == "remote"
                && (!(url.starts_with("http://") || url.starts_with("https://"))
                    || url.len() > 2048))
            || self.model.provider.trim().is_empty()
            || self.model.provider.len() > 128
            || self.model.endpoint.trim().is_empty()
            || self.model.name.trim().is_empty()
            || self.model.profile.trim().is_empty()
            || self.model.max_context_tokens == 0
            || self.workspace.root.trim().is_empty()
            || !matches!(
                self.permissions.mode.as_str(),
                "strict" | "risk-based" | "auto"
            )
            || self.context.max_mentions == 0
            || self.context.max_files == 0
            || self.context.max_total_bytes < self.context.max_file_bytes
            || self.context.max_directory_depth == 0
            || !matches!(
                self.context.compression_strategy.as_str(),
                "recent-window" | "extractive-summary"
            )
            || !(1..=100).contains(&self.context.compression_trigger_percent)
            || self.context.keep_recent_messages == 0
        {
            return Err(CliError::Configuration(
                "server, model, workspace or context configuration is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectEntryConfig {
    #[serde(default)]
    server: Option<ServerConfig>,
    #[serde(default)]
    workspace: Option<WorkspaceConfig>,
}

fn load_project_entry(root: &Path) -> CliResult<ProjectEntryConfig> {
    let directory = agent_directory(root);
    let paths = ["config.yaml", "config.yml", "config.json"]
        .into_iter()
        .map(|name| directory.join(name))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    let Some(path) = paths.first() else {
        return Ok(ProjectEntryConfig::default());
    };
    if paths.len() > 1 {
        return Err(CliError::Configuration(format!(
            "project configuration is ambiguous: {}",
            paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let bytes = fs::read(path)?;
    match path.extension().and_then(|value| value.to_str()) {
        Some("json") => serde_json::from_slice(&bytes).map_err(CliError::from),
        _ => serde_yaml::from_slice(&bytes).map_err(CliError::from),
    }
}

fn config_error(error: impl std::fmt::Display) -> CliError {
    CliError::Configuration(error.to_string())
}

fn default_server_mode() -> String {
    "embedded".into()
}

fn default_model_endpoint() -> String {
    "https://api.deepseek.com".into()
}

fn default_model_name() -> String {
    "deepseek-v4-flash".into()
}

fn default_model_profile() -> String {
    "default".into()
}

fn default_max_context_tokens() -> u64 {
    128_000
}

fn default_compression_strategy() -> String {
    "recent-window".into()
}

fn default_compression_trigger_percent() -> u8 {
    80
}

fn default_keep_recent_messages() -> usize {
    20
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSessionState {
    pub current_session_id: Option<Uuid>,
    pub recent_session_ids: Vec<Uuid>,
}

impl LocalSessionState {
    pub fn load(root: &Path) -> CliResult<Self> {
        let path = state_path(root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let state: Self = serde_json::from_slice(&fs::read(path)?)?;
        if state.recent_session_ids.len() > 100 {
            return Err(CliError::Configuration(
                "local session history exceeds 100 entries".into(),
            ));
        }
        Ok(state)
    }

    pub fn start_new(root: &Path) -> CliResult<Self> {
        let mut state = Self::load(root)?;
        state.current_session_id = None;
        state.persist(root)?;
        Ok(state)
    }

    pub fn record(&mut self, root: &Path, session_id: Uuid) -> CliResult<()> {
        self.current_session_id = Some(session_id);
        self.recent_session_ids.retain(|value| *value != session_id);
        self.recent_session_ids.insert(0, session_id);
        self.recent_session_ids.truncate(100);
        self.persist(root)
    }

    pub fn resolve(&self, explicit: Option<Uuid>) -> CliResult<Uuid> {
        explicit
            .or(self.current_session_id)
            .ok_or(CliError::NoSession)
    }

    fn persist(&self, root: &Path) -> CliResult<()> {
        let path = state_path(root);
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }
}

pub fn agent_directory(root: &Path) -> PathBuf {
    root.join(".agent")
}

fn state_path(root: &Path) -> PathBuf {
    agent_directory(root).join("sessions.json")
}
