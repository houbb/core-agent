//! Society Command Plugin — Agent Society 命令注册入口
//!
//! 统一管理 /agents, /delegate, /team, /roles, /collaborate 命令的
//! 注册和生命周期。

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::MultiAgentManager;

use super::commands::agents::AgentsCommand;
use super::commands::collaborate::CollaborateCommand;
use super::commands::delegate::DelegateCommand;
use super::commands::roles::RolesCommand;
use super::commands::team::TeamCommand;
use super::{CommandContext, CommandOutput, CommandMetadata, SlashCategory, SlashCommand, SlashError, SlashResult, SlashCommandRegistry};

/// Society 命令插件
///
/// 将 5 个 Agent Society 命令注册到 SlashCommandRegistry。
/// 每个命令持有 `Arc<MultiAgentManager>` 用于访问多 Agent 运行时。
pub struct SocietyCommandPlugin {
    pub agents: AgentsCommand,
    pub delegate: DelegateCommand,
    pub team: TeamCommand,
    pub roles: RolesCommand,
    pub collaborate: CollaborateCommand,
    multi_agent: Arc<MultiAgentManager>,
}

impl SocietyCommandPlugin {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self {
            agents: AgentsCommand::new(multi_agent.clone()),
            delegate: DelegateCommand::new(multi_agent.clone()),
            team: TeamCommand::new(multi_agent.clone()),
            roles: RolesCommand::new(multi_agent.clone()),
            collaborate: CollaborateCommand::new(multi_agent.clone()),
            multi_agent,
        }
    }

    /// 注册所有 Society 命令到注册表
    pub fn register_all(&self, registry: &mut SlashCommandRegistry) -> SlashResult<()> {
        registry.register(Arc::new(CommandsWrapper::Agents(self.agents.clone())))?;
        registry.register(Arc::new(CommandsWrapper::Delegate(self.delegate.clone())))?;
        registry.register(Arc::new(CommandsWrapper::Team(self.team.clone())))?;
        registry.register(Arc::new(CommandsWrapper::Roles(self.roles.clone())))?;
        registry.register(Arc::new(CommandsWrapper::Collaborate(self.collaborate.clone())))?;
        Ok(())
    }

    pub fn multi_agent(&self) -> Arc<MultiAgentManager> {
        self.multi_agent.clone()
    }
}

/// 命令包装器（用于统一 SlashCommand trait 注册）
enum CommandsWrapper {
    Agents(AgentsCommand),
    Delegate(DelegateCommand),
    Team(TeamCommand),
    Roles(RolesCommand),
    Collaborate(CollaborateCommand),
}

#[async_trait]
impl SlashCommand for CommandsWrapper {
    fn metadata(&self) -> CommandMetadata {
        match self {
            Self::Agents(cmd) => cmd.metadata(),
            Self::Delegate(cmd) => cmd.metadata(),
            Self::Team(cmd) => cmd.metadata(),
            Self::Roles(cmd) => cmd.metadata(),
            Self::Collaborate(cmd) => cmd.metadata(),
        }
    }

    fn category(&self) -> SlashCategory {
        match self {
            Self::Agents(cmd) => cmd.category(),
            Self::Delegate(cmd) => cmd.category(),
            Self::Team(cmd) => cmd.category(),
            Self::Roles(cmd) => cmd.category(),
            Self::Collaborate(cmd) => cmd.category(),
        }
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        match self {
            Self::Agents(cmd) => cmd.validate(args).await,
            Self::Delegate(cmd) => cmd.validate(args).await,
            Self::Team(cmd) => cmd.validate(args).await,
            Self::Roles(cmd) => cmd.validate(args).await,
            Self::Collaborate(cmd) => cmd.validate(args).await,
        }
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        match self {
            Self::Agents(cmd) => cmd.execute(ctx).await,
            Self::Delegate(cmd) => cmd.execute(ctx).await,
            Self::Team(cmd) => cmd.execute(ctx).await,
            Self::Roles(cmd) => cmd.execute(ctx).await,
            Self::Collaborate(cmd) => cmd.execute(ctx).await,
        }
    }
}