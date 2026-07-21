use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::{SlashError, SlashResult};

/// Command category for organization and help display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlashCategory {
    System,
    Session,
    Context,
    Project,
    Memory,
    Agent,
    Cognitive,
    Checkpoint,
    Governance,
    Observability,
    Society,
    Developer,
    Workflow,
    Orchestration,
}

impl SlashCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Session => "session",
            Self::Context => "context",
            Self::Project => "project",
            Self::Memory => "memory",
            Self::Agent => "agent",
            Self::Cognitive => "cognitive",
            Self::Checkpoint => "checkpoint",
            Self::Governance => "governance",
            Self::Observability => "observability",
            Self::Society => "society",
            Self::Developer => "developer",
            Self::Workflow => "workflow",
            Self::Orchestration => "orchestration",
        }
    }
}

/// Command metadata for registration and help display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMetadata {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub usage: String,
    pub category: SlashCategory,
    pub min_args: usize,
    pub max_args: usize,
    pub read_only: bool,
    pub async_exec: bool,
}

/// Execution context for a slash command.
#[derive(Debug, Clone)]
pub struct CommandContext {
    pub line: String,
    pub args: Vec<String>,
    pub workspace: String,
    pub session_id: Option<String>,
    pub data: HashMap<String, String>,
}

impl CommandContext {
    pub fn new(line: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            line: line.into(),
            args,
            workspace: String::new(),
            session_id: None,
            data: HashMap::new(),
        }
    }
}

/// Output from a slash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    pub response: String,
    pub data: serde_json::Value,
    pub action: CommandAction,
}

impl CommandOutput {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            data: serde_json::Value::Null,
            action: CommandAction::None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    pub fn with_action(mut self, action: CommandAction) -> Self {
        self.action = action;
        self
    }
}

/// Action to take after command execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandAction {
    None,
    NewSession,
    ClearView,
    Exit,
}

/// A parsed slash command.
#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub name: String,
    pub args: Vec<String>,
}

/// The unified slash command trait.
#[async_trait]
pub trait SlashCommand: Send + Sync {
    fn metadata(&self) -> CommandMetadata;

    fn category(&self) -> SlashCategory;

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        let meta = self.metadata();
        if args.len() < meta.min_args || args.len() > meta.max_args {
            return Err(SlashError::InvalidArgument(format!(
                "usage: {}",
                meta.usage
            )));
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput>;

    async fn on_register(&self) -> SlashResult<()> {
        Ok(())
    }

    async fn on_destroy(&self) -> SlashResult<()> {
        Ok(())
    }
}

/// Observer for slash command lifecycle events.
#[async_trait]
pub trait SlashCommandObserver: Send + Sync {
    async fn on_command_start(&self, ctx: &CommandContext);
    async fn on_command_success(&self, ctx: &CommandContext, output: &CommandOutput);
    async fn on_command_failure(&self, ctx: &CommandContext, error: &SlashError);
}

/// Parse a slash command line into a ParsedCommand.
pub fn parse_command(line: &str) -> SlashResult<ParsedCommand> {
    if !line.starts_with('/') {
        return Err(SlashError::InvalidArgument(
            "command must start with /".into(),
        ));
    }
    let parts = tokenize(&line[1..])?;
    let name = parts
        .first()
        .ok_or_else(|| SlashError::InvalidArgument("command name is required".into()))?
        .clone();
    let args = parts[1..].to_vec();
    Ok(ParsedCommand { name, args })
}

fn tokenize(value: &str) -> SlashResult<Vec<String>> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\'' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                output.push(std::mem::take(&mut current));
            }
        } else if character.is_control() {
            return Err(SlashError::InvalidArgument(
                "command contains control characters".into(),
            ));
        } else {
            current.push(character);
        }
    }
    if quoted || escaped {
        return Err(SlashError::InvalidArgument(
            "command has an unterminated quote".into(),
        ));
    }
    if !current.is_empty() {
        output.push(current);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_commands() {
        let cmd = parse_command("/help").unwrap();
        assert_eq!(cmd.name, "help");
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn parse_command_with_args() {
        let cmd = parse_command("/review src/main.rs").unwrap();
        assert_eq!(cmd.name, "review");
        assert_eq!(cmd.args, vec!["src/main.rs"]);
    }

    #[test]
    fn parse_command_with_quoted_args() {
        let cmd = parse_command("/say \"hello world\"").unwrap();
        assert_eq!(cmd.name, "say");
        assert_eq!(cmd.args, vec!["hello world"]);
    }

    #[test]
    fn parse_rejects_non_slash() {
        assert!(parse_command("help").is_err());
    }

    #[test]
    fn parse_rejects_control_characters() {
        assert!(parse_command("/test\u{0}").is_err());
    }
}