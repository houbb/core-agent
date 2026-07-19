use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use core_agent_execution::{Execution, ExecutionStatus};
use uuid::Uuid;

use crate::coordinator::UnavailableAgentCoordinator;
use crate::defaults::{
    DefaultAgentFactory, DefaultAgentLifecycle, EmbeddedAgentPolicy, InMemoryAgentStore,
};
use crate::domain::{
    Agent, AgentGoalRequest, AgentOperation, AgentPolicyDefinition, AgentProfile, AgentRunOutcome,
    AgentRunReference, AgentSnapshot, AgentState, AgentStateRecord, CreateAgentRequest,
};
use crate::error::{AgentError, AgentResult};
use crate::infrastructure::{
    AgentCommit, AgentCoordinator, AgentExecutionControl, AgentFactory, AgentInterceptor,
    AgentLifecycle, AgentObservation, AgentObserver, AgentPolicy, AgentStage, AgentStore,
};

pub struct AgentManagerBuilder {
    store: Arc<dyn AgentStore>,
    coordinator: Arc<dyn AgentCoordinator>,
    lifecycle: Arc<dyn AgentLifecycle>,
    policy: Arc<dyn AgentPolicy>,
    factory: Arc<dyn AgentFactory>,
    interceptors: Vec<Arc<dyn AgentInterceptor>>,
    observers: Vec<Arc<dyn AgentObserver>>,
}

