use std::sync::Arc;

use async_trait::async_trait;
use core_agent_orchestrator::{OrchestrationStrategy, OrchestratorManager, OrchestrationStatus};

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct OrchestrateStatusCommand {
    manager: Arc<OrchestratorManager>,
}

impl OrchestrateStatusCommand {
    pub fn new(manager: Arc<OrchestratorManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for OrchestrateStatusCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "orchestrate".into(),
            display_name: "Orchestrate Status".into(),
            description: "查看编排任务状态".into(),
            usage: "/orchestrate status <orch_id>".into(),
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
                "usage: /orchestrate status <orch_id>".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let orch_id = uuid::Uuid::parse_str(&ctx.args[1])
            .map_err(|_| crate::slash::SlashError::InvalidArgument("invalid UUID".into()))?;

        // List all orchestrations and find the matching one
        let all = self
            .manager
            .list_all()
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        let orchestration = all
            .iter()
            .find(|o| o.id == orch_id)
            .ok_or_else(|| crate::slash::SlashError::NotFound(format!("orchestration {orch_id} not found")))?;

        let status_icon = match orchestration.status {
            OrchestrationStatus::Created => "🆕",
            OrchestrationStatus::Running => "🔄",
            OrchestrationStatus::Completed => "✅",
            OrchestrationStatus::Failed => "❌",
        };

        let mut lines = vec![
            "╭──────────────────────────────────────╮".into(),
            "│ Orchestration Status                 │".into(),
            "╰──────────────────────────────────────╯".into(),
            String::new(),
            format!("Goal: {}", orchestration.goal),
            format!("Strategy: {}", orchestration.strategy.as_str()),
            format!("{status_icon} Status: {}", orchestration.status.as_str()),
            format!("ID: {}", orchestration.id),
            format!("Supervisor: {}", orchestration.supervisor_agent_id),
            format!("Workers: {}", orchestration.worker_agents.len()),
        ];

        for worker in &orchestration.worker_agents {
            let role_icon = match worker.role {
                core_agent_subagent::AgentRole::Planner => "🧠",
                core_agent_subagent::AgentRole::Executor => "🛠",
                core_agent_subagent::AgentRole::Researcher => "📚",
                core_agent_subagent::AgentRole::Reviewer => "🔍",
                core_agent_subagent::AgentRole::Monitor => "👁",
                core_agent_subagent::AgentRole::DecisionMaker => "⚖",
            };
            lines.push(format!(
                "   {role_icon} {} ({})",
                worker.agent_name,
                worker.role.as_str()
            ));
        }

        if let Some(result) = &orchestration.result {
            lines.push(String::new());
            lines.push(format!("Confidence: {:.0}%", result.confidence * 100.0));
            lines.push(format!("Result: {}", result.summary));
        }

        Ok(CommandOutput::new(lines.join("\n")))
    }
}