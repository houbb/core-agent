use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    parse_command, CommandContext, CommandMetadata, CommandOutput,
    ParsedCommand, SlashCommand, SlashCommandObserver,
};
use crate::error::{SlashError, SlashResult};

/// A registry for slash commands with observer support.
pub struct SlashCommandRegistry {
    commands: BTreeMap<String, Arc<dyn SlashCommand>>,
    observers: Vec<Arc<dyn SlashCommandObserver>>,
}

impl SlashCommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: BTreeMap::new(),
            observers: Vec::new(),
        }
    }

    /// Register a slash command.
    pub fn register(&mut self, command: Arc<dyn SlashCommand>) -> SlashResult<()> {
        let meta = command.metadata();
        if self.commands.contains_key(&meta.name) {
            return Err(SlashError::InvalidArgument(format!(
                "command /{} is already registered",
                meta.name
            )));
        }
        self.commands.insert(meta.name.clone(), command);
        Ok(())
    }

    /// Unregister a slash command by name.
    pub fn unregister(&mut self, name: &str) -> SlashResult<()> {
        self.commands
            .remove(name)
            .ok_or_else(|| SlashError::NotFound(format!("command /{name} not found")))?;
        Ok(())
    }

    /// Add an observer.
    pub fn add_observer(&mut self, observer: Arc<dyn SlashCommandObserver>) {
        self.observers.push(observer);
    }

    /// Parse and execute a command line.
    pub async fn execute_line(&self, line: &str) -> SlashResult<CommandOutput> {
        let parsed = parse_command(line)?;
        let ctx = CommandContext::new(line, parsed.args.clone());
        self.execute(&parsed, ctx).await
    }

    /// Execute a parsed command.
    pub async fn execute(
        &self,
        parsed: &ParsedCommand,
        ctx: CommandContext,
    ) -> SlashResult<CommandOutput> {
        for observer in &self.observers {
            observer.on_command_start(&ctx).await;
        }

        let result = match self.commands.get(&parsed.name) {
            Some(command) => {
                command.validate(&parsed.args).await?;
                command.execute(ctx.clone()).await
            }
            None => Err(SlashError::NotFound(format!(
                "unknown command /{}",
                parsed.name
            ))),
        };

        match &result {
            Ok(output) => {
                for observer in &self.observers {
                    observer.on_command_success(&ctx, output).await;
                }
            }
            Err(error) => {
                for observer in &self.observers {
                    observer.on_command_failure(&ctx, error).await;
                }
            }
        }

        result
    }

    /// Get command completion suggestions for a prefix.
    pub fn complete(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim_start_matches('/');
        let mut names: Vec<String> = self
            .commands
            .keys()
            .filter(|name| name.starts_with(prefix))
            .map(|name| format!("/{name}"))
            .collect();
        names.sort();
        names
    }

    /// Get all command metadata for help display.
    pub fn help(&self) -> Vec<CommandMetadata> {
        let mut commands: Vec<CommandMetadata> =
            self.commands.values().map(|cmd| cmd.metadata()).collect();
        commands.sort_by(|a, b| a.name.cmp(&b.name));
        commands
    }

    /// Get a command by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn SlashCommand>> {
        self.commands.get(name).cloned()
    }

    /// Check if a command is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for SlashCommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Noop observer ──

#[allow(dead_code)]
pub struct NoopSlashCommandObserver;

#[async_trait]
impl SlashCommandObserver for NoopSlashCommandObserver {
    async fn on_command_start(&self, _ctx: &CommandContext) {}
    async fn on_command_success(&self, _ctx: &CommandContext, _output: &CommandOutput) {}
    async fn on_command_failure(&self, _ctx: &CommandContext, _error: &SlashError) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::SlashCategory;

    struct EchoCommand;

    #[async_trait]
    impl SlashCommand for EchoCommand {
        fn metadata(&self) -> CommandMetadata {
            CommandMetadata {
                name: "echo".into(),
                display_name: "Echo".into(),
                description: "Echo back arguments".into(),
                usage: "/echo <text>".into(),
                category: SlashCategory::System,
                min_args: 1,
                max_args: 10,
                read_only: true,
                async_exec: false,
            }
        }

        fn category(&self) -> SlashCategory {
            SlashCategory::System
        }

        async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
            Ok(CommandOutput::new(ctx.args.join(" ")))
        }
    }

    #[tokio::test]
    async fn registry_echoes_and_provides_help() {
        let mut registry = SlashCommandRegistry::new();
        registry.register(Arc::new(EchoCommand)).unwrap();

        assert!(registry.contains("echo"));
        assert_eq!(registry.help().len(), 1);

        let output = registry.execute_line("/echo hello world").await.unwrap();
        assert_eq!(output.response, "hello world");
    }

    #[tokio::test]
    async fn registry_rejects_unknown_commands() {
        let registry = SlashCommandRegistry::new();
        let result = registry.execute_line("/unknown").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SlashError::NotFound(_)));
    }

    #[tokio::test]
    async fn registry_completes_commands() {
        let mut registry = SlashCommandRegistry::new();
        registry.register(Arc::new(EchoCommand)).unwrap();
        let completions = registry.complete("/ec");
        assert_eq!(completions, vec!["/echo"]);
    }
}