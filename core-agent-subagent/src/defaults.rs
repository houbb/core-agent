use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{AgentInstance, AgentRole, InstanceType, SubAgentStatus};
use crate::error::{SubAgentError, SubAgentResult};
use crate::infrastructure::{
    SubAgentFactory, SubAgentLifecycle, SubAgentObserver,
    SubAgentStore,
};

// ── DefaultSubAgentLifecycle ──

#[derive(Default)]
pub struct DefaultSubAgentLifecycle;

impl SubAgentLifecycle for DefaultSubAgentLifecycle {
    fn transition(
        &self,
        instance: &mut AgentInstance,
        next: SubAgentStatus,
        actor: &str,
        _reason: &str,
    ) -> SubAgentResult<()> {
        let from = instance.status;
        let allowed = matches!(
            (from, next),
            (SubAgentStatus::Created, SubAgentStatus::Initialized)
                | (SubAgentStatus::Initialized, SubAgentStatus::Running)
                | (SubAgentStatus::Running, SubAgentStatus::Waiting)
                | (SubAgentStatus::Waiting, SubAgentStatus::Running)
                | (SubAgentStatus::Waiting, SubAgentStatus::Completed)
                | (SubAgentStatus::Running, SubAgentStatus::Failed)
                | (SubAgentStatus::Waiting, SubAgentStatus::Failed)
                | (SubAgentStatus::Initialized, SubAgentStatus::Failed)
                | (_, SubAgentStatus::Destroyed)
        );
        if !allowed {
            return Err(SubAgentError::InvalidState(format!(
                "cannot transition from {} to {}",
                from.as_str(),
                next.as_str()
            )));
        }
        instance.status = next;
        instance.version = instance.version.saturating_add(1);
        instance.actor = actor.into();
        instance.updated_at = chrono::Utc::now().max(instance.updated_at);
        Ok(())
    }
}

// ── DefaultSubAgentFactory ──

#[derive(Default)]
pub struct DefaultSubAgentFactory;

impl SubAgentFactory for DefaultSubAgentFactory {
    fn create(
        &self,
        name: String,
        instance_type: InstanceType,
        role: AgentRole,
        parent_agent_id: Option<Uuid>,
        supervisor_agent_id: Option<Uuid>,
        config: Value,
        actor: String,
    ) -> SubAgentResult<AgentInstance> {
        AgentInstance::new(name, instance_type, role, parent_agent_id, supervisor_agent_id, config, actor)
    }
}

// ── InMemorySubAgentStore ──

#[derive(Default)]
struct MemoryState {
    instances: BTreeMap<Uuid, AgentInstance>,
}

#[derive(Default)]
pub struct InMemorySubAgentStore {
    state: RwLock<MemoryState>,
}

impl InMemorySubAgentStore {
    fn read(&self) -> SubAgentResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| SubAgentError::Internal("subagent store lock poisoned".into()))
    }

    fn write(&self) -> SubAgentResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| SubAgentError::Internal("subagent store lock poisoned".into()))
    }
}

#[async_trait]
impl SubAgentStore for InMemorySubAgentStore {
    async fn save(
        &self,
        instance: &AgentInstance,
        expected_version: Option<u64>,
        _actor: &str,
    ) -> SubAgentResult<()> {
        let mut state = self.write()?;
        if let Some(current) = state.instances.get(&instance.id) {
            if let Some(expected) = expected_version {
                if current.version != expected {
                    return Err(SubAgentError::Conflict(
                        "subagent version conflict".into(),
                    ));
                }
            }
        }
        state.instances.insert(instance.id, instance.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> SubAgentResult<Option<AgentInstance>> {
        Ok(self.read()?.instances.get(&id).cloned())
    }

    async fn list_by_parent(&self, parent_id: Uuid) -> SubAgentResult<Vec<AgentInstance>> {
        let mut values = self
            .read()?
            .instances
            .values()
            .filter(|inst| inst.parent_agent_id == Some(parent_id))
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|inst| (inst.created_at, inst.id));
        Ok(values)
    }

    async fn list_by_supervisor(&self, supervisor_id: Uuid) -> SubAgentResult<Vec<AgentInstance>> {
        let mut values = self
            .read()?
            .instances
            .values()
            .filter(|inst| inst.supervisor_agent_id == Some(supervisor_id))
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|inst| (inst.created_at, inst.id));
        Ok(values)
    }

    async fn list_by_status(&self, status: SubAgentStatus) -> SubAgentResult<Vec<AgentInstance>> {
        let mut values = self
            .read()?
            .instances
            .values()
            .filter(|inst| inst.status == status)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|inst| (std::cmp::Reverse(inst.updated_at), inst.id));
        Ok(values)
    }

    async fn list_all(&self) -> SubAgentResult<Vec<AgentInstance>> {
        let mut values = self
            .read()?
            .instances
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|inst| (inst.created_at, inst.id));
        Ok(values)
    }
}

// ── NoopObserver ──

#[derive(Default)]
pub struct NoopSubAgentObserver;
impl SubAgentObserver for NoopSubAgentObserver {
    fn on_observation(&self, _observation: &crate::infrastructure::SubAgentObservation) {}
}