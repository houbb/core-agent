use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::agent_directory;
use crate::{CliError, CliResult, LocalSessionState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub name: String,
    pub root: String,
    pub markers: BTreeSet<String>,
    pub languages: BTreeSet<String>,
    pub frameworks: BTreeSet<String>,
    pub build_systems: BTreeSet<String>,
    pub modules: Vec<String>,
    pub git_repository: bool,
    pub git_branch: Option<String>,
}

impl ProjectSnapshot {
    pub fn scan(root: &Path) -> CliResult<Self> {
        if !root.is_dir() {
            return Err(CliError::InvalidArgument(format!(
                "workspace {} is not a directory",
                root.display()
            )));
        }
        let canonical = root.canonicalize()?;
        let mut snapshot = Self {
            name: canonical
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("workspace")
                .to_owned(),
            root: canonical.to_string_lossy().into_owned(),
            markers: BTreeSet::new(),
            languages: BTreeSet::new(),
            frameworks: BTreeSet::new(),
            build_systems: BTreeSet::new(),
            modules: Vec::new(),
            git_repository: canonical.join(".git").exists(),
            git_branch: read_git_branch(&canonical),
        };
        for (marker, language, framework, build) in marker_definitions() {
            if canonical.join(marker).is_file() {
                snapshot.markers.insert((*marker).into());
                if let Some(value) = language {
                    snapshot.languages.insert((*value).into());
                }
                if let Some(value) = framework {
                    snapshot.frameworks.insert((*value).into());
                }
                if let Some(value) = build {
                    snapshot.build_systems.insert((*value).into());
                }
            }
        }
        let mut modules = fs::read_dir(&canonical)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_dir()
                    || entry.file_name() == ".git"
                    || entry.file_name() == ".agent"
                {
                    return None;
                }
                let path = entry.path();
                marker_definitions()
                    .iter()
                    .any(|(marker, _, _, _)| path.join(marker).is_file())
                    .then(|| entry.file_name().to_string_lossy().into_owned())
            })
            .take(128)
            .collect::<Vec<_>>();
        modules.sort();
        snapshot.modules = modules;
        Ok(snapshot)
    }
}

type MarkerDefinition = (
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
);

fn marker_definitions() -> &'static [MarkerDefinition] {
    &[
        ("Cargo.toml", Some("Rust"), None, Some("Cargo")),
        ("pom.xml", Some("Java"), Some("Spring/Maven"), Some("Maven")),
        ("build.gradle", Some("Java"), Some("Gradle"), Some("Gradle")),
        (
            "build.gradle.kts",
            Some("Kotlin"),
            Some("Gradle"),
            Some("Gradle"),
        ),
        (
            "package.json",
            Some("JavaScript/TypeScript"),
            None,
            Some("Node"),
        ),
        ("pyproject.toml", Some("Python"), None, Some("Python")),
        ("go.mod", Some("Go"), None, Some("Go Modules")),
        ("Dockerfile", None, Some("Container"), Some("Docker")),
        ("README.md", None, Some("Documentation"), None),
    ]
}

fn read_git_branch(root: &Path) -> Option<String> {
    let head = fs::read_to_string(root.join(".git/HEAD")).ok()?;
    let head = head.trim();
    let branch = head.strip_prefix("ref: refs/heads/")?;
    (!branch.is_empty() && branch.len() <= 512 && !branch.chars().any(char::is_control))
        .then(|| branch.to_owned())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub summary: String,
    pub usage: String,
    pub minimum_arguments: usize,
    pub maximum_arguments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    pub arguments: Vec<String>,
    #[serde(skip, default = "agent_route")]
    pub route: core_agent::InteractionCommandRoute,
}

pub struct CommandRegistry {
    commands: core_agent::InteractionCommandRegistry,
}

impl CommandRegistry {
    pub fn with_builtins() -> Self {
        Self {
            commands: core_agent::InteractionCommandRegistry::with_builtins(),
        }
    }

    pub fn register(&mut self, definition: CommandDefinition) -> CliResult<()> {
        self.commands
            .register(core_agent::InteractionCommandDefinition {
                name: definition.name,
                summary: definition.summary,
                usage: definition.usage,
                minimum_arguments: definition.minimum_arguments,
                maximum_arguments: definition.maximum_arguments,
                route: core_agent::InteractionCommandRoute::Agent,
            })
            .map_err(interaction_error)
    }

    pub fn parse(&self, line: &str) -> CliResult<CommandInvocation> {
        let invocation = self.commands.parse(line).map_err(interaction_error)?;
        Ok(CommandInvocation {
            name: invocation.name,
            arguments: invocation.arguments,
            route: invocation.route,
        })
    }

    pub fn complete(&self, prefix: &str) -> Vec<String> {
        self.commands.complete(prefix)
    }

    pub fn help(&self) -> Vec<CommandDefinition> {
        self.commands
            .help()
            .into_iter()
            .map(|definition| CommandDefinition {
                name: definition.name,
                summary: definition.summary,
                usage: definition.usage,
                minimum_arguments: definition.minimum_arguments,
                maximum_arguments: definition.maximum_arguments,
            })
            .collect()
    }

