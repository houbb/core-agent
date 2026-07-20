//! `/checkpoint` — Agent Transaction Snapshot
//!
//! 创建/恢复/列出 Agent 状态快照。
//! 比 undo/redo 更显式，支持命名 checkpoint。
//!
//! 用法：
//!   /checkpoint save <name>   — 创建命名 checkpoint
//!   /checkpoint list          — 列出所有 checkpoint
//!   /checkpoint restore <id>  — 恢复到指定 checkpoint
//!
//! 路由：Runtime

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Checkpoint 命令
pub struct CheckpointCommand;

impl CheckpointCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for CheckpointCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "checkpoint".into(),
            display_name: "Agent Checkpoint".into(),
            description: "Save, list, or restore Agent state checkpoints".into(),
            usage: "/checkpoint <save|list|restore> [name|id]".into(),
            category: SlashCategory::Checkpoint,
            min_args: 1,
            max_args: 2,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Checkpoint
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() {
            return Err(SlashError::InvalidArgument(
                "usage: /checkpoint <save|list|restore> [name|id]".into(),
            ));
        }
        let subcommand = args[0].as_str();
        match subcommand {
            "save" => {
                if args.len() < 2 {
                    return Err(SlashError::InvalidArgument(
                        "usage: /checkpoint save <name>".into(),
                    ));
                }
            }
            "list" => {
                if args.len() > 1 {
                    return Err(SlashError::InvalidArgument(
                        "usage: /checkpoint list".into(),
                    ));
                }
            }
            "restore" => {
                if args.len() < 2 {
                    return Err(SlashError::InvalidArgument(
                        "usage: /checkpoint restore <id>".into(),
                    ));
                }
            }
            _ => {
                return Err(SlashError::InvalidArgument(format!(
                    "unknown subcommand: {}. Use save, list, or restore",
                    subcommand
                )));
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        // 实际 checkpoint 逻辑由 EnterpriseAgent 的 execute_command 处理
        // 这里返回占位响应
        Ok(CommandOutput::new(
            "Checkpoint command accepted.\n\nUse /checkpoint in an active session to manage checkpoints."
        ))
    }
}