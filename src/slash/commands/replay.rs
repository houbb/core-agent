//! `/replay` — 历史执行回放
//!
//! 基于事件溯源，回放 Agent 历史执行过程。
//!
//! 用法：
//!   /replay <trace-id>             — 回放指定 Trace
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::{SqliteTraceStore, ReplayEngine};

pub struct ReplayCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl ReplayCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for ReplayCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "replay".into(),
            display_name: "Execution Replay".into(),
            description: "Replay historical Agent execution step by step".into(),
            usage: "/replay <trace-id>".into(),
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
            SlashError::InvalidArgument("usage: /replay <trace-id>".into())
        })?;

        let uuid = uuid::Uuid::parse_str(id).map_err(|_| {
            SlashError::InvalidArgument("invalid trace UUID format".into())
        })?;

        let trace = self
            .store
            .get_trace(uuid)
            .map_err(|e| SlashError::Execution(e))?
            .ok_or_else(|| SlashError::InvalidArgument(format!("Trace not found: {id}")))?;

        let report = ReplayEngine::build_replay(&trace);

        let mut output = String::new();
        output.push_str("Execution Replay\n\n");

        output.push_str(&format!(
            "Original: {} ({})\n\n",
            trace.agent_id,
            trace.created_at.format("%Y-%m-%d %H:%M:%S")
        ));

        output.push_str("Event History:\n\n");
        for event in &report.events {
            let icon = match event.event_type.as_str() {
                "planning" => "🔍",
                "reasoning" => "💭",
                "delegation" => "🔄",
                "tool_call" => "🔧",
                "observation" => "👁",
                "decision" => "🎯",
                "reflection" => "📝",
                "response" => "💬",
                _ => "➡",
            };
            output.push_str(&format!(
                "  #[{}] {} {} — {}\n",
                event.sequence, icon, event.agent, event.event_type
            ));
            if let Some(ref tool) = event.tool {
                output.push_str(&format!(
                    "        Tool: {}\n",
                    tool
                ));
            }
            output.push_str(&format!(
                "        Input: {}\n",
                truncate(&event.input, 100)
            ));
            output.push_str(&format!(
                "        Output: {}\n",
                truncate(&event.output, 100)
            ));
            if let Some(ref error) = event.error {
                output.push_str(&format!("        ⚠ Error: {}\n", error));
            }
            output.push('\n');
        }

        if !report.differences.is_empty() {
            output.push_str("Differences detected:\n\n");
            for step in &report.differences {
                output.push_str(&format!("  Step {} changed (had error)\n", step));
            }
            output.push('\n');
        }

        output.push_str(&format!(
            "Result: {}\n",
            if report.success { "✅ Success" } else { "❌ Failed" }
        ));
        output.push_str(&format!("Total events: {}", report.total_events));

        Ok(CommandOutput::new(output))
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}...", &text[..max])
    }
}