    pub fn execute_entry(
        &self,
        invocation: &CommandInvocation,
    ) -> CliResult<Option<core_agent::InteractionEntryOutcome>> {
        self.commands
            .execute_entry(&core_agent::InteractionCommandInvocation {
                name: invocation.name.clone(),
                arguments: invocation.arguments.clone(),
                route: invocation.route,
            })
            .map_err(interaction_error)
    }
}

fn agent_route() -> core_agent::InteractionCommandRoute {
    core_agent::InteractionCommandRoute::Agent
}

fn interaction_error(error: impl std::fmt::Display) -> CliError {
    CliError::InvalidArgument(error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileState {
    pub active: String,
}

impl Default for ProfileState {
    fn default() -> Self {
        Self {
            active: "coder".into(),
        }
    }
}

impl ProfileState {
    pub fn load(root: &Path) -> CliResult<Self> {
        let path = agent_directory(root).join("profile.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let state: Self = serde_json::from_slice(&fs::read(path)?)?;
        validate_profile(&state.active)?;
        Ok(state)
    }

    pub fn set(root: &Path, active: impl Into<String>) -> CliResult<Self> {
        let state = Self {
            active: active.into(),
        };
        validate_profile(&state.active)?;
        let directory = agent_directory(root);
        fs::create_dir_all(&directory)?;
        atomic_json(&directory.join("profile.json"), &state)?;
        Ok(state)
    }
}

fn validate_profile(value: &str) -> CliResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(CliError::InvalidArgument("profile name is invalid".into()));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TerminalHistory {
    pub commands: Vec<String>,
}

impl TerminalHistory {
    pub fn load(root: &Path) -> CliResult<Self> {
        let path = agent_directory(root).join("history.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        let history: Self = serde_json::from_slice(&fs::read(path)?)?;
        if history.commands.len() > 500 {
            return Err(CliError::Configuration(
                "terminal history exceeds 500 entries".into(),
            ));
        }
        Ok(history)
    }

    pub fn record_command(&mut self, root: &Path, line: &str) -> CliResult<()> {
        if !line.starts_with('/') || line.len() > 64 * 1024 || line.chars().any(char::is_control) {
            return Err(CliError::InvalidArgument(
                "only safe slash commands can be stored in history".into(),
            ));
        }
        self.commands.push(line.into());
        if self.commands.len() > 500 {
            self.commands.drain(..self.commands.len() - 500);
        }
        let directory = agent_directory(root);
        fs::create_dir_all(&directory)?;
        atomic_json(&directory.join("history.json"), self)
    }
}

fn atomic_json(path: &Path, value: &impl Serialize) -> CliResult<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfessionalRequest {
    pub invocation: CommandInvocation,
    pub profile: String,
    pub project: ProjectSnapshot,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfessionalResponse {
    pub summary: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub data: serde_json::Value,
}

#[async_trait]
pub trait ProfessionalAgentClient: Send + Sync {
    async fn index_project(
        &self,
        project: ProjectSnapshot,
        profile: &str,
    ) -> CliResult<ProfessionalResponse>;
    async fn execute_professional(
        &self,
        request: ProfessionalRequest,
    ) -> CliResult<ProfessionalResponse>;
}

pub struct ProfessionalApplication<C: ProfessionalAgentClient + ?Sized> {
    root: PathBuf,
    client: Arc<C>,
    commands: CommandRegistry,
}

impl<C: ProfessionalAgentClient + ?Sized> ProfessionalApplication<C> {
    pub fn new(root: impl Into<PathBuf>, client: Arc<C>) -> Self {
        Self {
            root: root.into(),
            client,
            commands: CommandRegistry::with_builtins(),
        }
    }

    pub fn commands(&self) -> &CommandRegistry {
        &self.commands
    }

    pub async fn execute_line(&self, line: &str) -> CliResult<Vec<String>> {
        let invocation = self.commands.parse(line)?;
        if let Some(entry) = self.commands.execute_entry(&invocation)? {
            let response = match entry.action {
                core_agent::InteractionEntryAction::NewSession => {
                    LocalSessionState::start_new(&self.root)?;
                    entry.response
                }
                core_agent::InteractionEntryAction::Profile(value) => {
                    let profile = if let Some(value) = value {
                        ProfileState::set(&self.root, value)?.active
                    } else {
                        ProfileState::load(&self.root)?.active
                    };
                    format!("Profile: {profile}")
                }
                _ => entry.response,
            };
            if invocation.name != "help" && invocation.name != "exit" {
                self.record(line)?;
            }
            return Ok(vec![response]);
        }
        let profile = ProfileState::load(&self.root)?.active;
        let project = ProjectSnapshot::scan(&self.root)?;
        let session_id = LocalSessionState::load(&self.root)?.current_session_id;
        let response = self
            .client
            .execute_professional(ProfessionalRequest {
                invocation,
                profile,
                project,
                session_id,
            })
            .await?;
        self.record(line)?;
        let mut lines = vec![response.summary];
        lines.extend(response.items);
        Ok(lines)
    }

    fn record(&self, line: &str) -> CliResult<()> {
        TerminalHistory::load(&self.root)?.record_command(&self.root, line)
    }
}
