use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{AgentRegistration, AgentStatus, NetworkQuery, NetworkSnapshot};
use crate::error::NetworkResult;

#[async_trait]
pub trait NetworkStore: Send + Sync {
    async fn register(&self, reg: &AgentRegistration, actor: &str) -> NetworkResult<()>;
    async fn find(&self, id: Uuid) -> NetworkResult<Option<AgentRegistration>>;
    async fn find_by_agent(&self, agent_id: Uuid) -> NetworkResult<Option<AgentRegistration>>;
    async fn list(&self, query: &NetworkQuery) -> NetworkResult<Vec<AgentRegistration>>;
    async fn snapshot(&self) -> NetworkResult<NetworkSnapshot>;
}

pub trait NetworkObserver: Send + Sync {
    fn on_register(&self, reg: &AgentRegistration);
    fn on_status_change(&self, agent_id: Uuid, status: AgentStatus);
}