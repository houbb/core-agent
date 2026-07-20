//! `/debug-agent` — Debug Agent
//!
//! 使用 Debug SubAgent 分析错误、诊断问题、定位根因。
//!
//! 用法：
//!   /debug-agent <task>             — 运行 Debug Agent 处理指定任务
//!
//! 路由：Agent（触发 SubAgent Runtime）
//!
//! 注意：此命令与 `/debug`（Trace 分析）不同，/debug-agent 使用 LLM 驱动的
//! Debug SubAgent 进行错误诊断和根因分析。

use std::sync::Arc;

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::subagent_runtime::{SubAgentProfile, SubAgentRuntime};
use tokio_util::sync::CancellationToken;

/// Debug Agent 命令
#[derive(Clone)]
pub struct DebugAgentCommand {
    runtime: Arc<SubAgentRuntime>,
}

impl DebugAgentCommand {
    pub fn new(runtime: Arc<SubAgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl SlashCommand for DebugAgentCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "debug-agent".into(),
            display_name: "Debug Agent".into(),
            description: "Diagnose errors, analyze stack traces, locate root causes using the Debug Agent".into(),
            usage: "/debug-agent <task>".into(),
            category: SlashCategory::Agent,
            min_args: 1,
            max_args: 8,
            read_only: false,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Agent
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let task = ctx.args.join(" ");

        let outcome = self.runtime.run(
            &task,
            SubAgentProfile::Debug,
            ctx.session_id.and_then(|s| s.parse().ok()),
            CancellationToken::new(),
        ).await.map_err(|e| SlashError::Execution(e.to_string()))?;

        Ok(CommandOutput::new(format!(
            "╭────────────────────────╮\n\
             │ Debug Agent Result     │\n\
             ╰────────────────────────╯\n\n\
             {}\n\n\
             ---\n\
             Profile: {:?} | Tool calls: {} | Turns: {}",
            outcome.response,
            outcome.profile,
            outcome.tool_calls,
            outcome.turns,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent_runtime::SubAgentProfile;

    #[test]
    fn debug_agent_profile_is_debug() {
        assert_eq!(SubAgentProfile::Debug.max_turns(), 8);
    }
}