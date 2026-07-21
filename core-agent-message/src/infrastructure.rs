use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{AgentMessage, MessageType};
use crate::error::MessageResult;

// ── MessageStore ──

#[async_trait]
pub trait MessageStore: Send + Sync {
    async fn save(
        &self,
        message: &AgentMessage,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MessageResult<()>;

    async fn find(&self, id: Uuid) -> MessageResult<Option<AgentMessage>>;

    async fn list_by_to_agent(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>>;

    async fn list_by_from_agent(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>>;

    async fn list_by_correlation(
        &self,
        correlation_id: Uuid,
    ) -> MessageResult<Vec<AgentMessage>>;

    async fn mark_read(&self, message_id: Uuid, actor: &str) -> MessageResult<bool>;

    async fn list_inbox(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>>;
}

// ── MessageBus ──

#[async_trait]
pub trait MessageBus: Send + Sync {
    async fn send(&self, message: &AgentMessage, actor: &str) -> MessageResult<AgentMessage>;

    async fn receive(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>>;

    async fn broadcast(
        &self,
        from: Uuid,
        to_agents: &[Uuid],
        message_type: MessageType,
        intent: &str,
        payload: Value,
        actor: &str,
    ) -> MessageResult<Vec<AgentMessage>>;
}

// ── Observer ──

pub trait MessageObserver: Send + Sync {
    fn on_message_sent(&self, message: &AgentMessage);
    fn on_message_read(&self, message_id: Uuid, agent_id: Uuid);
}

// ── Interceptor ──

pub trait MessageInterceptor: Send + Sync {
    fn before_send(&self, _message: &mut AgentMessage) -> MessageResult<()> {
        Ok(())
    }
}