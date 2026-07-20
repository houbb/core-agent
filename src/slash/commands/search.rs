//! `/search` — Code Search
//!
//! 搜索代码符号和文件，基于 code_index.query 工具。
//! 支持按语言、符号类型、路径过滤。
//!
//! 用法：
//!   /search <query>                 — 搜索符号
//!   /search <query> --type java     — 按语言过滤
//!   /search <query> --kind class    — 按符号类型过滤
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Search 命令
pub struct SearchCommand;

impl SearchCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for SearchCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "search".into(),
            display_name: "Code Search".into(),
            description: "Search code symbols and files across the project".into(),
            usage: "/search <query> [--type <language>] [--kind <symbol-kind>]".into(),
            category: SlashCategory::Project,
            min_args: 1,
            max_args: 6,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Project
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() || args[0].starts_with("--") {
            return Err(SlashError::InvalidArgument(
                "usage: /search <query> [--type <language>] [--kind <symbol-kind>]".into(),
            ));
        }
        // Validate optional flags
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--type" | "--kind" | "--path" => {
                    i += 2; // skip flag and value
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --type, --kind, --path",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        // Actual implementation is in EnterpriseAgent::execute_command()
        Ok(CommandOutput::new(
            "Code search command accepted.\n\nUse /search <query> in an active session to search code symbols."
        ))
    }
}