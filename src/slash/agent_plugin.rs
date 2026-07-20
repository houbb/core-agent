//! Agent Command Plugin — Agent 命令注册入口
//!
//! 统一管理 /test, /debug-agent, /security-review 命令的注册和生命周期。
//! 不同于 SocietyCommandPlugin（依赖 MultiAgentManager），这些命令直接依赖
//! SubAgentRuntime。

use std::sync::Arc;

use async_trait::async_trait;

use crate::subagent_runtime::SubAgentRuntime;

use super::commands::debug_agent::DebugAgentCommand;
use super::commands::security_review::SecurityReviewCommand;
use super::commands::test::TestCommand;
use super::{CommandContext, CommandOutput, CommandMetadata, SlashCategory, SlashCommand, SlashError, SlashResult, SlashCommandRegistry};

/// Agent 命令插件
pub struct AgentCommandPlugin {
    pub test: TestCommand,
    pub debug_agent: DebugAgentCommand,
    pub security_review: SecurityReviewCommand,
    runtime: Arc<SubAgentRuntime>,
}

impl AgentCommandPlugin {
    pub fn new(runtime: Arc<SubAgentRuntime>) -> Self {
        Self {
            test: TestCommand::new(runtime.clone()),
            debug_agent: DebugAgentCommand::new(runtime.clone()),
            security_review: SecurityReviewCommand::new(runtime.clone()),
            runtime,
        }
    }

    /// 注册所有 Agent 命令到注册表
    pub fn register_all(&self, registry: &mut SlashCommandRegistry) -> SlashResult<()> {
        registry.register(Arc::new(CommandsWrapper::Test(self.test.clone())))?;
        registry.register(Arc::new(CommandsWrapper::DebugAgent(self.debug_agent.clone())))?;
        registry.register(Arc::new(CommandsWrapper::SecurityReview(self.security_review.clone())))?;
        Ok(())
    }

    pub fn runtime(&self) -> Arc<SubAgentRuntime> {
        self.runtime.clone()
    }
}

/// 命令包装器（用于统一 SlashCommand trait 注册）
enum CommandsWrapper {
    Test(TestCommand),
    DebugAgent(DebugAgentCommand),
    SecurityReview(SecurityReviewCommand),
}

#[async_trait]
impl SlashCommand for CommandsWrapper {
    fn metadata(&self) -> CommandMetadata {
        match self {
            Self::Test(cmd) => cmd.metadata(),
            Self::DebugAgent(cmd) => cmd.metadata(),
            Self::SecurityReview(cmd) => cmd.metadata(),
        }
    }

    fn category(&self) -> SlashCategory {
        match self {
            Self::Test(cmd) => cmd.category(),
            Self::DebugAgent(cmd) => cmd.category(),
            Self::SecurityReview(cmd) => cmd.category(),
        }
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        match self {
            Self::Test(cmd) => cmd.validate(args).await,
            Self::DebugAgent(cmd) => cmd.validate(args).await,
            Self::SecurityReview(cmd) => cmd.validate(args).await,
        }
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        match self {
            Self::Test(cmd) => cmd.execute(ctx).await,
            Self::DebugAgent(cmd) => cmd.execute(ctx).await,
            Self::SecurityReview(cmd) => cmd.execute(ctx).await,
        }
    }
}