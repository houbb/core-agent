//! `/permissions` — Agent Security Runtime
//!
//! 查看当前 Agent 权限状态，包括文件系统、Shell、网络、Git 等权限。
//!
//! 用法：
//!   /permissions                    — 查看所有权限
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Permissions 命令
pub struct PermissionsCommand;

impl PermissionsCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for PermissionsCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "permissions".into(),
            display_name: "Agent Permissions".into(),
            description: "View current Agent permission state for filesystem, shell, network, and git".into(),
            usage: "/permissions".into(),
            category: SlashCategory::Governance,
            min_args: 0,
            max_args: 0,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Governance
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Permissions command accepted.\n\nUse /permissions in an active session to view Agent permissions."
        ))
    }
}