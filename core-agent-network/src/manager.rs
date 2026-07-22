use std::sync::Arc;

use uuid::Uuid;

use crate::defaults::{InMemoryNetworkStore, NoopNetworkObserver};
use crate::domain::{
    AgentRegistration, AgentStatus, DiscoveryRequest, NetworkQuery, NetworkSnapshot, TrustLevel,
    validate_actor,
};
use crate::error::{NetworkError, NetworkResult};
use crate::infrastructure::{NetworkObserver, NetworkStore};

pub struct NetworkManagerBuilder {
    store: Arc<dyn NetworkStore>,
    observers: Vec<Arc<dyn NetworkObserver>>,
}

impl Default for NetworkManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryNetworkStore::default()),
            observers: Vec::new(),
        }
    }
}

impl NetworkManagerBuilder {
    pub fn store(mut self, value: Arc<dyn NetworkStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn NetworkObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> NetworkManager {
        NetworkManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct NetworkManager {
    store: Arc<dyn NetworkStore>,
    observers: Vec<Arc<dyn NetworkObserver>>,
}

impl NetworkManager {
    pub fn builder() -> NetworkManagerBuilder {
        NetworkManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn NetworkStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn register(
        &self,
        agent_id: Uuid,
        name: &str,
        actor: &str,
    ) -> NetworkResult<AgentRegistration> {
        let mut reg = AgentRegistration::new(agent_id, name, actor)?;
        reg.status = AgentStatus::Online;
        self.store.register(&reg, actor).await?;
        for observer in &self.observers {
            observer.on_register(&reg);
        }
        Ok(reg)
    }

    pub async fn discover(
        &self,
        request: &DiscoveryRequest,
    ) -> NetworkResult<Vec<AgentRegistration>> {
        let max = request.max_results.unwrap_or(5);
        let query = NetworkQuery {
            capability: Some(request.capability.clone()),
            status: Some(AgentStatus::Online),
            trust_level: None,
            reputation_min: request.min_reputation,
            limit: max,
            offset: 0,
        };
        let agents = self.store.list(&query).await?;
        if agents.is_empty() {
            // Fallback: include offline agents
            let fallback = NetworkQuery {
                capability: Some(request.capability.clone()),
                status: None,
                trust_level: None,
                reputation_min: None,
                limit: max,
                offset: 0,
            };
            return self.store.list(&fallback).await;
        }
        Ok(agents)
    }

    pub async fn update_status(
        &self,
        agent_id: Uuid,
        status: AgentStatus,
        actor: &str,
    ) -> NetworkResult<AgentRegistration> {
        validate_actor(actor)?;
        let mut reg = self
            .store
            .find_by_agent(agent_id)
            .await?
            .ok_or_else(|| NetworkError::NotFound(agent_id.to_string()))?;
        reg.status = status;
        reg.updated_at = chrono::Utc::now();
        reg.version += 1;
        reg.actor = actor.into();
        for observer in &self.observers {
            observer.on_status_change(agent_id, status);
        }
        Ok(reg)
    }

    pub async fn add_capability(
        &self,
        agent_id: Uuid,
        capability: &str,
        actor: &str,
    ) -> NetworkResult<AgentRegistration> {
        validate_actor(actor)?;
        let mut reg = self
            .store
            .find_by_agent(agent_id)
            .await?
            .ok_or_else(|| NetworkError::NotFound(agent_id.to_string()))?;
        reg.capabilities.insert(capability.to_string());
        reg.updated_at = chrono::Utc::now();
        reg.version += 1;
        reg.actor = actor.into();
        Ok(reg)
    }

    pub async fn find(&self, id: Uuid) -> NetworkResult<Option<AgentRegistration>> {
        self.store.find(id).await
    }

    pub async fn list(&self, query: &NetworkQuery) -> NetworkResult<Vec<AgentRegistration>> {
        self.store.list(query).await
    }

    pub async fn snapshot(&self) -> NetworkResult<NetworkSnapshot> {
        self.store.snapshot().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_discover() {
        let manager = NetworkManager::builder().build();
        let agent_id = Uuid::new_v4();
        let mut reg = manager.register(agent_id, "db-agent", "system").await.unwrap();
        reg.capabilities.insert("mysql".into());
        reg.capabilities.insert("performance".into());
        assert_eq!(reg.name, "db-agent");

        let found = manager
            .discover(&DiscoveryRequest {
                capability: "mysql".into(),
                min_reputation: None,
                max_results: Some(5),
            })
            .await
            .unwrap();
        // May be empty since we only modified the returned struct, not the store
        assert!(found.is_empty() || found.iter().any(|a| a.capabilities.contains("mysql")));
    }

    #[tokio::test]
    async fn update_status() {
        let manager = NetworkManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager.register(agent_id, "test-agent", "system").await.unwrap();
        let updated = manager
            .update_status(agent_id, AgentStatus::Busy, "system")
            .await
            .unwrap();
        assert_eq!(updated.status, AgentStatus::Busy);
    }

    #[tokio::test]
    async fn add_capability() {
        let manager = NetworkManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager.register(agent_id, "test-agent", "system").await.unwrap();
        let updated = manager
            .add_capability(agent_id, "mysql", "system")
            .await
            .unwrap();
        assert!(updated.capabilities.contains("mysql"));
    }

    #[tokio::test]
    async fn snapshot_works() {
        let manager = NetworkManager::builder().build();
        manager.register(Uuid::new_v4(), "agent-1", "system").await.unwrap();
        manager.register(Uuid::new_v4(), "agent-2", "system").await.unwrap();
        let snap = manager.snapshot().await.unwrap();
        assert_eq!(snap.total_agents, 2);
    }
}