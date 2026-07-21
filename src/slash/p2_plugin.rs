//! P2 Command Plugin — Orchestration & SubAgent 命令注册入口
//!
//! 统一管理 /subagent, /orchestrate, /message 命令的注册和生命周期。

use std::sync::Arc;

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashError,
    SlashResult, SlashCommandRegistry,
};

use super::commands::message_inbox::MessageInboxCommand;
use super::commands::message_send::MessageSendCommand;
use super::commands::orchestrate::OrchestrateCommand;
use super::commands::orchestrate_status::OrchestrateStatusCommand;
use super::commands::subagent_destroy::SubAgentDestroyCommand;
use super::commands::subagent_list::SubAgentListCommand;
use super::commands::subagent_spawn::SubAgentSpawnCommand;
use super::commands::subagent_status::SubAgentStatusCommand;

/// P2 命令插件
pub struct P2CommandPlugin {
    pub subagent_list: SubAgentListCommand,
    pub subagent_spawn: SubAgentSpawnCommand,
    pub subagent_status: SubAgentStatusCommand,
    pub subagent_destroy: SubAgentDestroyCommand,
    pub orchestrate: OrchestrateCommand,
    pub orchestrate_status: OrchestrateStatusCommand,
    pub message_send: MessageSendCommand,
    pub message_inbox: MessageInboxCommand,
}

impl P2CommandPlugin {
    pub fn new(
        subagent_manager: Arc<core_agent_subagent::SubAgentManager>,
        message_manager: Arc<core_agent_message::MessageManager>,
        orchestrator_manager: Arc<core_agent_orchestrator::OrchestratorManager>,
    ) -> Self {
        Self {
            subagent_list: SubAgentListCommand::new(subagent_manager.clone()),
            subagent_spawn: SubAgentSpawnCommand::new(subagent_manager.clone()),
            subagent_status: SubAgentStatusCommand::new(subagent_manager.clone()),
            subagent_destroy: SubAgentDestroyCommand::new(subagent_manager.clone()),
            orchestrate: OrchestrateCommand::new(orchestrator_manager.clone()),
            orchestrate_status: OrchestrateStatusCommand::new(orchestrator_manager.clone()),
            message_send: MessageSendCommand::new(message_manager.clone()),
            message_inbox: MessageInboxCommand::new(message_manager.clone()),
        }
    }

    /// 注册所有 P2 命令到注册表
    pub fn register_all(&self, registry: &mut SlashCommandRegistry) -> SlashResult<()> {
        registry.register(Arc::new(P2CommandWrapper::SubAgentList(
            self.subagent_list.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::SubAgentSpawn(
            self.subagent_spawn.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::SubAgentStatus(
            self.subagent_status.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::SubAgentDestroy(
            self.subagent_destroy.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::Orchestrate(
            self.orchestrate.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::OrchestrateStatus(
            self.orchestrate_status.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::MessageSend(
            self.message_send.clone(),
        )))?;
        registry.register(Arc::new(P2CommandWrapper::MessageInbox(
            self.message_inbox.clone(),
        )))?;
        Ok(())
    }
}

// ── P2CommandWrapper ──

enum P2CommandWrapper {
    SubAgentList(SubAgentListCommand),
    SubAgentSpawn(SubAgentSpawnCommand),
    SubAgentStatus(SubAgentStatusCommand),
    SubAgentDestroy(SubAgentDestroyCommand),
    Orchestrate(OrchestrateCommand),
    OrchestrateStatus(OrchestrateStatusCommand),
    MessageSend(MessageSendCommand),
    MessageInbox(MessageInboxCommand),
}

#[async_trait]
impl SlashCommand for P2CommandWrapper {
    fn metadata(&self) -> CommandMetadata {
        match self {
            Self::SubAgentList(cmd) => cmd.metadata(),
            Self::SubAgentSpawn(cmd) => cmd.metadata(),
            Self::SubAgentStatus(cmd) => cmd.metadata(),
            Self::SubAgentDestroy(cmd) => cmd.metadata(),
            Self::Orchestrate(cmd) => cmd.metadata(),
            Self::OrchestrateStatus(cmd) => cmd.metadata(),
            Self::MessageSend(cmd) => cmd.metadata(),
            Self::MessageInbox(cmd) => cmd.metadata(),
        }
    }

    fn category(&self) -> SlashCategory {
        match self {
            Self::SubAgentList(cmd) => cmd.category(),
            Self::SubAgentSpawn(cmd) => cmd.category(),
            Self::SubAgentStatus(cmd) => cmd.category(),
            Self::SubAgentDestroy(cmd) => cmd.category(),
            Self::Orchestrate(cmd) => cmd.category(),
            Self::OrchestrateStatus(cmd) => cmd.category(),
            Self::MessageSend(cmd) => cmd.category(),
            Self::MessageInbox(cmd) => cmd.category(),
        }
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        match self {
            Self::SubAgentList(cmd) => cmd.validate(args).await,
            Self::SubAgentSpawn(cmd) => cmd.validate(args).await,
            Self::SubAgentStatus(cmd) => cmd.validate(args).await,
            Self::SubAgentDestroy(cmd) => cmd.validate(args).await,
            Self::Orchestrate(cmd) => cmd.validate(args).await,
            Self::OrchestrateStatus(cmd) => cmd.validate(args).await,
            Self::MessageSend(cmd) => cmd.validate(args).await,
            Self::MessageInbox(cmd) => cmd.validate(args).await,
        }
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        match self {
            Self::SubAgentList(cmd) => cmd.execute(ctx).await,
            Self::SubAgentSpawn(cmd) => cmd.execute(ctx).await,
            Self::SubAgentStatus(cmd) => cmd.execute(ctx).await,
            Self::SubAgentDestroy(cmd) => cmd.execute(ctx).await,
            Self::Orchestrate(cmd) => cmd.execute(ctx).await,
            Self::OrchestrateStatus(cmd) => cmd.execute(ctx).await,
            Self::MessageSend(cmd) => cmd.execute(ctx).await,
            Self::MessageInbox(cmd) => cmd.execute(ctx).await,
        }
    }
}