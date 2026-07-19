use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    Agent, AgentOperation, AgentPolicyDecision, AgentPolicyDefinition, AgentProfile, AgentSnapshot,
    AgentState, AgentStateRecord, CreateAgentRequest,
};
use crate::error::{AgentError, AgentResult};
use crate::infrastructure::{AgentCommit, AgentFactory, AgentLifecycle, AgentPolicy, AgentStore};

pub struct DefaultAgentFactory;

impl AgentFactory for DefaultAgentFactory {
    fn create(&self, profile: AgentProfile, request: CreateAgentRequest) -> AgentResult<Agent> {
        Agent::new(profile, request)
    }
}

pub struct EmbeddedAgentPolicy;

impl AgentPolicy for EmbeddedAgentPolicy {
    fn evaluate(&self, operation: AgentOperation, agent: &Agent, _actor: &str) -> AgentResult<()> {
        match agent
            .policy
            .as_ref()
            .map(|value| value.decision(operation))
            .unwrap_or(AgentPolicyDecision::Allow)
        {
            AgentPolicyDecision::Allow => Ok(()),
            AgentPolicyDecision::Ask => Err(AgentError::PolicyDenied(format!(
                "{} requires human approval, which is unavailable in P7",
                operation.as_str()
            ))),
            AgentPolicyDecision::Deny => Err(AgentError::PolicyDenied(operation.as_str().into())),
        }
    }
}

pub struct DefaultAgentLifecycle;

impl DefaultAgentLifecycle {
    fn can_transition(current: AgentState, next: AgentState) -> bool {
        use AgentState::*;
        matches!(
            (current, next),
            (Created, Ready | Destroyed)
                | (Ready, Running | Paused | Completed | Destroyed)
                | (Waiting, Running | Paused | Completed | Destroyed)
                | (Running, Waiting | Paused | Failed)
                | (Paused, Ready | Running | Destroyed)
                | (Failed, Ready | Destroyed)
                | (Completed, Ready | Destroyed)
        )
    }

    fn record(
        agent: &mut Agent,
        from_state: Option<AgentState>,
        actor: &str,
        reason: &str,
    ) -> AgentResult<AgentStateRecord> {
        crate::domain::validate_actor(actor)?;
        crate::domain::validate_text("agent state reason", reason, 1024)?;
        agent.version = agent
            .version
            .checked_add(1)
            .ok_or_else(|| AgentError::Internal("agent version overflow".into()))?;
        agent.actor = actor.into();
        agent.updated_at = Utc::now();
        Ok(AgentStateRecord {
            id: Uuid::new_v4(),
            agent_id: agent.id,
            sequence: agent.version,
            from_state,
            to_state: agent.state,
            goal_id: agent.current_goal_id,
            plan_id: agent.current_plan_id,
            execution_id: agent.current_execution_id,
            reason: reason.into(),
            actor: actor.into(),
            created_at: agent.updated_at,
        })
    }
}

impl AgentLifecycle for DefaultAgentLifecycle {
    fn transition(
        &self,
        agent: &mut Agent,
        next: AgentState,
        actor: &str,
        reason: &str,
    ) -> AgentResult<AgentStateRecord> {
        let previous = agent.state;
        if !Self::can_transition(previous, next) {
            return Err(AgentError::InvalidState(format!(
                "{} -> {}",
                previous.as_str(),
                next.as_str()
            )));
        }
        agent.state = next;
        Self::record(agent, Some(previous), actor, reason)
    }

    fn record_progress(
        &self,
        agent: &mut Agent,
        actor: &str,
        reason: &str,
    ) -> AgentResult<AgentStateRecord> {
        Self::record(agent, Some(agent.state), actor, reason)
    }
}

#[derive(Default)]
struct MemoryState {
    agents: BTreeMap<Uuid, Agent>,
    profiles: BTreeMap<Uuid, AgentProfile>,
    policies: BTreeMap<Uuid, AgentPolicyDefinition>,
    snapshots: BTreeMap<Uuid, AgentSnapshot>,
    states: BTreeMap<Uuid, Vec<AgentStateRecord>>,
}

#[derive(Default)]
pub struct InMemoryAgentStore {
    state: RwLock<MemoryState>,
}

#[async_trait]
impl AgentStore for InMemoryAgentStore {
    async fn commit(&self, commit: &AgentCommit, actor: &str) -> AgentResult<()> {
        commit.validate(actor)?;
        let mut state = self
            .state
            .write()
            .map_err(|_| AgentError::Internal("agent store lock poisoned".into()))?;
        if let Some(current) = state.agents.get(&commit.agent.id) {
            if !same_agent_identity(current, &commit.agent) {
                return Err(AgentError::Validation(
                    "Agent identity, Profile/Policy snapshots, or bindings changed".into(),
                ));
            }
        }
        match (state.agents.get(&commit.agent.id), commit.expected_version) {
            (None, None) if commit.agent.version == 1 => {}
            (Some(current), Some(expected))
                if current.version == expected
                    && expected.checked_add(1) == Some(commit.agent.version) => {}
            _ => {
                return Err(AgentError::Conflict(format!(
                    "agent {} was concurrently modified",
                    commit.agent.id
                )))
            }
        }
        state.agents.insert(commit.agent.id, commit.agent.clone());
        state
            .states
            .entry(commit.agent.id)
            .or_default()
            .push(commit.state.clone());
        Ok(())
    }

