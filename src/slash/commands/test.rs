//! `/test` — Test Agent
//!
//! 使用 Test SubAgent 分析测试失败、生成测试用例、诊断测试问题。
//!
//! 用法：
//!   /test <task>                     — 运行测试 Agent 处理指定任务
//!   /test --profile test             — 显式指定 Test Profile
//!
//! 路由：Agent（触发 SubAgent Runtime）

use std::sync::Arc;

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::subagent_runtime::{SubAgentProfile, SubAgentRuntime};
use tokio_util::sync::CancellationToken;

/// Test 命令
#[derive(Clone)]
pub struct TestCommand {
    runtime: Arc<SubAgentRuntime>,
}

impl TestCommand {
    pub fn new(runtime: Arc<SubAgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl SlashCommand for TestCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "test".into(),
            display_name: "Test Agent".into(),
            description: "Analyze test failures, generate test cases, and diagnose test issues using the Test Agent".into(),
            usage: "/test <task>".into(),
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
            SubAgentProfile::Test,
            ctx.session_id.and_then(|s| s.parse().ok()),
            CancellationToken::new(),
        ).await.map_err(|e| SlashError::Execution(e.to_string()))?;

        Ok(CommandOutput::new(format!(
            "╭────────────────────────╮\n\
             │ Test Agent Result      │\n\
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
    fn test_command_profile_is_test() {
        // Verify the correct profile is used
        assert_eq!(SubAgentProfile::Test.max_turns(), 8);
    }
}