use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::agent_directory;
use crate::{CliError, CliResult};

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

impl CommandDefinition {
    fn validate(&self) -> CliResult<()> {
        if self.name.is_empty()
            || self.name.len() > 64
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
            || self.summary.trim().is_empty()
            || self.summary.len() > 256
            || self.usage.len() > 256
            || self.maximum_arguments > 32
            || self.minimum_arguments > self.maximum_arguments
        {
            return Err(CliError::InvalidArgument(
                "command definition is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    pub arguments: Vec<String>,
}

#[derive(Default)]
pub struct CommandRegistry {
    commands: BTreeMap<String, CommandDefinition>,
}

impl CommandRegistry {
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        for (name, summary, usage, minimum, maximum) in [
            (
                "project",
                "Index and describe the project",
                "/project",
                0,
                0,
            ),
            (
                "profile",
                "Show or switch Agent profile",
                "/profile [name]",
                0,
                1,
            ),
            ("tasks", "List active tasks", "/tasks", 0, 0),
            (
                "history",
                "Inspect project history",
                "/history [query]",
                0,
                1,
            ),
            (
                "review",
                "Review the current change",
                "/review [target]",
                0,
                1,
            ),
            (
                "plan",
                "Create an implementation plan",
                "/plan <goal>",
                1,
                32,
            ),
            (
                "explain",
                "Explain project code",
                "/explain <target>",
                1,
                32,
            ),
            ("test", "Run or plan tests", "/test [target]", 0, 32),
            ("fix", "Fix the current issue", "/fix [target]", 0, 32),
            ("refactor", "Refactor a target", "/refactor <target>", 1, 32),
            ("commit", "Generate a commit proposal", "/commit", 0, 0),
            ("pr", "Generate a pull request proposal", "/pr", 0, 0),
            ("config", "Show effective configuration", "/config", 0, 0),
            ("status", "Show current status", "/status", 0, 0),
            ("tools", "List available tools", "/tools", 0, 0),
            ("memory", "Show project memory", "/memory", 0, 0),
        ] {
            registry
                .register(CommandDefinition {
                    name: name.into(),
                    summary: summary.into(),
                    usage: usage.into(),
                    minimum_arguments: minimum,
                    maximum_arguments: maximum,
                })
                .expect("built-in command must be valid");
        }
        registry
    }

    pub fn register(&mut self, definition: CommandDefinition) -> CliResult<()> {
        definition.validate()?;
        if self.commands.contains_key(&definition.name) {
            return Err(CliError::InvalidArgument(format!(
                "command /{} is already registered",
                definition.name
            )));
        }
        self.commands.insert(definition.name.clone(), definition);
        Ok(())
    }

    pub fn parse(&self, line: &str) -> CliResult<CommandInvocation> {
        if !line.starts_with('/') || line.len() > 64 * 1024 {
            return Err(CliError::InvalidArgument(
                "professional command must start with /".into(),
            ));
        }
        let parts = tokenize(&line[1..])?;
        let Some(name) = parts.first() else {
            return Err(CliError::InvalidArgument("command name is required".into()));
        };
        let definition = self
            .commands
            .get(name)
            .ok_or_else(|| CliError::InvalidArgument(format!("unknown command /{name}")))?;
        let arguments = parts[1..].to_vec();
        if arguments.len() < definition.minimum_arguments
            || arguments.len() > definition.maximum_arguments
        {
            return Err(CliError::InvalidArgument(format!(
                "usage: {}",
                definition.usage
            )));
        }
        Ok(CommandInvocation {
            name: name.clone(),
            arguments,
        })
    }

    pub fn complete(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim_start_matches('/');
        self.commands
            .keys()
            .filter(|name| name.starts_with(prefix))
            .map(|name| format!("/{name}"))
            .collect()
    }

    pub fn help(&self) -> Vec<CommandDefinition> {
        self.commands.values().cloned().collect()
    }
}

fn tokenize(value: &str) -> CliResult<Vec<String>> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                output.push(std::mem::take(&mut current));
            }
        } else if character.is_control() {
            return Err(CliError::InvalidArgument(
                "command contains control characters".into(),
            ));
        } else {
            current.push(character);
        }
    }
    if quoted || escaped {
        return Err(CliError::InvalidArgument(
            "command has an unterminated quote".into(),
        ));
    }
    if !current.is_empty() {
        output.push(current);
    }
    Ok(output)
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
        if invocation.name == "profile" {
            let profile = if let Some(value) = invocation.arguments.first() {
                ProfileState::set(&self.root, value)?.active
            } else {
                ProfileState::load(&self.root)?.active
            };
            self.record(line)?;
            return Ok(vec![format!("Profile: {profile}")]);
        }
        let profile = ProfileState::load(&self.root)?.active;
        let project = ProjectSnapshot::scan(&self.root)?;
        let response = if invocation.name == "project" {
            self.client.index_project(project, &profile).await?
        } else {
            self.client
                .execute_professional(ProfessionalRequest {
                    invocation,
                    profile,
                    project,
                })
                .await?
        };
        self.record(line)?;
        let mut lines = vec![response.summary];
        lines.extend(response.items);
        Ok(lines)
    }

    fn record(&self, line: &str) -> CliResult<()> {
        TerminalHistory::load(&self.root)?.record_command(&self.root, line)
    }
}
