//! `/approve` — Human-in-the-loop Governance
//!
//! 查看和管理待审批操作。当 Agent 需要执行高风险操作时，
//! 可以查看待审批列表，或通过 ID 批准/拒绝。
//!
//! 用法：
//!   /approve list                   — 查看待审批操作列表
//!   /approve <id>                   — 批准指定 ID 的操作
//!   /deny <id>                      — 拒绝指定 ID 的操作
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Approve 命令
pub struct ApproveCommand;

impl ApproveCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for ApproveCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "approve".into(),
            display_name: "Approval Management".into(),
            description: "View and manage pending approval requests for high-risk operations".into(),
            usage: "/approve list | /approve <id> | /deny <id>".into(),
            category: SlashCategory::Governance,
            min_args: 1,
            max_args: 1,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Governance
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() {
            return Err(SlashError::InvalidArgument(
                "usage: /approve list | /approve <id> | /deny <id>".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Approval command accepted.\n\nUse /approve list in an active session to see pending approvals."
        ))
    }
}