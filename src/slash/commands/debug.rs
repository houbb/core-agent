//! `/debug` — Agent 调试
//!
//! 分析 Agent 失败根因，定位失败点，给出修复建议。
//!
//! 用法：
//!   /debug <trace-id>              — 调试指定 Trace
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::{SqliteTraceStore, DebugEngine};

pub struct DebugCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl DebugCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for DebugCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "debug".into(),
            display_name: "Agent Debug".into(),
            description: "Debug Agent failure root cause analysis".into(),
            usage: "/debug <trace-id>".into(),
            category: SlashCategory::Observability,
            min_args: 1,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Observability
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let id = ctx.args.first().ok_or_else(|| {
            SlashError::InvalidArgument("usage: /debug <trace-id>".into())
        })?;

        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
            SlashError::InvalidArgument("invalid trace UUID format".into())
        })?;

        let trace = self
            .store
            .get_trace(uuid)
            .map_err(|e| SlashError::Execution(e))?
            .ok_or_else(|| SlashError::InvalidArgument(format!("Trace not found: {id}")))?;

        let analysis = DebugEngine::analyze(&trace);

        let mut output = String::new();
        output.push_str("Debug Analysis\n\n");

        if analysis.failure_points.is_empty() && analysis.success {
            output.push_str("✅ No failures detected in this execution.\n\n");
        }

        if !analysis.failure_points.is_empty() {
            output.push_str("Failure Points:\n\n");
            for fp in &analysis.failure_points {
                output.push_str(&format!(
                    "  Step {}  Agent: {}  Type: {:?}\n",
                    fp.step_index, fp.agent_name, fp.step_type
                ));
                output.push_str(&format!("  Problem: {}\n\n", fp.problem));
            }
        }

        if !analysis.root_causes.is_empty() {
            output.push_str("Root Cause:\n\n");
            for cause in &analysis.root_causes {
                output.push_str(&format!("  • {}\n", cause));
            }
            output.push('\n');
        }

        if !analysis.recommendations.is_empty() {
            output.push_str("Recommendation:\n\n");
            for rec in &analysis.recommendations {
                output.push_str(&format!("  → {}\n", rec));
            }
            output.push('\n');
        }

        output.push_str(&format!(
            "\nTotal steps: {} | Overall: {}",
            analysis.total_steps,
            if analysis.success { "Success" } else { "Failed" }
        ));

        Ok(CommandOutput::new(output))
    }
}