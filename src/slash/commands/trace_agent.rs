//! `/trace-agent` — Agent 执行链追踪
//!
//! 查看 Agent 执行过程，包含时间线、步骤、工具调用等。
//!
//! 用法：
//!   /trace-agent                    — 查看最近一次 Trace
//!   /trace-agent <trace-id>         — 查看指定 Trace
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::observability::{SqliteTraceStore, TraceType};

pub struct TraceAgentCommand {
    store: std::sync::Arc<SqliteTraceStore>,
}

impl TraceAgentCommand {
    pub fn new(store: std::sync::Arc<SqliteTraceStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SlashCommand for TraceAgentCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "trace-agent".into(),
            display_name: "Agent Trace".into(),
            description: "View Agent execution chain with timeline and steps".into(),
            usage: "/trace-agent [trace-id]".into(),
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
        let trace_id = ctx.args.first().cloned();

        let traces = if let Some(id) = trace_id {
            let uuid = uuid::Uuid::parse_str(&id).map_err(|_| {
                SlashError::InvalidArgument("invalid trace UUID format".into())
            })?;
            match self.store.get_trace(uuid) {
                Ok(Some(trace)) => vec![trace],
                Ok(None) => return Ok(CommandOutput::new(format!("Trace not found: {id}"))),
                Err(e) => return Err(SlashError::Execution(e)),
            }
        } else {
            self.store
                .list_traces(5, 0)
                .map_err(|e| SlashError::Execution(e))?
        };

        if traces.is_empty() {
            return Ok(CommandOutput::new("No traces found. Run an Agent task first to generate a trace."));
        }

        let mut output = String::new();
        for trace in &traces {
            output.push_str(&format!(
                "╭────────────────────╮\n Agent Trace: {}\n╰────────────────────╯\n\n",
                &trace.trace_id.to_string()[..8]
            ));
            output.push_str(&format!("Task: {}\n\n", trace.goal));
            output.push_str("Timeline:\n\n");

            for step in &trace.steps {
                let time = step.created_at.format("%H:%M:%S").to_string();
                let icon = match step.step_type {
                    TraceType::Planning => "🔍",
                    TraceType::Reasoning => "💭",
                    TraceType::Delegation => "🔄",
                    TraceType::ToolCall => "🔧",
                    TraceType::Observation => "👁",
                    TraceType::Decision => "🎯",
                    TraceType::Reflection => "📝",
                    TraceType::Response => "💬",
                };
                output.push_str(&format!(
                    "  {}  {}  {}\n",
                    time,
                    step.agent_name,
                    step.output
                ));
                if let Some(ref tool) = step.tool_name {
                    output.push_str(&format!("       → tool: {}\n", tool));
                }
                if let Some(ref error) = step.error {
                    output.push_str(&format!("       ⚠ error: {}\n", error));
                }
                output.push('\n');
            }

            output.push_str(&format!(
                "Result: {}\n\n",
                if trace.success { "✅ Success" } else { "❌ Failed" }
            ));
            if let Some(score) = trace.score {
                output.push_str(&format!("Score: {:.1}/10\n\n", score));
            }
            output.push_str(&format!(
                "Duration: {}ms | Tokens: {}\n\n",
                trace.wall_duration_ms, trace.token_usage
            ));
            output.push_str(&format!("{}\n", "-".repeat(50)));
        }

        Ok(CommandOutput::new(output))
    }
}