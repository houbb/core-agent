//! `/context` — Agent Context Status
//!
//! 显示当前 Agent 上下文状态：token 用量、文件、内存、工具等。
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Context 命令
pub struct ContextCommand;

impl ContextCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for ContextCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "context".into(),
            display_name: "Agent Context Status".into(),
            description: "Show current Agent context status (token usage, files, memory, tools)".into(),
            usage: "/context".into(),
            category: SlashCategory::Context,
            min_args: 0,
            max_args: 0,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Context
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        // 注意：实际 Context 数据来自 EnterpriseAgent 的 ContextRuntime
        // 这里的响应会被 EnterpriseAgent 的 execute_command 覆盖
        // 我们返回一个占位响应，EnterpriseAgent 会填充真实数据
        Ok(CommandOutput::new(
            "Agent Context Status\n\nUse /context in an active session to see detailed context information."
        ))
    }
}