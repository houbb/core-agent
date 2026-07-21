use std::sync::Arc;

use async_trait::async_trait;
use core_agent_subagent::SubAgentManager;
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct SubAgentStatusCommand {
    manager: Arc<SubAgentManager>,
}

impl SubAgentStatusCommand {
    pub fn new(manager: Arc<SubAgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for SubAgentStatusCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "subagent".into(),
            display_name: "SubAgent Status".into(),
            description: "查看子 Agent 状态".into(),
            usage: "/subagent status <id>".into(),
            category: SlashCategory::Orchestration,
            min_args: 2,
            max_args: 2,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() != 2 || args[0] != "status" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /subagent status <id>".into(),
            ));
        }
        Uuid::parse_str(&args[1])
            .map_err(|_| crate::slash::SlashError::InvalidArgument("invalid UUID".into()))?;
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let id = Uuid::parse_str(&ctx.args[1]).unwrap();
        let instance = self
            .manager
            .find(id)
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?
            .ok_or_else(|| crate::slash::SlashError::NotFound(format!("subagent {id} not found")))?;

        let lines = vec![
            "╭────────────────────────╮".into(),
            "│ SubAgent Status         │".into(),
            "╰────────────────────────╯".into(),
            String::new(),
            format!("Name: {}", instance.name),
            format!("Role: {}", instance.role.as_str()),
            format!("Type: {}", instance.instance_type.as_str()),
            format!("Status: {}", instance.status.as_str()),
            format!("ID: {}", instance.id),
            format!(
                "Parent: {}",
                instance
                    .parent_agent_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".into())
            ),
            format!(
                "Supervisor: {}",
                instance
                    .supervisor_agent_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "none".into())
            ),
            format!("Version: {}", instance.version),
            format!("Actor: {}", instance.actor),
            format!("Created: {}", instance.created_at),
            format!("Updated: {}", instance.updated_at),
        ];

        Ok(CommandOutput::new(lines.join("\n")))
    }
}