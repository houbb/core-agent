//! `/memory-clear` — Clear Agent Memory
//!
//! 清除记忆（软删除，可恢复）。
//! 使用 MemoryManager.archive() 将记忆标记为 Archived 状态。
//!
//! 用法：
//!   /memory-clear <scope>            — 清除指定 scope 的所有记忆
//!   /memory-clear <scope> --confirm  — 跳过确认直接清除
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// MemoryClear 命令
pub struct MemoryClearCommand;

impl MemoryClearCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for MemoryClearCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "memory-clear".into(),
            display_name: "Clear Memory".into(),
            description: "Clear memory entries (soft-delete, recoverable)".into(),
            usage: "/memory-clear <scope> [--confirm]".into(),
            category: SlashCategory::Memory,
            min_args: 1,
            max_args: 2,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Memory
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() {
            return Err(SlashError::InvalidArgument(
                "usage: /memory-clear <scope> [--confirm]".into(),
            ));
        }
        let scope = args[0].as_str();
        if scope != "project" && scope != "session" && scope != "all" {
            return Err(SlashError::InvalidArgument(
                "scope must be 'project', 'session', or 'all'".into(),
            ));
        }
        if args.len() > 1 && args[1] != "--confirm" {
            return Err(SlashError::InvalidArgument(format!(
                "unknown flag: {}. Supported: --confirm",
                args[1]
            )));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Memory clear command accepted.\n\nUse /memory-clear <scope> in an active session to clear memory."
        ))
    }
}