    async fn find_agent(&self, id: Uuid) -> AgentResult<Option<Agent>> {
        Ok(self.read()?.agents.get(&id).cloned())
    }

    async fn list_agents(&self) -> AgentResult<Vec<Agent>> {
        Ok(self.read()?.agents.values().cloned().collect())
    }

    async fn list_states(&self, agent_id: Uuid) -> AgentResult<Vec<AgentStateRecord>> {
        Ok(self
            .read()?
            .states
            .get(&agent_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save_profile(&self, profile: &AgentProfile, actor: &str) -> AgentResult<()> {
        profile.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut state = self.write()?;
        if let Some(current) = state.profiles.get(&profile.id) {
            if current.key != profile.key
                || current.created_at != profile.created_at
                || profile.updated_at < current.updated_at
            {
                return Err(AgentError::Validation(
                    "profile key, creation time, or update order is invalid".into(),
                ));
            }
        }
        save_versioned(
            &mut state.profiles,
            profile.id,
            profile.version,
            |value| value.version,
            "profile",
        )?;
        state.profiles.insert(profile.id, profile.clone());
        Ok(())
    }

    async fn find_profile(&self, id: Uuid) -> AgentResult<Option<AgentProfile>> {
        Ok(self.read()?.profiles.get(&id).cloned())
    }

    async fn list_profiles(&self) -> AgentResult<Vec<AgentProfile>> {
        Ok(self.read()?.profiles.values().cloned().collect())
    }

    async fn save_policy(&self, policy: &AgentPolicyDefinition, actor: &str) -> AgentResult<()> {
        policy.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut state = self.write()?;
        if let Some(current) = state.policies.get(&policy.id) {
            if current.key != policy.key
                || current.created_at != policy.created_at
                || policy.updated_at < current.updated_at
            {
                return Err(AgentError::Validation(
                    "policy key, creation time, or update order is invalid".into(),
                ));
            }
        }
        save_versioned(
            &mut state.policies,
            policy.id,
            policy.version,
            |value| value.version,
            "policy",
        )?;
        state.policies.insert(policy.id, policy.clone());
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> AgentResult<Option<AgentPolicyDefinition>> {
        Ok(self.read()?.policies.get(&id).cloned())
    }

    async fn list_policies(&self) -> AgentResult<Vec<AgentPolicyDefinition>> {
        Ok(self.read()?.policies.values().cloned().collect())
    }

    async fn save_snapshot(&self, snapshot: &AgentSnapshot, actor: &str) -> AgentResult<()> {
        snapshot.validate()?;
        crate::domain::validate_actor(actor)?;
        let mut state = self.write()?;
        if !state.agents.contains_key(&snapshot.agent_id) {
            return Err(AgentError::NotFound(snapshot.agent_id.to_string()));
        }
        if state.snapshots.values().any(|value| {
            value.agent_id == snapshot.agent_id && value.agent_version == snapshot.agent_version
        }) {
            return Err(AgentError::Conflict(format!(
                "agent {} version {} already has a snapshot",
                snapshot.agent_id, snapshot.agent_version
            )));
        }
        if state.snapshots.contains_key(&snapshot.id) {
            return Err(AgentError::Conflict(format!(
                "snapshot {} already exists",
                snapshot.id
            )));
        }
        state.snapshots.insert(snapshot.id, snapshot.clone());
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> AgentResult<Option<AgentSnapshot>> {
        Ok(self.read()?.snapshots.get(&id).cloned())
    }

    async fn list_snapshots(&self, agent_id: Uuid) -> AgentResult<Vec<AgentSnapshot>> {
        let mut values = self
            .read()?
            .snapshots
            .values()
            .filter(|value| value.agent_id == agent_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.agent_version, value.created_at, value.id));
        Ok(values)
    }
}

impl InMemoryAgentStore {
    fn read(&self) -> AgentResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| AgentError::Internal("agent store lock poisoned".into()))
    }

    fn write(&self) -> AgentResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| AgentError::Internal("agent store lock poisoned".into()))
    }
}

fn same_agent_identity(current: &Agent, next: &Agent) -> bool {
    current.id == next.id
        && current.profile == next.profile
        && current.policy == next.policy
        && current.session_id == next.session_id
        && current.workspace_id == next.workspace_id
        && current.created_at == next.created_at
}

fn save_versioned<T>(
    values: &mut BTreeMap<Uuid, T>,
    id: Uuid,
    version: u64,
    current_version: impl Fn(&T) -> u64,
    label: &str,
) -> AgentResult<()> {
    let valid = match values.get(&id) {
        None => version == 1,
        Some(current) => current_version(current).checked_add(1) == Some(version),
    };
    if !valid {
        return Err(AgentError::Conflict(format!(
            "{label} {id} was concurrently modified"
        )));
    }
    Ok(())
}
