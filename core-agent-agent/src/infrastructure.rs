use async_trait::async_trait;
use core_agent_execution::{Execution, ExecutionControl};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::domain::{
    Agent, AgentGoalRequest, AgentOperation, AgentPolicyDefinition, AgentProfile,
    AgentRunReference, AgentSnapshot, AgentState, AgentStateRecord, CreateAgentRequest,
};
use crate::error::AgentResult;

#[derive(Clone)]
pub struct AgentExecutionControl {
    execution: ExecutionControl,
    stop_requested: Arc<AtomicBool>,
    stop_actor: Arc<RwLock<Option<String>>>,
}

impl Default for AgentExecutionControl {
    fn default() -> Self {
        Self {
            execution: ExecutionControl::default(),
            stop_requested: Arc::new(AtomicBool::new(false)),
            stop_actor: Arc::new(RwLock::new(None)),
        }
    }
}

impl AgentExecutionControl {
    pub fn request_stop(&self, actor: &str) -> AgentResult<()> {
        crate::domain::validate_actor(actor)?;
        let mut value = self.stop_actor.write().map_err(|_| {
            crate::error::AgentError::Internal("Agent stop actor lock poisoned".into())
        })?;
        *value = Some(actor.into());
        self.execution.request_pause();
        self.stop_requested.store(true, Ordering::Release);
        Ok(())
    }

    pub fn is_stop_requested(&self) -> bool {
        self.stop_requested.load(Ordering::Acquire)
    }

    pub fn stop_actor(&self) -> AgentResult<Option<String>> {
        self.stop_actor
            .read()
            .map(|value| value.clone())
            .map_err(|_| {
                crate::error::AgentError::Internal("Agent stop actor lock poisoned".into())
            })
    }

    pub fn execution_control(&self) -> ExecutionControl {
        self.execution.clone()
    }
}

#[derive(Debug, Clone)]
pub struct AgentCommit {
    pub agent: Agent,
    pub expected_version: Option<u64>,
    pub state: AgentStateRecord,
}

impl AgentCommit {
    pub fn create(agent: Agent, state: AgentStateRecord) -> Self {
        Self {
            agent,
            expected_version: None,
            state,
        }
    }

    pub fn update(agent: Agent, expected_version: u64, state: AgentStateRecord) -> Self {
        Self {
            agent,
            expected_version: Some(expected_version),
            state,
        }
    }

    pub fn validate(&self, actor: &str) -> AgentResult<()> {
        self.agent.validate()?;
        crate::domain::validate_actor(actor)?;
        crate::domain::validate_text("agent state reason", &self.state.reason, 1024)?;
        if match self.expected_version {
            None => self.agent.version != 1 || self.state.from_state.is_some(),
            Some(value) => {
                value.checked_add(1) != Some(self.agent.version) || self.state.from_state.is_none()
            }
        } || self.state.agent_id != self.agent.id
            || self.state.sequence != self.agent.version
            || self.state.to_state != self.agent.state
            || self.state.goal_id != self.agent.current_goal_id
            || self.state.plan_id != self.agent.current_plan_id
            || self.state.execution_id != self.agent.current_execution_id
            || self.state.created_at != self.agent.updated_at
            || self.state.actor != actor
        {
            return Err(crate::error::AgentError::Validation(
                "agent commit does not match aggregate".into(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn commit(&self, commit: &AgentCommit, actor: &str) -> AgentResult<()>;
    async fn find_agent(&self, id: Uuid) -> AgentResult<Option<Agent>>;
    async fn list_agents(&self) -> AgentResult<Vec<Agent>>;
    async fn list_states(&self, agent_id: Uuid) -> AgentResult<Vec<AgentStateRecord>>;

    async fn save_profile(&self, profile: &AgentProfile, actor: &str) -> AgentResult<()>;
    async fn find_profile(&self, id: Uuid) -> AgentResult<Option<AgentProfile>>;
    async fn list_profiles(&self) -> AgentResult<Vec<AgentProfile>>;

    async fn save_policy(&self, policy: &AgentPolicyDefinition, actor: &str) -> AgentResult<()>;
    async fn find_policy(&self, id: Uuid) -> AgentResult<Option<AgentPolicyDefinition>>;
    async fn list_policies(&self) -> AgentResult<Vec<AgentPolicyDefinition>>;

    async fn save_snapshot(&self, snapshot: &AgentSnapshot, actor: &str) -> AgentResult<()>;
    async fn find_snapshot(&self, id: Uuid) -> AgentResult<Option<AgentSnapshot>>;
    async fn list_snapshots(&self, agent_id: Uuid) -> AgentResult<Vec<AgentSnapshot>>;
}

pub trait AgentRegistry: AgentStore {}
impl<T> AgentRegistry for T where T: AgentStore {}

pub trait AgentLifecycle: Send + Sync {
    fn transition(
        &self,
        agent: &mut Agent,
        next: AgentState,
        actor: &str,
        reason: &str,
    ) -> AgentResult<AgentStateRecord>;

    fn record_progress(
        &self,
        agent: &mut Agent,
        actor: &str,
        reason: &str,
    ) -> AgentResult<AgentStateRecord>;
}

pub trait AgentPolicy: Send + Sync {
    fn evaluate(&self, operation: AgentOperation, agent: &Agent, actor: &str) -> AgentResult<()>;
}

pub trait AgentFactory: Send + Sync {
    fn create(&self, profile: AgentProfile, request: CreateAgentRequest) -> AgentResult<Agent>;
}

pub trait AgentInterceptor: Send + Sync {
    fn before_create(&self, _agent: &mut Agent) -> AgentResult<()> {
        Ok(())
    }

    fn before_goal(&self, _agent: &Agent, _request: &mut AgentGoalRequest) -> AgentResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStage {
    Lifecycle,
    Planning,
    Execution,
    Snapshot,
    Persistence,
}

#[derive(Debug, Clone)]
pub struct AgentObservation {
    pub operation: AgentOperation,
    pub stage: AgentStage,
    pub success: bool,
    pub agent_id: Uuid,
    pub state: AgentState,
    pub goal_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub actor: String,
    pub message: Option<String>,
}

pub trait AgentObserver: Send + Sync {
    fn on_observation(&self, observation: &AgentObservation);
}

#[async_trait]
pub trait AgentCoordinator: Send + Sync {
    async fn next(
        &self,
        agent: &Agent,
        request: AgentGoalRequest,
    ) -> AgentResult<AgentRunReference>;
    async fn run(
        &self,
        reference: &AgentRunReference,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution>;
    async fn resume(
        &self,
        execution_id: Uuid,
        actor: &str,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution>;
    async fn pause(&self, execution_id: Uuid) -> AgentResult<Execution>;
    async fn find_execution(&self, execution_id: Uuid) -> AgentResult<Option<Execution>>;
}

pub trait AgentSnapshotStore: AgentStore {}
impl<T> AgentSnapshotStore for T where T: AgentStore {}
