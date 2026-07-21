use std::sync::Arc;

use async_trait::async_trait;
use core_agent_subagent::{InstanceType, SubAgentManager};
use core_agent_subagent::AgentRole;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct SubAgentSpawnCommand {
    manager: Arc<SubAgentManager>,
}

impl SubAgentSpawnCommand {
    pub fn new(manager: Arc<SubAgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for SubAgentSpawnCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "subagent".into(),
            display_name: "SubAgent Spawn".into(),
            description: "创建子 Agent 实例".into(),
            usage: "/subagent spawn <role> <task>".into(),
            category: SlashCategory::Orchestration,
            min_args: 3,
            max_args: 100,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() < 3 || args[0] != "spawn" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /subagent spawn <role> <task>".into(),
            ));
        }
        let role = &args[1];
        AgentRole::parse(&role.to_uppercase())
            .ok_or_else(|| crate::slash::SlashError::InvalidArgument(format!(
                "unknown role: {role}. Valid: planner, executor, researcher, reviewer, monitor, decisionmaker"
            )))?;
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let role_str = ctx.args[1].to_uppercase();
        let role = AgentRole::parse(&role_str).unwrap();
        let task = ctx.args[2..].join(" ");

        let instance = self
            .manager
            .create(
                format!("subagent-{}", &ctx.args[1]),
                InstanceType::Worker,
                role,
                None,
                None,
                serde_json::json!({"task": task}),
                "system",
            )
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        let lines = vec![
            "╭────────────────────────╮".into(),
            "│ SubAgent Spawned       │".into(),
            "╰────────────────────────╯".into(),
            String::new(),
            format!("Name: {}", instance.name),
            format!("Role: {}", instance.role.as_str()),
            format!("ID: {}", instance.id),
            format!("Status: {}", instance.status.as_str()),
            format!("Task: {task}"),
        ];

        Ok(CommandOutput::new(lines.join("\n")))
    }
}