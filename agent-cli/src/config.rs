use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{CliError, CliResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CliConfig {
    pub server: ServerConfig,
    pub model: ModelConfig,
    pub workspace: WorkspaceConfig,
    pub memory: MemoryConfig,
    #[serde(default)]
    pub permissions: PermissionsConfig,
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
    #[serde(default)]
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

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                mode: default_server_mode(),
                url: None,
            },
            model: ModelConfig {
                provider: "ollama".into(),
                endpoint: default_model_endpoint(),
                name: default_model_name(),
                profile: default_model_profile(),
                api_key_env: None,
            },
            workspace: WorkspaceConfig { root: ".".into() },
            memory: MemoryConfig { enabled: true },
            permissions: PermissionsConfig::default(),
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
        let config = Self::default();
        fs::write(&config_path, serde_yaml::to_string(&config)?)?;
        fs::write(
            directory.join("context.yaml"),
            "version: 1\ninclude: []\nexclude: []\n",
        )?;
        Ok(config)
    }

    pub fn load(root: &Path) -> CliResult<Self> {
        let path = agent_directory(root).join("config.yaml");
        let config: Self = serde_yaml::from_str(&fs::read_to_string(&path).map_err(|error| {
            CliError::Configuration(format!("cannot read {}: {error}", path.display()))
        })?)?;
        config.validate()?;
        Ok(config)
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
            || self.workspace.root.trim().is_empty()
            || !matches!(
                self.permissions.mode.as_str(),
                "strict" | "risk-based" | "auto"
            )
        {
            return Err(CliError::Configuration(
                "server, model or workspace configuration is invalid".into(),
            ));
        }
        if self.model.api_key_env.as_ref().is_some_and(|name| {
            name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }) {
            return Err(CliError::Configuration(
                "model api_key_env must be a valid environment variable name".into(),
            ));
        }
        Ok(())
    }
}

fn default_server_mode() -> String {
    "embedded".into()
}

fn default_model_endpoint() -> String {
    "http://127.0.0.1:11434/v1".into()
}

fn default_model_name() -> String {
    "qwen3".into()
}

fn default_model_profile() -> String {
    "default".into()
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

    pub fn record(&mut self, root: &Path, session_id: Uuid) -> CliResult<()> {
        self.current_session_id = Some(session_id);
        self.recent_session_ids.retain(|value| *value != session_id);
        self.recent_session_ids.insert(0, session_id);
        self.recent_session_ids.truncate(100);
        let path = state_path(root);
        fs::create_dir_all(path.parent().unwrap_or(root))?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn resolve(&self, explicit: Option<Uuid>) -> CliResult<Uuid> {
        explicit
            .or(self.current_session_id)
            .ok_or(CliError::NoSession)
    }
}

pub fn agent_directory(root: &Path) -> PathBuf {
    root.join(".agent")
}

fn state_path(root: &Path) -> PathBuf {
    agent_directory(root).join("sessions.json")
}
