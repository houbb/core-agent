use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    AgentRegistration, AgentStatus, NetworkQuery, NetworkSnapshot, validate_actor,
};
use crate::error::{NetworkError, NetworkResult};
use crate::infrastructure::NetworkStore;

#[derive(Default)]
pub struct InMemoryNetworkStore {
    agents: RwLock<Vec<AgentRegistration>>,
}

#[async_trait]
impl NetworkStore for InMemoryNetworkStore {
    async fn register(&self, reg: &AgentRegistration, actor: &str) -> NetworkResult<()> {
        validate_actor(actor)?;
        reg.validate()?;
        let mut agents = self
            .agents
            .write()
            .map_err(|_| NetworkError::Internal("lock poisoned".into()))?;
        if agents.iter().any(|a| a.id == reg.id) {
            return Err(NetworkError::Conflict("registration already exists".into()));
        }
        agents.push(reg.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> NetworkResult<Option<AgentRegistration>> {
        let agents = self
            .agents
            .read()
            .map_err(|_| NetworkError::Internal("lock poisoned".into()))?;
        Ok(agents.iter().find(|a| a.id == id).cloned())
    }

    async fn find_by_agent(&self, agent_id: Uuid) -> NetworkResult<Option<AgentRegistration>> {
        let agents = self
            .agents
            .read()
            .map_err(|_| NetworkError::Internal("lock poisoned".into()))?;
        Ok(agents.iter().find(|a| a.agent_id == agent_id).cloned())
    }

    async fn list(&self, query: &NetworkQuery) -> NetworkResult<Vec<AgentRegistration>> {
        let agents = self
            .agents
            .read()
            .map_err(|_| NetworkError::Internal("lock poisoned".into()))?;
        Ok(agents
            .iter()
            .filter(|a| {
                query
                    .capability
                    .as_ref()
                    .map_or(true, |c| a.capabilities.contains(c))
                    && query.status.map_or(true, |s| a.status == s)
                    && query.trust_level.map_or(true, |t| a.trust_level == t)
                    && query.reputation_min.map_or(true, |r| a.reputation >= r)
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect())
    }

    async fn snapshot(&self) -> NetworkResult<NetworkSnapshot> {
        let agents = self
            .agents
            .read()
            .map_err(|_| NetworkError::Internal("lock poisoned".into()))?;
        let mut by_capability = std::collections::BTreeMap::new();
        let mut online = 0u64;
        let mut rep_sum = 0.0;

        for a in agents.iter() {
            if a.status == AgentStatus::Online || a.status == AgentStatus::Busy {
                online += 1;
            }
            rep_sum += a.reputation;
            for c in &a.capabilities {
                *by_capability.entry(c.clone()).or_insert(0u64) += 1;
            }
        }

        let avg = if agents.is_empty() {
            0.0
        } else {
            rep_sum / agents.len() as f64
        };

        Ok(NetworkSnapshot {
            total_agents: agents.len() as u64,
            online_count: online,
            by_capability,
            avg_reputation: (avg * 100.0).round() / 100.0,
        })
    }
}

pub struct NoopNetworkObserver;

impl crate::infrastructure::NetworkObserver for NoopNetworkObserver {
    fn on_register(&self, _reg: &AgentRegistration) {}
    fn on_status_change(&self, _agent_id: Uuid, _status: AgentStatus) {}
}