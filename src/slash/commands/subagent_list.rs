use std::sync::Arc;

use async_trait::async_trait;
use core_agent_subagent::SubAgentManager;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct SubAgentListCommand {
    manager: Arc<SubAgentManager>,
}

impl SubAgentListCommand {
    pub fn new(manager: Arc<SubAgentManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for SubAgentListCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "subagent".into(),
            display_name: "SubAgent List".into(),
            description: "列出所有子 Agent 实例".into(),
            usage: "/subagent list".into(),
            category: SlashCategory::Orchestration,
            min_args: 1,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() != 1 || args[0] != "list" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /subagent list".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        let instances = self
            .manager
            .list_all()
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        if instances.is_empty() {
            return Ok(CommandOutput::new(
                "No sub-agents found.\nUse /subagent spawn <role> <task> to create one.",
            ));
        }

        let mut lines = vec![
            "╭──────────────────────────────────────────────────╮".into(),
            "│ Sub-Agent Instances                               │".into(),
            "╰──────────────────────────────────────────────────╯".into(),
            String::new(),
        ];
        for inst in &instances {
            let status_icon = match inst.status {
                core_agent_subagent::SubAgentStatus::Running => "🔄",
                core_agent_subagent::SubAgentStatus::Waiting => "⏳",
                core_agent_subagent::SubAgentStatus::Completed => "✅",
                core_agent_subagent::SubAgentStatus::Failed => "❌",
                core_agent_subagent::SubAgentStatus::Destroyed => "💀",
                _ => "🆕",
            };
            let role_icon = match inst.role {
                core_agent_subagent::AgentRole::Planner => "🧠",
                core_agent_subagent::AgentRole::Executor => "🛠",
                core_agent_subagent::AgentRole::Researcher => "📚",
                core_agent_subagent::AgentRole::Reviewer => "🔍",
                core_agent_subagent::AgentRole::Monitor => "👁",
                core_agent_subagent::AgentRole::DecisionMaker => "⚖",
            };
            lines.push(format!(
                "{status_icon} {role_icon} {} — {} ({})",
                inst.name,
                inst.status.as_str(),
                inst.role.as_str()
            ));
            lines.push(format!("   ID: {}", inst.id));
            if let Some(parent) = inst.parent_agent_id {
                lines.push(format!("   Parent: {}", parent));
            }
            lines.push(String::new());
        }
        lines.push(format!("Summary: {} sub-agent(s)", instances.len()));

        Ok(CommandOutput::new(lines.join("\n")))
    }
}