impl Default for AgentManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryAgentStore::default()),
            coordinator: Arc::new(UnavailableAgentCoordinator),
            lifecycle: Arc::new(DefaultAgentLifecycle),
            policy: Arc::new(EmbeddedAgentPolicy),
            factory: Arc::new(DefaultAgentFactory),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl AgentManagerBuilder {
    pub fn store(mut self, value: Arc<dyn AgentStore>) -> Self {
        self.store = value;
        self
    }

    pub fn coordinator(mut self, value: Arc<dyn AgentCoordinator>) -> Self {
        self.coordinator = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn AgentLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn AgentPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn factory(mut self, value: Arc<dyn AgentFactory>) -> Self {
        self.factory = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn AgentInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn AgentObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> AgentManager {
        AgentManager {
            store: self.store,
            coordinator: self.coordinator,
            lifecycle: self.lifecycle,
            policy: self.policy,
            factory: self.factory,
            interceptors: self.interceptors,
            observers: self.observers,
            live: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

pub struct AgentManager {
    store: Arc<dyn AgentStore>,
    coordinator: Arc<dyn AgentCoordinator>,
    lifecycle: Arc<dyn AgentLifecycle>,
    policy: Arc<dyn AgentPolicy>,
    factory: Arc<dyn AgentFactory>,
    interceptors: Vec<Arc<dyn AgentInterceptor>>,
    observers: Vec<Arc<dyn AgentObserver>>,
    live: Arc<RwLock<HashMap<Uuid, Arc<LiveAgentEntry>>>>,
}

struct LiveAgentEntry {
    control: Arc<AgentExecutionControl>,
    stoppable: AtomicBool,
}

impl AgentManager {
    pub fn builder() -> AgentManagerBuilder {
        AgentManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn AgentStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn register_profile(
        &self,
        profile: AgentProfile,
        actor: &str,
    ) -> AgentResult<AgentProfile> {
        profile.validate()?;
        if let Some(policy_id) = profile.policy_id {
            if self.store.find_policy(policy_id).await?.is_none() {
                return Err(AgentError::NotFound(policy_id.to_string()));
            }
        }
        self.store.save_profile(&profile, actor).await?;
        Ok(profile)
    }

    pub async fn register_policy(
        &self,
        policy: AgentPolicyDefinition,
        actor: &str,
    ) -> AgentResult<AgentPolicyDefinition> {
        policy.validate()?;
        self.store.save_policy(&policy, actor).await?;
        Ok(policy)
    }

    pub async fn find_profile(&self, id: Uuid) -> AgentResult<Option<AgentProfile>> {
        self.store.find_profile(id).await
    }

    pub async fn list_profiles(&self) -> AgentResult<Vec<AgentProfile>> {
        self.store.list_profiles().await
    }

    pub async fn find_policy(&self, id: Uuid) -> AgentResult<Option<AgentPolicyDefinition>> {
        self.store.find_policy(id).await
    }

    pub async fn list_policies(&self) -> AgentResult<Vec<AgentPolicyDefinition>> {
        self.store.list_policies().await
    }

    pub async fn create(&self, mut request: CreateAgentRequest) -> AgentResult<Agent> {
        let profile = self
            .store
            .find_profile(request.profile_id)
            .await?
            .ok_or_else(|| AgentError::NotFound(request.profile_id.to_string()))?;
        request.policy = match profile.policy_id {
            Some(id) => Some(
                self.store
                    .find_policy(id)
                    .await?
                    .ok_or_else(|| AgentError::NotFound(id.to_string()))?,
            ),
            None => None,
        };
        let actor = request.actor.clone();
        let mut agent = self.factory.create(profile, request)?;
        self.policy
            .evaluate(AgentOperation::Create, &agent, &actor)?;
        let immutable = (
            agent.id,
            agent.profile.clone(),
            agent.policy.clone(),
            agent.state,
            agent.version,
            agent.actor.clone(),
        );
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_create(&mut agent)))
                .map_err(|_| AgentError::Extension("agent interceptor panicked".into()))??;
        }
        if immutable
            != (
                agent.id,
                agent.profile.clone(),
                agent.policy.clone(),
                agent.state,
                agent.version,
                agent.actor.clone(),
            )
        {
            return Err(AgentError::Validation(
                "agent interceptor changed immutable creation fields".into(),
            ));
        }
        agent.validate()?;
        let state = AgentStateRecord {
            id: Uuid::new_v4(),
            agent_id: agent.id,
            sequence: agent.version,
            from_state: None,
            to_state: AgentState::Created,
            goal_id: None,
            plan_id: None,
            execution_id: None,
            reason: "agent created from Profile snapshot".into(),
            actor: actor.clone(),
            created_at: agent.created_at,
        };
        self.store
            .commit(&AgentCommit::create(agent.clone(), state), &actor)
            .await?;
        self.notify(
            AgentOperation::Create,
            AgentStage::Persistence,
            true,
            &agent,
            None,
        );
        Ok(agent)
    }

    pub async fn start(&self, id: Uuid, actor: &str) -> AgentResult<Agent> {
        crate::domain::validate_actor(actor)?;
        let (live, _guard) = self.enter_live(id, false)?;
        let stop_requested = Arc::clone(&live.control);
        let mut agent = self.required(id).await?;
        self.policy.evaluate(AgentOperation::Start, &agent, actor)?;
        if agent.state == AgentState::Running {
            self.policy
                .evaluate(AgentOperation::Reconcile, &agent, actor)?;
            agent = self.reconcile_agent(agent, actor).await?;
            self.policy.evaluate(AgentOperation::Start, &agent, actor)?;
        }
        if matches!(agent.state, AgentState::Ready | AgentState::Waiting) {
            return Ok(agent);
        }
        if agent.state == AgentState::Paused {
            if let Some(execution_id) = agent.current_execution_id {
                live.stoppable.store(true, Ordering::SeqCst);
                self.transition(
                    &mut agent,
                    AgentState::Running,
                    actor,
                    "paused Execution resumed",
                )
                .await?;
                if stop_requested.is_stop_requested() {
                    live.stoppable.store(false, Ordering::SeqCst);
                    let outcome_actor =
                        stop_requested.stop_actor()?.unwrap_or_else(|| actor.into());
                    if let Err(error) = self
                        .transition(
                            &mut agent,
                            AgentState::Paused,
                            &outcome_actor,
                            "resume stopped before Execution start",
                        )
                        .await
                    {
                        self.fail_agent(id, &outcome_actor, &error).await?;
                        return Err(error);
                    }
                    return Ok(agent);
                }
                let execution_result = self
                    .coordinator
                    .resume(execution_id, actor, &stop_requested)
                    .await;
                let execution = match execution_result {
                    Ok(value) => value,
                    Err(_) if stop_requested.is_stop_requested() => {
                        match self.coordinator.pause(execution_id).await {
                            Ok(value) => value,
                            Err(error) => {
                                live.stoppable.store(false, Ordering::SeqCst);
                                let outcome_actor =
                                    stop_requested.stop_actor()?.unwrap_or_else(|| actor.into());
                                self.fail_agent(id, &outcome_actor, &error).await?;
                                return Err(error);
                            }
                        }
                    }
                    Err(error) => {
                        live.stoppable.store(false, Ordering::SeqCst);
                        self.fail_agent(id, actor, &error).await?;
                        return Err(error);
                    }
                };
                live.stoppable.store(false, Ordering::SeqCst);
                let outcome_actor = if execution.status == ExecutionStatus::Paused {
                    stop_requested.stop_actor()?.unwrap_or_else(|| actor.into())
                } else {
                    actor.into()
                };
                if let Err(error) = self
                    .apply_execution(&mut agent, &execution, &outcome_actor)
                    .await
                {
                    self.fail_agent(id, &outcome_actor, &error).await?;
                    return Err(error);
                }
                return Ok(agent);
            }
        }
        if !matches!(
            agent.state,
            AgentState::Created | AgentState::Paused | AgentState::Failed | AgentState::Completed
        ) {
            return Err(AgentError::InvalidState(format!(
                "cannot start {} agent",
                agent.state.as_str()
            )));
        }
        agent.current_goal_id = None;
        agent.current_plan_id = None;
        agent.current_execution_id = None;
        agent.last_error_kind = None;
        agent.last_error_message = None;
        self.transition(&mut agent, AgentState::Ready, actor, "agent started")
            .await?;
        Ok(agent)
    }

    pub async fn run_goal(
        &self,
        id: Uuid,
        request: AgentGoalRequest,
    ) -> AgentResult<AgentRunOutcome> {
        let (live, _guard) = self.enter_live(id, false)?;
        self.run_goal_inner(id, request, live).await
    }

    async fn run_goal_inner(
        &self,
        id: Uuid,
        mut request: AgentGoalRequest,
        live: Arc<LiveAgentEntry>,
    ) -> AgentResult<AgentRunOutcome> {
        let stop_requested = Arc::clone(&live.control);
        let mut agent = self.required(id).await?;
        if !matches!(agent.state, AgentState::Ready | AgentState::Waiting) {
            return Err(AgentError::InvalidState(format!(
                "cannot run Goal while Agent is {}",
                agent.state.as_str()
            )));
        }
        let actor = request.goal.actor.clone();
        crate::domain::validate_actor(&actor)?;
        self.policy.evaluate(AgentOperation::Run, &agent, &actor)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.before_goal(&agent, &mut request)
            }))
            .map_err(|_| AgentError::Extension("agent interceptor panicked".into()))??;
        }
        if request.goal.actor != actor {
            return Err(AgentError::Validation(
                "agent interceptor changed the operation actor".into(),
            ));
        }
        agent.current_goal_id = None;
        agent.current_plan_id = None;
        agent.current_execution_id = None;
        agent.last_error_kind = None;
        agent.last_error_message = None;
        self.transition(
            &mut agent,
            AgentState::Running,
            &actor,
            "Goal accepted for planning",
        )
        .await?;
        live.stoppable.store(true, Ordering::SeqCst);

        let reference = match self.coordinator.next(&agent, request).await {
            Ok(value) => value,
            Err(error) => {
                live.stoppable.store(false, Ordering::SeqCst);
                if let Some((goal_id, plan_id, execution_id)) = error.partial_reference() {
                    agent.current_goal_id = goal_id;
                    agent.current_plan_id = plan_id;
                    agent.current_execution_id = execution_id;
                    if let Err(progress_error) = self
                        .progress(
                            &mut agent,
                            &actor,
                            "partial coordination references recorded",
                        )
                        .await
                    {
                        let _ = self
                            .fail_agent_with_partial_reference(
                                id,
                                &actor,
                                &progress_error,
                                goal_id,
                                plan_id,
                                execution_id,
                            )
                            .await;
                        return Err(progress_error);
                    }
                }
                self.fail_agent(id, &actor, &error).await?;
                if let Ok(failed) = self.required(id).await {
                    self.notify(
                        AgentOperation::Run,
                        AgentStage::Planning,
                        false,
                        &failed,
                        Some(error.to_string()),
                    );
                }
                return Err(error);
            }
        };
        agent.current_goal_id = Some(reference.goal_id);
        agent.current_plan_id = Some(reference.plan_id);
        agent.current_execution_id = Some(reference.execution_id);
        if let Err(error) = self
            .progress(&mut agent, &actor, "Goal planned and Execution prepared")
            .await
        {
            let _ = self.coordinator.pause(reference.execution_id).await;
            let failure = AgentError::PartialCoordination {
                stage: "PERSIST_AGENT_REFERENCE".into(),
                goal_id: Some(reference.goal_id),
                plan_id: Some(reference.plan_id),
                execution_id: Some(reference.execution_id),
                message: error.to_string(),
            };
            let _ = self
                .fail_agent_with_reference(id, &actor, &failure, Some(&reference))
                .await;
            return Err(failure);
        }
        self.notify(
            AgentOperation::Run,
            AgentStage::Planning,
            true,
            &agent,
            None,
        );

        let execution_result = if stop_requested.is_stop_requested() {
            self.coordinator.pause(reference.execution_id).await
        } else {
            self.coordinator.run(&reference, &stop_requested).await
        };
        let execution = match execution_result {
            Ok(value) => value,
            Err(_) if stop_requested.is_stop_requested() => {
                match self.coordinator.pause(reference.execution_id).await {
                    Ok(value) => value,
                    Err(error) => {
                        live.stoppable.store(false, Ordering::SeqCst);
                        let outcome_actor = stop_requested.stop_actor()?.unwrap_or(actor.clone());
                        self.fail_agent_with_reference(
                            id,
                            &outcome_actor,
                            &error,
                            Some(&reference),
                        )
                        .await?;
                        return Err(error);
                    }
                }
            }
            Err(error) => {
                live.stoppable.store(false, Ordering::SeqCst);
                self.fail_agent(id, &actor, &error).await?;
                return Err(error);
            }
        };
        live.stoppable.store(false, Ordering::SeqCst);
        let outcome_actor = if execution.status == ExecutionStatus::Paused {
            stop_requested.stop_actor()?.unwrap_or(actor.clone())
        } else {
            actor.clone()
        };
        if let Err(error) = self
            .apply_execution(&mut agent, &execution, &outcome_actor)
            .await
        {
            let _ = self.fail_agent(id, &actor, &error).await;
            return Err(error);
        }
        Ok(AgentRunOutcome {
            agent,
            reference,
            execution_status: execution.status,
        })
    }

    pub async fn stop(&self, id: Uuid, actor: &str) -> AgentResult<bool> {
        crate::domain::validate_actor(actor)?;
        if let Some(live) = self.live_entry(id)? {
            return self.request_live_stop(id, actor, live).await;
        }
        let (_live, _guard) = match self.enter_live(id, false) {
            Ok(value) => value,
            Err(AgentError::Conflict(_)) => {
                if let Some(live) = self.live_entry(id)? {
                    return self.request_live_stop(id, actor, live).await;
                }
                return Err(AgentError::Conflict(format!("agent {id} is active")));
            }
            Err(error) => return Err(error),
        };
        let mut agent = self.required(id).await?;
        self.policy.evaluate(AgentOperation::Stop, &agent, actor)?;
        if agent.state == AgentState::Running {
            self.policy
                .evaluate(AgentOperation::Reconcile, &agent, actor)?;
            agent = self.reconcile_agent(agent, actor).await?;
            self.policy.evaluate(AgentOperation::Stop, &agent, actor)?;
        }
        if agent.state == AgentState::Paused {
            return Ok(false);
        }
        if let Some(execution_id) = agent.current_execution_id {
            if agent.state == AgentState::Running {
                let _ = self.coordinator.pause(execution_id).await?;
            }
        }
        if !matches!(
            agent.state,
            AgentState::Ready | AgentState::Waiting | AgentState::Running
        ) {
            return Err(AgentError::InvalidState(format!(
                "cannot stop {} agent",
                agent.state.as_str()
            )));
        }
        if matches!(agent.state, AgentState::Ready | AgentState::Waiting) {
            agent.current_goal_id = None;
            agent.current_plan_id = None;
            agent.current_execution_id = None;
        }
        self.transition(&mut agent, AgentState::Paused, actor, "agent stopped")
            .await?;
        Ok(true)
    }

    pub async fn finish(&self, id: Uuid, actor: &str) -> AgentResult<Agent> {
        crate::domain::validate_actor(actor)?;
        let (_live, _guard) = self.enter_live(id, false)?;
        let mut agent = self.required(id).await?;
        self.policy
            .evaluate(AgentOperation::Finish, &agent, actor)?;
        if agent.state == AgentState::Running {
            self.policy
                .evaluate(AgentOperation::Reconcile, &agent, actor)?;
            agent = self.reconcile_agent(agent, actor).await?;
            self.policy
                .evaluate(AgentOperation::Finish, &agent, actor)?;
        }
        if !matches!(agent.state, AgentState::Ready | AgentState::Waiting) {
            return Err(AgentError::InvalidState(format!(
                "cannot finish {} agent",
                agent.state.as_str()
            )));
        }
        self.transition(
            &mut agent,
            AgentState::Completed,
            actor,
            "agent lifecycle finished",
        )
        .await?;
        Ok(agent)
    }

    pub async fn destroy(&self, id: Uuid, actor: &str) -> AgentResult<Agent> {
        crate::domain::validate_actor(actor)?;
        let (_live, _guard) = self.enter_live(id, false)?;
        let mut agent = self.required(id).await?;
        self.policy
            .evaluate(AgentOperation::Destroy, &agent, actor)?;
        if agent.state == AgentState::Running {
            self.policy
                .evaluate(AgentOperation::Reconcile, &agent, actor)?;
            agent = self.reconcile_agent(agent, actor).await?;
            self.policy
                .evaluate(AgentOperation::Destroy, &agent, actor)?;
        }
        if matches!(agent.state, AgentState::Running | AgentState::Destroyed) {
            return Err(AgentError::InvalidState(format!(
                "cannot destroy {} agent",
                agent.state.as_str()
            )));
        }
        agent.current_goal_id = None;
        agent.current_plan_id = None;
        agent.current_execution_id = None;
        self.transition(&mut agent, AgentState::Destroyed, actor, "agent destroyed")
            .await?;
        Ok(agent)
    }

    pub async fn save_snapshot(
        &self,
        id: Uuid,
        label: impl Into<String>,
        actor: &str,
    ) -> AgentResult<AgentSnapshot> {
        crate::domain::validate_actor(actor)?;
        let (_live, _guard) = self.enter_live(id, false)?;
        let agent = self.required(id).await?;
        self.policy
            .evaluate(AgentOperation::Snapshot, &agent, actor)?;
        let snapshot = AgentSnapshot::new(&agent, label)?;
        self.store.save_snapshot(&snapshot, actor).await?;
        self.notify(
            AgentOperation::Snapshot,
            AgentStage::Snapshot,
            true,
            &agent,
            None,
        );
        Ok(snapshot)
    }

    pub async fn restore_snapshot(&self, id: Uuid, actor: &str) -> AgentResult<Agent> {
        crate::domain::validate_actor(actor)?;
        let snapshot = self
            .store
            .find_snapshot(id)
            .await?
            .ok_or_else(|| AgentError::NotFound(id.to_string()))?;
        snapshot.validate()?;
        let (_live, _guard) = self.enter_live(snapshot.agent_id, false)?;
        let current = self.required(snapshot.agent_id).await?;
        self.policy
            .evaluate(AgentOperation::Restore, &current, actor)?;
        if current.version != snapshot.agent_version || current != snapshot.content {
            return Err(AgentError::Conflict(
                "only the current Agent snapshot can be restored without replaying side effects"
                    .into(),
            ));
        }
        let mut restored = snapshot.content;
        self.progress(&mut restored, actor, "current Agent snapshot restored")
            .await?;
        self.notify(
            AgentOperation::Restore,
            AgentStage::Snapshot,
            true,
            &restored,
            None,
        );
        Ok(restored)
    }

    pub async fn find(&self, id: Uuid) -> AgentResult<Option<Agent>> {
        self.store.find_agent(id).await
    }

    pub async fn list(&self) -> AgentResult<Vec<Agent>> {
        self.store.list_agents().await
    }

    pub async fn list_states(&self, id: Uuid) -> AgentResult<Vec<AgentStateRecord>> {
        self.store.list_states(id).await
    }

    pub async fn list_snapshots(&self, id: Uuid) -> AgentResult<Vec<AgentSnapshot>> {
        self.store.list_snapshots(id).await
    }

    /// Reconciles a durable orphaned RUNNING Agent after process/task loss.
    /// Lower-runtime side effects are never replayed here; a non-boundary
    /// Execution is converted to a resumable Agent PAUSED boundary.
    pub async fn reconcile(&self, id: Uuid, actor: &str) -> AgentResult<Agent> {
        crate::domain::validate_actor(actor)?;
        let (_live, _guard) = self.enter_live(id, false)?;
        let agent = self.required(id).await?;
        self.policy
            .evaluate(AgentOperation::Reconcile, &agent, actor)?;
        self.reconcile_agent(agent, actor).await
    }

    async fn required(&self, id: Uuid) -> AgentResult<Agent> {
        self.store
            .find_agent(id)
            .await?
            .ok_or_else(|| AgentError::NotFound(id.to_string()))
    }

    async fn request_live_stop(
        &self,
        id: Uuid,
        actor: &str,
        live: Arc<LiveAgentEntry>,
    ) -> AgentResult<bool> {
        if !live.stoppable.load(Ordering::SeqCst) {
            return Err(AgentError::Conflict(format!(
                "agent {id} is active in a non-stoppable operation"
            )));
        }
        let agent = self.required(id).await?;
        self.policy.evaluate(AgentOperation::Stop, &agent, actor)?;
        if agent.state == AgentState::Running {
            if let Some(execution_id) = agent.current_execution_id {
                self.verify_live_stop(id, &live)?;
                let execution = self.coordinator.pause(execution_id).await?;
                if execution.status.is_terminal() {
                    return Err(AgentError::Conflict(format!(
                        "execution {execution_id} reached terminal status {} before the stop boundary",
                        execution.status.as_str()
                    )));
                }
            }
        }
        self.accept_live_stop(id, &live, actor)?;
        Ok(true)
    }

    fn verify_live_stop(&self, id: Uuid, expected: &Arc<LiveAgentEntry>) -> AgentResult<()> {
        let live = self
            .live
            .read()
            .map_err(|_| AgentError::Internal("live Agent lock poisoned".into()))?;
        let current = live
            .get(&id)
            .ok_or_else(|| AgentError::Conflict(format!("agent {id} is no longer active")))?;
        if !Arc::ptr_eq(current, expected) || !current.stoppable.load(Ordering::SeqCst) {
            return Err(AgentError::Conflict(format!(
                "agent {id} no longer accepts cooperative stop"
            )));
        }
        Ok(())
    }

    fn accept_live_stop(
        &self,
        id: Uuid,
        expected: &Arc<LiveAgentEntry>,
        actor: &str,
    ) -> AgentResult<()> {
        let live = self
            .live
            .read()
            .map_err(|_| AgentError::Internal("live Agent lock poisoned".into()))?;
        let current = live
            .get(&id)
            .ok_or_else(|| AgentError::Conflict(format!("agent {id} is no longer active")))?;
        if !Arc::ptr_eq(current, expected) || !current.stoppable.load(Ordering::SeqCst) {
            return Err(AgentError::Conflict(format!(
                "agent {id} no longer accepts cooperative stop"
            )));
        }
        current.control.request_stop(actor)
    }

    async fn reconcile_agent(&self, mut agent: Agent, actor: &str) -> AgentResult<Agent> {
        if agent.state != AgentState::Running {
            return Ok(agent);
        }
        if let Some(execution_id) = agent.current_execution_id {
            match self.coordinator.find_execution(execution_id).await? {
                Some(execution)
                    if matches!(
                        execution.status,
                        ExecutionStatus::Completed
                            | ExecutionStatus::Paused
                            | ExecutionStatus::Failed
                            | ExecutionStatus::Cancelled
                    ) =>
                {
                    self.apply_execution(&mut agent, &execution, actor).await?;
                }
                Some(execution) if execution.has_uncertain_action() => {
                    agent.failed_goals = agent.failed_goals.saturating_add(1);
                    agent.last_error_kind = Some("OUTCOME_UNKNOWN".into());
                    agent.last_error_message = Some(format!(
                        "Execution {execution_id} lost ownership with an in-flight command"
                    ));
                    self.transition(
                        &mut agent,
                        AgentState::Failed,
                        actor,
                        "orphaned Agent has an outcome-unknown Execution",
                    )
                    .await?;
                }
                Some(_) => match self.coordinator.pause(execution_id).await {
                    Ok(paused) if paused.status == ExecutionStatus::Paused => {
                        self.apply_execution(&mut agent, &paused, actor).await?;
                    }
                    Ok(value) => {
                        return Err(AgentError::InvalidState(format!(
                            "recovery pause returned {}",
                            value.status.as_str()
                        )))
                    }
                    Err(error) => {
                        agent.failed_goals = agent.failed_goals.saturating_add(1);
                        agent.last_error_kind = Some("RECOVERY_FAILED".into());
                        agent.last_error_message = Some(truncate(&error.to_string(), 1024));
                        self.transition(
                            &mut agent,
                            AgentState::Failed,
                            actor,
                            "orphaned Execution could not reach a safe boundary",
                        )
                        .await?;
                    }
                },
                None => {
                    agent.failed_goals = agent.failed_goals.saturating_add(1);
                    agent.last_error_kind = Some("EXECUTION_NOT_FOUND".into());
                    agent.last_error_message = Some(format!(
                        "Execution {execution_id} is missing during Agent recovery"
                    ));
                    self.transition(
                        &mut agent,
                        AgentState::Failed,
                        actor,
                        "orphaned Agent references a missing Execution",
                    )
                    .await?;
                }
            }
        } else {
            agent.failed_goals = agent.failed_goals.saturating_add(1);
            agent.last_error_kind = Some("PLANNING_INTERRUPTED".into());
            agent.last_error_message = Some("Agent lost ownership during Planning".into());
            self.transition(
                &mut agent,
                AgentState::Failed,
                actor,
                "orphaned Agent recovered after interrupted Planning",
            )
            .await?;
        }
        Ok(agent)
    }

    async fn transition(
        &self,
        agent: &mut Agent,
        next: AgentState,
        actor: &str,
        reason: &str,
    ) -> AgentResult<()> {
        let expected = agent.version;
        let immutable = lifecycle_identity(agent);
        let state = self.lifecycle.transition(agent, next, actor, reason)?;
        if immutable != lifecycle_identity(agent) {
            return Err(AgentError::Validation(
                "Agent lifecycle changed immutable identity or bindings".into(),
            ));
        }
        self.store
            .commit(&AgentCommit::update(agent.clone(), expected, state), actor)
            .await?;
        self.notify(
            operation_for_state(next),
            AgentStage::Lifecycle,
            next != AgentState::Failed,
            agent,
            (next == AgentState::Failed)
                .then(|| agent.last_error_message.clone())
                .flatten(),
        );
        Ok(())
    }

    async fn progress(&self, agent: &mut Agent, actor: &str, reason: &str) -> AgentResult<()> {
        let expected = agent.version;
        let immutable = lifecycle_identity(agent);
        let state = self.lifecycle.record_progress(agent, actor, reason)?;
        if immutable != lifecycle_identity(agent) {
            return Err(AgentError::Validation(
                "Agent lifecycle changed immutable identity or bindings".into(),
            ));
        }
        self.store
            .commit(&AgentCommit::update(agent.clone(), expected, state), actor)
            .await
    }

    async fn apply_execution(
        &self,
        agent: &mut Agent,
        execution: &Execution,
        actor: &str,
    ) -> AgentResult<()> {
        if agent.current_execution_id != Some(execution.id) {
            return Err(AgentError::Validation(
                "Execution outcome belongs to another Agent run".into(),
            ));
        }
        match execution.status {
            ExecutionStatus::Completed => {
                agent.completed_goals = agent.completed_goals.saturating_add(1);
                agent.last_error_kind = None;
                agent.last_error_message = None;
                self.transition(
                    agent,
                    AgentState::Waiting,
                    actor,
                    "Goal Execution completed; Agent awaits next Goal",
                )
                .await?;
            }
            ExecutionStatus::Paused => {
                self.transition(agent, AgentState::Paused, actor, "Goal Execution paused")
                    .await?;
            }
            ExecutionStatus::Cancelled => {
                agent.failed_goals = agent.failed_goals.saturating_add(1);
                agent.last_error_kind = Some("EXECUTION_CANCELLED".into());
                agent.last_error_message = Some("Execution reached CANCELLED".into());
                self.transition(
                    agent,
                    AgentState::Failed,
                    actor,
                    "Goal Execution was cancelled",
                )
                .await?;
            }
            ExecutionStatus::Failed => {
                agent.failed_goals = agent.failed_goals.saturating_add(1);
                agent.last_error_kind = Some("EXECUTION_FAILED".into());
                agent.last_error_message = Some("Execution reached FAILED".into());
                self.transition(agent, AgentState::Failed, actor, "Goal Execution failed")
                    .await?;
            }
            status => {
                return Err(AgentError::InvalidState(format!(
                    "Execution returned non-boundary status {}",
                    status.as_str()
                )))
            }
        }
        self.notify(
            AgentOperation::Run,
            AgentStage::Execution,
            execution.status == ExecutionStatus::Completed,
            agent,
            None,
        );
        Ok(())
    }

    async fn fail_agent(&self, id: Uuid, actor: &str, error: &AgentError) -> AgentResult<()> {
        self.fail_agent_with_partial_reference(id, actor, error, None, None, None)
            .await
    }

    async fn fail_agent_with_reference(
        &self,
        id: Uuid,
        actor: &str,
        error: &AgentError,
        reference: Option<&AgentRunReference>,
    ) -> AgentResult<()> {
        let (goal_id, plan_id, execution_id) = reference
            .map(|value| {
                (
                    Some(value.goal_id),
                    Some(value.plan_id),
                    Some(value.execution_id),
                )
            })
            .unwrap_or((None, None, None));
        self.fail_agent_with_partial_reference(id, actor, error, goal_id, plan_id, execution_id)
            .await
    }

    async fn fail_agent_with_partial_reference(
        &self,
        id: Uuid,
        actor: &str,
        error: &AgentError,
        goal_id: Option<Uuid>,
        plan_id: Option<Uuid>,
        execution_id: Option<Uuid>,
    ) -> AgentResult<()> {
        let mut agent = self.required(id).await?;
        if agent.state != AgentState::Running {
            return Ok(());
        }
        agent.current_goal_id = goal_id.or(agent.current_goal_id);
        agent.current_plan_id = plan_id.or(agent.current_plan_id);
        agent.current_execution_id = execution_id.or(agent.current_execution_id);
        agent.failed_goals = agent.failed_goals.saturating_add(1);
        agent.last_error_kind = Some(error.kind().into());
        agent.last_error_message = Some(truncate(&error.to_string(), 1024));
        self.transition(
            &mut agent,
            AgentState::Failed,
            actor,
            "Agent coordination failed",
        )
        .await
    }

    fn live_entry(&self, id: Uuid) -> AgentResult<Option<Arc<LiveAgentEntry>>> {
        self.live
            .read()
            .map(|value| value.get(&id).cloned())
            .map_err(|_| AgentError::Internal("live Agent lock poisoned".into()))
    }

    fn enter_live(
        &self,
        id: Uuid,
        stoppable: bool,
    ) -> AgentResult<(Arc<LiveAgentEntry>, LiveAgentGuard)> {
        let control = Arc::new(AgentExecutionControl::default());
        let live_entry = Arc::new(LiveAgentEntry {
            control,
            stoppable: AtomicBool::new(stoppable),
        });
        let mut live = self
            .live
            .write()
            .map_err(|_| AgentError::Internal("live Agent lock poisoned".into()))?;
        match live.entry(id) {
            std::collections::hash_map::Entry::Occupied(_) => {
                return Err(AgentError::Conflict(format!("agent {id} is active")))
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&live_entry));
            }
        }
        drop(live);
        Ok((
            live_entry,
            LiveAgentGuard {
                id,
                live: Arc::clone(&self.live),
            },
        ))
    }

    fn notify(
        &self,
        operation: AgentOperation,
        stage: AgentStage,
        success: bool,
        agent: &Agent,
        message: Option<String>,
    ) {
        let observation = AgentObservation {
            operation,
            stage,
            success,
            agent_id: agent.id,
            state: agent.state,
            goal_id: agent.current_goal_id,
            plan_id: agent.current_plan_id,
            execution_id: agent.current_execution_id,
            actor: agent.actor.clone(),
            message,
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

struct LiveAgentGuard {
    id: Uuid,
    live: Arc<RwLock<HashMap<Uuid, Arc<LiveAgentEntry>>>>,
}

impl Drop for LiveAgentGuard {
    fn drop(&mut self) {
        if let Ok(mut live) = self.live.write() {
            live.remove(&self.id);
        }
    }
}

fn operation_for_state(state: AgentState) -> AgentOperation {
    match state {
        AgentState::Ready => AgentOperation::Start,
        AgentState::Running | AgentState::Waiting | AgentState::Failed => AgentOperation::Run,
        AgentState::Paused => AgentOperation::Stop,
        AgentState::Completed => AgentOperation::Finish,
        AgentState::Destroyed => AgentOperation::Destroy,
        AgentState::Created => AgentOperation::Create,
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.into();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

fn lifecycle_identity(
    agent: &Agent,
) -> (
    Uuid,
    AgentProfile,
    Option<AgentPolicyDefinition>,
    Option<Uuid>,
    Option<Uuid>,
    chrono::DateTime<chrono::Utc>,
) {
    (
        agent.id,
        agent.profile.clone(),
        agent.policy.clone(),
        agent.session_id,
        agent.workspace_id,
        agent.created_at,
    )
}
