//! `/benchmark` — 能力基准测试
//!
//! 运行内置基准测试任务，评估 Agent 能力。
//!
//! 用法：
//!   /benchmark                      — 查看所有基准测试结果
//!   /benchmark <agent-id>           — 运行或查看指定 Agent 的基准测试
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::{SqliteTraceStore, BenchmarkEngine, BenchmarkResult, BenchmarkSummary};

pub struct BenchmarkCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl BenchmarkCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for BenchmarkCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "benchmark".into(),
            display_name: "Agent Benchmark".into(),
            description: "Run or view Agent capability benchmark results".into(),
            usage: "/benchmark [agent-id]".into(),
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

        // 查询已有结果
        let results = self
            .store
            .list_benchmark_results(agent_id)
            .map_err(|e| SlashError::Execution(e))?;

        let mut output = String::new();
        output.push_str("Benchmark\n\n");

        let tasks = BenchmarkEngine::builtin_tasks();
        output.push_str(&format!("Available tasks: {}\n\n", tasks.len()));

        if results.is_empty() {
            output.push_str(&format!(
                "No benchmark results for '{}'.\n\n",
                agent_id
            ));
            output.push_str("Available tasks:\n\n");
            for task in &tasks {
                output.push_str(&format!(
                    "  • {}  ({})  — {}\n",
                    task.name, task.category, task.description
                ));
            }
            output.push_str("\nRun a benchmark by executing tasks and they will be recorded here.");
            output.push_str("\n\nNote: MVP benchmark shows previously recorded results. Use /trace-agent to view execution traces.");
        } else {
            let summary = BenchmarkEngine::summarize(agent_id, &results);
            output.push_str(&format!("Agent: {}\n\n", summary.agent_id));
            output.push_str(&format!("Tasks: {}\n", summary.total_tasks));
            output.push_str(&format!("Success: {}\n", summary.success_count));
            output.push_str(&format!("Average Score: {:.1}\n", summary.average_score));
            output.push_str(&format!("Average Cost: {:.0} tokens\n", summary.average_cost));
            output.push_str(&format!("Average Duration: {:.0}ms\n\n", summary.average_duration_ms));

            output.push_str("Results:\n\n");
            for r in &results {
                let status = if r.success { "✅" } else { "❌" };
                output.push_str(&format!(
                    "  {}  {}  ({})  Score: {:.1}  {}ms\n",
                    status, r.task_name, r.task_category, r.score, r.duration_ms
                ));
                if let Some(ref error) = r.error {
                    output.push_str(&format!("       ⚠ {}\n", error));
                }
            }
        }

        Ok(CommandOutput::new(output))
    }
}