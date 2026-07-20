//! `/score` — Agent 健康度评分
//!
//! 快速查看 Agent 健康度，包含成功率、平均评分、耗时等。
//!
//! 用法：
//!   /score [agent-id]               — 查看 Agent 健康度
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::SqliteTraceStore;

pub struct ScoreCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl ScoreCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for ScoreCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "score".into(),
            display_name: "Agent Score".into(),
            description: "View Agent health dashboard with key metrics".into(),
            usage: "/score [agent-id]".into(),
            category: SlashCategory::Observability,
            min_args: 0,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Observability
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let agent_id = ctx.args.first().map(|s| s.as_str()).unwrap_or("default-agent");

        let health = self
            .store
            .agent_stats(agent_id)
            .map_err(|e| SlashError::Execution(e))?;

        let mut output = String::new();
        output.push_str("Agent Health Dashboard\n\n");

        // 标题
        output.push_str(&format!("Agent: {}\n\n", health.agent_id));

        // 指标
        output.push_str(&format!(
            "  Success Rate:  {:.0}%\n",
            health.success_rate
        ));
        output.push_str(&format!(
            "  Avg Score:     {:.1}/10\n",
            health.avg_score
        ));
        output.push_str(&format!(
            "  Avg Cost:      {:.0} tokens\n",
            health.avg_cost_tokens
        ));
        output.push_str(&format!(
            "  Avg Latency:   {:.0}ms\n",
            health.avg_latency_ms
        ));
        output.push_str(&format!(
            "  Total Traces:  {}\n",
            health.total_traces
        ));
        output.push_str(&format!(
            "  Recent (24h):  {}\n\n",
            health.recent_traces
        ));

        // 健康度条
        output.push_str("Health Bar:\n\n");
        let success_pct = (health.success_rate / 100.0).clamp(0.0, 1.0);
        let filled = (success_pct * 20.0).round() as usize;
        let empty = 20 - filled;
        output.push_str(&format!(
            "  Success:   [{}{}] {:.0}%\n",
            "█".repeat(filled),
            "░".repeat(empty),
            health.success_rate
        ));

        let score_pct = (health.avg_score / 10.0).clamp(0.0, 1.0);
        let filled = (score_pct * 20.0).round() as usize;
        let empty = 20 - filled;
        output.push_str(&format!(
            "  Score:     [{}{}] {:.1}/10\n",
            "█".repeat(filled),
            "░".repeat(empty),
            health.avg_score
        ));

        Ok(CommandOutput::new(output))
    }
}