//! `/trace` — Program Flow Analysis
//!
//! 分析函数调用链，基于 callgraph.query 工具。
//!
//! 用法：
//!   /trace <function>              — 追踪函数调用链
//!   /trace <function> --depth 5    — 指定追踪深度（默认 3）
//!
//! 路由：Runtime（零模型调用）

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Trace 命令
pub struct TraceCommand;

impl TraceCommand {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SlashCommand for TraceCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "trace".into(),
            display_name: "Call Trace".into(),
            description: "Analyze function call chains and dependencies".into(),
            usage: "/trace <function> [--depth <n>]".into(),
            category: SlashCategory::Project,
            min_args: 1,
            max_args: 4,
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
                "usage: /trace <function> [--depth <n>]".into(),
            ));
        }
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--depth" => {
                    let depth = args.get(i + 1).ok_or_else(|| {
                        SlashError::InvalidArgument("--depth requires a numeric value".into())
                    })?;
                    let n: usize = depth.parse().map_err(|_| {
                        SlashError::InvalidArgument("--depth must be a positive integer".into())
                    })?;
                    if n == 0 || n > 10 {
                        return Err(SlashError::InvalidArgument(
                            "--depth must be between 1 and 10".into(),
                        ));
                    }
                    i += 2;
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --depth",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        Ok(CommandOutput::new(
            "Call trace command accepted.\n\nUse /trace <function> in an active session to analyze call chains."
        ))
    }
}