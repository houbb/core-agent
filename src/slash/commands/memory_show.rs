//! `/memory-show` — View Agent Memory
//!
//! 查看项目/会话记忆列表，按 scope 过滤。
//!
//! 用法：
//!   /memory-show                    — 查看所有记忆
//!   /memory-show project            — 查看项目记忆
//!   /memory-show session            — 查看当前会话记忆
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// MemoryShow 命令
pub struct MemoryShowCommand;

impl MemoryShowCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for MemoryShowCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "memory-show".into(),
            display_name: "Show Memory".into(),
            description: "View project or session memory entries".into(),
            usage: "/memory-show [scope]".into(),
            category: SlashCategory::Memory,
            min_args: 0,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Memory
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if let Some(scope) = args.first() {
            match scope.as_str() {
                "project" | "session" | "all" => {}
                _ => {
                    return Err(SlashError::InvalidArgument(
                        "scope must be 'project', 'session', or 'all'".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Memory show command accepted.\n\nUse /memory-show in an active session to view memory entries."
        ))
    }
}