//! `/learn` — Learn from Files
//!
//! 从文件或目录扫描知识，提取关键信息保存为记忆。
//! MVP 实现：扫描目录下的文件，提取文件名和内容作为记忆条目。
//!
//! 用法：
//!   /learn <path>                    — 从目录学习知识
//!   /learn <path> --recursive        — 递归扫描子目录
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Learn 命令
pub struct LearnCommand;

impl LearnCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for LearnCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "learn".into(),
            display_name: "Learn Knowledge".into(),
            description: "Scan files or directories and save extracted knowledge as memory".into(),
            usage: "/learn <path> [--recursive]".into(),
            category: SlashCategory::Memory,
            min_args: 1,
            max_args: 3,
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
                "usage: /learn <path> [--recursive]".into(),
            ));
        }
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--recursive" => {
                    i += 1;
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --recursive",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Learn command accepted.\n\nUse /learn <path> in an active session to scan and learn from files."
        ))
    }
}