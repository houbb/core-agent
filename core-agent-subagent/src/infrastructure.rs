use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{AgentInstance, AgentRole, InstanceType, SubAgentStatus};
use crate::error::SubAgentResult;

// ── Operation & Stage ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentOperation {
    Create,
    Start,
    Stop,
    Destroy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubAgentStage {
    Lifecycle,
    Persistence,
}

// ── Observation ──

#[derive(Debug, Clone)]
pub struct SubAgentObservation {
    pub operation: SubAgentOperation,
    pub stage: SubAgentStage,
    pub success: bool,
    pub agent_id: Uuid,
    pub status: SubAgentStatus,
    pub actor: String,
    pub message: Option<String>,
}

// ── Observer ──

pub trait SubAgentObserver: Send + Sync {
    fn on_observation(&self, observation: &SubAgentObservation);
}

// ── Interceptor ──

pub trait SubAgentInterceptor: Send + Sync {
    fn before_create(&self, _instance: &mut AgentInstance) -> SubAgentResult<()> {
        Ok(())
    }
    fn before_destroy(&self, _instance: &AgentInstance) -> SubAgentResult<()> {
        Ok(())
    }
}

// ── Store ──

#[async_trait]
pub trait SubAgentStore: Send + Sync {
    async fn save(
        &self,
        instance: &AgentInstance,
        expected_version: Option<u64>,
        actor: &str,
    ) -> SubAgentResult<()>;

    async fn find(&self, id: Uuid) -> SubAgentResult<Option<AgentInstance>>;

    async fn list_by_parent(&self, parent_id: Uuid) -> SubAgentResult<Vec<AgentInstance>>;

    async fn list_by_supervisor(&self, supervisor_id: Uuid) -> SubAgentResult<Vec<AgentInstance>>;

    async fn list_by_status(&self, status: SubAgentStatus) -> SubAgentResult<Vec<AgentInstance>>;

    async fn list_all(&self) -> SubAgentResult<Vec<AgentInstance>>;
}

// ── Lifecycle ──

pub trait SubAgentLifecycle: Send + Sync {
    fn transition(
        &self,
        instance: &mut AgentInstance,
        next: SubAgentStatus,
        actor: &str,
        reason: &str,
    ) -> SubAgentResult<()>;
}

// ── Factory ──

pub trait SubAgentFactory: Send + Sync {
    fn create(
        &self,
        name: String,
        instance_type: InstanceType,
        role: AgentRole,
        parent_agent_id: Option<Uuid>,
        supervisor_agent_id: Option<Uuid>,
        config: Value,
        actor: String,
    ) -> SubAgentResult<AgentInstance>;
}