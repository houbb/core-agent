//! `/resume` — Session Recovery
//!
//! 恢复一个已暂停的会话，重新加载上下文。
//! 连接 SessionRuntime::resume_session() 和 ContextSnapshotStore。
//!
//! 路由：Runtime

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Resume 命令
pub struct ResumeCommand;

impl ResumeCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for ResumeCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "resume".into(),
            display_name: "Session Resume".into(),
            description: "Resume a paused session with its context restored".into(),
            usage: "/resume <session-id>".into(),
            category: SlashCategory::Session,
            min_args: 1,
            max_args: 1,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Session
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        // 实际恢复逻辑由 EnterpriseAgent 的 execute_command 处理
        // 这里返回占位响应
        Ok(CommandOutput::new(
            "Session resume requested.\n\nUse /resume <session-id> in an active session to restore a session."
        ))
    }
}