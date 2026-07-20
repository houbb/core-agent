//! `/compact` — Context Compression
//!
//! 手动触发上下文压缩，降低 token 用量。
//! 基于 SummaryReducer 的 last-N + extractive summary 策略。
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Compact 命令
pub struct CompactCommand;

impl CompactCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for CompactCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "compact".into(),
            display_name: "Context Compact".into(),
            description: "Compress current conversation context to reduce token usage".into(),
            usage: "/compact".into(),
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
        // 实际压缩逻辑由 EnterpriseAgent 的 execute_command 处理
        // 这里返回占位响应
        Ok(CommandOutput::new(
            "Context compression triggered.\n\nUse /compact in an active session to see before/after token counts."
        ))
    }
}