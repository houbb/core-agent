//! `/architecture` — Architecture Understanding
//!
//! 查看项目架构图，基于 architecture.graph + project.analyzer 工具。
//!
//! 用法：
//!   /architecture                   — 查看架构图（文本格式）
//!   /architecture --format json     — JSON 格式输出
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Architecture 命令
pub struct ArchitectureCommand;

impl ArchitectureCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for ArchitectureCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "architecture".into(),
            display_name: "Project Architecture".into(),
            description: "View project architecture diagram and module dependencies".into(),
            usage: "/architecture [--format <json|text>]".into(),
            category: SlashCategory::Project,
            min_args: 0,
            max_args: 2,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Project
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "--format" => {
                    let fmt = args.get(i + 1).ok_or_else(|| {
                        SlashError::InvalidArgument("--format requires json or text".into())
                    })?;
                    if fmt != "json" && fmt != "text" {
                        return Err(SlashError::InvalidArgument(
                            "--format must be 'json' or 'text'".into(),
                        ));
                    }
                    i += 2;
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --format",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Architecture command accepted.\n\nUse /architecture in an active session to view the project architecture."
        ))
    }
}