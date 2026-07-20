//! `/knowledge` — Knowledge Base Status
//!
//! 查看知识库状态（当前 MVP 显示 Memory 系统状态）。
//!
//! 用法：
//!   /knowledge                      — 查看知识库状态
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Knowledge 命令
pub struct KnowledgeCommand;

impl KnowledgeCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for KnowledgeCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "knowledge".into(),
            display_name: "Knowledge Base".into(),
            description: "View knowledge base status and sources".into(),
            usage: "/knowledge".into(),
            category: SlashCategory::Memory,
            min_args: 0,
            max_args: 0,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Memory
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Knowledge command accepted.\n\nUse /knowledge in an active session to view knowledge base status."
        ))
    }
}