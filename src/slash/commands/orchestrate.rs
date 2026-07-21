use std::sync::Arc;

use async_trait::async_trait;
use core_agent_orchestrator::{OrchestrationStrategy, OrchestratorManager, OrchestrationStatus};
use core_agent_subagent::AgentRole;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct OrchestrateCommand {
    manager: Arc<OrchestratorManager>,
}

impl OrchestrateCommand {
    pub fn new(manager: Arc<OrchestratorManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for OrchestrateCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "orchestrate".into(),
            display_name: "Orchestrate".into(),
            description: "启动多 Agent 编排任务（RCA Demo: /orchestrate supervisor \"订单服务 500\"）".into(),
            usage: "/orchestrate <strategy> <goal>".into(),
            category: SlashCategory::Orchestration,
            min_args: 2,
            max_args: 100,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() < 2 {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /orchestrate <strategy> <goal>".into(),
            ));
        }
        let strategy = &args[0];
        if OrchestrationStrategy::parse(strategy).is_none() {
            return Err(crate::slash::SlashError::InvalidArgument(format!(
                "unknown strategy: {strategy}. Valid: sequential, parallel, supervisor, debate"
            )));
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let strategy = OrchestrationStrategy::parse(&ctx.args[0]).unwrap();
        let goal = ctx.args[1..].join(" ");
        let supervisor_agent_id = uuid::Uuid::new_v4();

        // For supervisor strategy with RCA-like goal, auto-create worker agents
        let workers: Vec<(String, AgentRole)> = if strategy == OrchestrationStrategy::Supervisor
            && (goal.contains("500") || goal.contains("503") || goal.contains("error"))
        {
            // RCA Demo: auto-create Log, Metric, Trace agents
            vec![
                ("Log-Agent".into(), AgentRole::Researcher),
                ("Metric-Agent".into(), AgentRole::Researcher),
                ("Trace-Agent".into(), AgentRole::Researcher),
            ]
        } else {
            // Generic: create a single worker
            vec![("Worker-Agent".into(), AgentRole::Executor)]
        };

        let result = self
            .manager
            .supervise(goal.clone(), workers, supervisor_agent_id, "system")
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        let mut lines = vec![
            "╭──────────────────────────────────────╮".into(),
            "│ Orchestration Complete               │".into(),
            "╰──────────────────────────────────────╯".into(),
            String::new(),
            format!("Strategy: {}", strategy.as_str()),
            format!("Goal: {goal}"),
            format!("Workers: {} agent(s)", result.details.len()),
            String::new(),
            "Worker Results:".into(),
        ];

        for detail in &result.details {
            let status_icon = match detail.status {
                core_agent_subagent::SubAgentStatus::Completed => "✅",
                core_agent_subagent::SubAgentStatus::Failed => "❌",
                _ => "⏳",
            };
            lines.push(format!(
                "   {status_icon} {} — confidence: {:.0}%",
                detail.agent_name,
                detail.confidence * 100.0
            ));
            lines.push(format!("      Finding: {}", detail.finding));
        }

        lines.push(String::new());
        lines.push("╭──────────────────────────────────────╮".into());
        lines.push("│ Aggregated Result                    │".into());
        lines.push("╰──────────────────────────────────────╯".into());
        lines.push(String::new());
        lines.push(format!("Confidence: {:.0}%", result.confidence * 100.0));
        lines.push(format!("{}", result.summary));

        Ok(CommandOutput::new(lines.join("\n")))
    }
}