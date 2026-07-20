//! `/memory-save` — Save Agent Memory
//!
//! 快速保存一条记忆到 Agent 记忆系统。
//!
//! 用法：
//!   /memory-save <content>          — 保存记忆（默认 scope=project, type=fact）
//!   /memory-save <content> --scope session --type rule --importance high
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// MemorySave 命令
pub struct MemorySaveCommand;

impl MemorySaveCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for MemorySaveCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "memory-save".into(),
            display_name: "Save Memory".into(),
            description: "Save a memory entry to the Agent memory system".into(),
            usage: "/memory-save <content> [--scope <project|session>] [--type <type>] [--importance <level>]".into(),
            category: SlashCategory::Memory,
            min_args: 1,
            max_args: 8,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Memory
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() || args[0].starts_with("--") {
            return Err(SlashError::InvalidArgument(
                "usage: /memory-save <content> [--scope <project|session>] [--type <type>] [--importance <level>]".into(),
            ));
        }
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--scope" => {
                    let scope = args.get(i + 1).ok_or_else(|| {
                        SlashError::InvalidArgument("--scope requires project or session".into())
                    })?;
                    if scope != "project" && scope != "session" {
                        return Err(SlashError::InvalidArgument(
                            "--scope must be 'project' or 'session'".into(),
                        ));
                    }
                    i += 2;
                }
                "--type" => {
                    let t = args.get(i + 1).ok_or_else(|| {
                        SlashError::InvalidArgument("--type requires a value".into())
                    })?;
                    if !["knowledge", "preference", "fact", "rule", "workspace"].contains(&t.as_str()) {
                        return Err(SlashError::InvalidArgument(
                            "--type must be: knowledge, preference, fact, rule, workspace".into(),
                        ));
                    }
                    i += 2;
                }
                "--importance" => {
                    let imp = args.get(i + 1).ok_or_else(|| {
                        SlashError::InvalidArgument("--importance requires a value".into())
                    })?;
                    if !["low", "medium", "high", "critical"].contains(&imp.as_str()) {
                        return Err(SlashError::InvalidArgument(
                            "--importance must be: low, medium, high, critical".into(),
                        ));
                    }
                    i += 2;
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --scope, --type, --importance",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Memory save command accepted.\n\nUse /memory-save <content> in an active session to save memory."
        ))
    }
}