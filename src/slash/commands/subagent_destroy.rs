use std::sync::Arc;

use async_trait::async_trait;
use core_agent_subagent::SubAgentManager;
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct SubAgentDestroyCommand {
    manager: Arc<SubAgentManager>,
}

impl SubAgentDestroyCommand {
    pub fn new(manager: Arc<SubAgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for SubAgentDestroyCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "subagent".into(),
            display_name: "SubAgent Destroy".into(),
            description: "销毁子 Agent 实例".into(),
            usage: "/subagent destroy <id>".into(),
            category: SlashCategory::Orchestration,
            min_args: 2,
            max_args: 2,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() != 2 || args[0] != "destroy" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /subagent destroy <id>".into(),
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
            .destroy(id, "system")
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        Ok(CommandOutput::new(format!(
            "SubAgent {} (ID: {}) destroyed.",
            instance.name, instance.id
        )))
    }
}