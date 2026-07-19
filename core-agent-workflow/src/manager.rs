use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{
    action_definition, action_progress_mut, initial_state_record, refresh_progress,
    DefaultWorkflowLifecycle, EmbeddedWorkflowPolicy, InMemoryWorkflowRegistry,
    InMemoryWorkflowStore, SequentialWorkflowScheduler, UnavailableWorkflowEngine,
};
use crate::domain::{
    validate_actor, validate_variables, StartWorkflowRequest, WorkItemState, WorkflowActionOutcome,
    WorkflowDefinition, WorkflowIdentity, WorkflowInstance, WorkflowSnapshot, WorkflowState,
    WorkflowStateRecord,
};
use crate::error::{WorkflowError, WorkflowResult};
use crate::infrastructure::{
    WorkflowActionContext, WorkflowControl, WorkflowEngine, WorkflowInstanceCommit,
    WorkflowInterceptor, WorkflowLifecycle, WorkflowObservation, WorkflowObserver,
    WorkflowOperation, WorkflowPolicy, WorkflowRegistrationCommit, WorkflowRegistry,
    WorkflowScheduler, WorkflowStage, WorkflowStore,
};

pub struct WorkflowManagerBuilder {
    store: Arc<dyn WorkflowStore>,
    registry: Arc<dyn WorkflowRegistry>,
    scheduler: Arc<dyn WorkflowScheduler>,
    engine: Arc<dyn WorkflowEngine>,
    policy: Arc<dyn WorkflowPolicy>,
    lifecycle: Arc<dyn WorkflowLifecycle>,
    interceptors: Vec<Arc<dyn WorkflowInterceptor>>,
    observers: Vec<Arc<dyn WorkflowObserver>>,
}

impl Default for WorkflowManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryWorkflowStore::default()),
            registry: Arc::new(InMemoryWorkflowRegistry::default()),
            scheduler: Arc::new(SequentialWorkflowScheduler),
            engine: Arc::new(UnavailableWorkflowEngine),
            policy: Arc::new(EmbeddedWorkflowPolicy),
            lifecycle: Arc::new(DefaultWorkflowLifecycle),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl WorkflowManagerBuilder {
    pub fn store(mut self, value: Arc<dyn WorkflowStore>) -> Self {
        self.store = value;
        self
    }

    pub fn registry(mut self, value: Arc<dyn WorkflowRegistry>) -> Self {
        self.registry = value;
        self
    }

    pub fn scheduler(mut self, value: Arc<dyn WorkflowScheduler>) -> Self {
        self.scheduler = value;
        self
    }

    pub fn engine(mut self, value: Arc<dyn WorkflowEngine>) -> Self {
        self.engine = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn WorkflowPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn WorkflowLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn WorkflowInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn WorkflowObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> WorkflowManager {
        WorkflowManager {
            store: self.store,
            registry: self.registry,
            scheduler: self.scheduler,
            engine: self.engine,
            policy: self.policy,
            lifecycle: self.lifecycle,
            interceptors: self.interceptors,
            observers: self.observers,
            live: Mutex::new(HashMap::new()),
        }
    }
}

pub struct WorkflowManager {
    store: Arc<dyn WorkflowStore>,
    registry: Arc<dyn WorkflowRegistry>,
    scheduler: Arc<dyn WorkflowScheduler>,
    engine: Arc<dyn WorkflowEngine>,
    policy: Arc<dyn WorkflowPolicy>,
    lifecycle: Arc<dyn WorkflowLifecycle>,
    interceptors: Vec<Arc<dyn WorkflowInterceptor>>,
    observers: Vec<Arc<dyn WorkflowObserver>>,
    live: Mutex<HashMap<Uuid, WorkflowControl>>,
}

impl WorkflowManager {
    pub fn builder() -> WorkflowManagerBuilder {
        WorkflowManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn WorkflowStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn register(
        &self,
        definition: WorkflowDefinition,
    ) -> WorkflowResult<WorkflowDefinition> {
        definition.validate()?;
        self.policy.check(
            WorkflowOperation::Register,
            &definition,
            None,
            &definition.actor,
        )?;
        let current = self.store.find_workflow_by_key(&definition.key).await?;
        let (identity, expected) = match current {
            None => {
                if definition.version != 1 {
                    return Err(WorkflowError::Validation(
                        "first Workflow Definition must use version 1".into(),
                    ));
                }
                (WorkflowIdentity::from_definition(&definition), None)
            }
            Some(mut identity) => {
                if identity.id != definition.workflow_id
                    || definition.version != identity.current_definition_version.saturating_add(1)
                {
                    return Err(WorkflowError::Conflict(
                        "Workflow Definition does not extend the current identity version".into(),
                    ));
                }
                let expected = identity.version;
                identity.current_definition_id = definition.id;
                identity.current_definition_version = definition.version;
                identity.name = definition.name.clone();
                identity.version = identity.version.saturating_add(1);
                identity.actor = definition.actor.clone();
                identity.updated_at = Utc::now().max(identity.updated_at);
                (identity, Some(expected))
            }
        };
        self.store
            .save_registration(
                &WorkflowRegistrationCommit {
                    identity,
                    definition: definition.clone(),
                    expected_identity_version: expected,
                },
                &definition.actor,
            )
            .await?;
        self.registry.register(definition.clone())?;
        self.notify(
            WorkflowOperation::Register,
            WorkflowStage::Registry,
            true,
            Some(definition.workflow_id),
            Some(definition.id),
            None,
            None,
            &definition.actor,
            "Workflow Definition registered",
        );
        Ok(definition)
    }

    pub async fn bind_existing(
        &self,
        workflow_id: Uuid,
        version: u64,
    ) -> WorkflowResult<WorkflowDefinition> {
        let definition = self
            .store
            .find_definition(workflow_id, version)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(format!("{workflow_id}@{version}")))?;
        self.registry.register(definition.clone())?;
        Ok(definition)
    }

    pub async fn start(&self, request: StartWorkflowRequest) -> WorkflowResult<WorkflowInstance> {
        request.validate()?;
        let definition = self
            .load_definition(&request.workflow_key, request.definition_version)
            .await?;
        self.policy
            .check(WorkflowOperation::Start, &definition, None, &request.actor)?;
        let mut variables = request.variables;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.before_start(&definition, &mut variables)
            }))
            .map_err(|_| WorkflowError::Extension("workflow interceptor panicked".into()))??;
        }
        validate_variables(&variables)?;
        let mut instance = WorkflowInstance::new(definition, variables, request.actor)?;
        self.store
            .commit_instance(
                &WorkflowInstanceCommit::create(instance.clone(), initial_state_record(&instance)),
                &instance.actor,
            )
            .await?;
        let actor = instance.actor.clone();
        self.transition_and_commit(
            &mut instance,
            WorkflowState::Scheduled,
            &actor,
            "Workflow scheduled",
        )
        .await?;
        self.run_registered(instance, WorkflowControl::default())
            .await
    }

    pub async fn resume(&self, id: Uuid, actor: &str) -> WorkflowResult<WorkflowInstance> {
        validate_actor(actor)?;
        if self.live_control(id)?.is_some() {
            return Err(WorkflowError::Conflict(format!(
                "Workflow Instance {id} is already active"
            )));
        }
        let instance = self.required_instance(id).await?;
        if !matches!(
            instance.state,
            WorkflowState::Created
                | WorkflowState::Scheduled
                | WorkflowState::Paused
                | WorkflowState::Waiting
                | WorkflowState::Running
        ) {
            return Err(WorkflowError::InvalidState(format!(
                "cannot resume {} Workflow",
                instance.state.as_str()
            )));
        }
        self.policy.check(
            WorkflowOperation::Resume,
            &instance.definition,
            Some(&instance),
            actor,
        )?;
        let mut instance = instance;
        instance.actor = actor.into();
        if instance.state == WorkflowState::Created {
            self.transition_and_commit(
                &mut instance,
                WorkflowState::Scheduled,
                actor,
                "Created Workflow recovered and scheduled",
            )
            .await?;
        }
        self.run_registered(instance, WorkflowControl::default())
            .await
    }

    pub async fn pause(&self, id: Uuid, actor: &str) -> WorkflowResult<bool> {
        validate_actor(actor)?;
        if let Some(control) = self.live_control(id)? {
            let instance = self.required_instance(id).await?;
            self.policy.check(
                WorkflowOperation::Pause,
                &instance.definition,
                Some(&instance),
                actor,
            )?;
            control.request_pause_as(actor);
            return Ok(true);
        }
        let mut instance = self.required_instance(id).await?;
        self.policy.check(
            WorkflowOperation::Pause,
            &instance.definition,
            Some(&instance),
            actor,
        )?;
        match instance.state {
            WorkflowState::Paused => Ok(false),
            WorkflowState::Scheduled | WorkflowState::Waiting => {
                self.transition_and_commit(
                    &mut instance,
                    WorkflowState::Paused,
                    actor,
                    "Workflow paused at safe boundary",
                )
                .await?;
                Ok(true)
            }
            WorkflowState::Running => Err(WorkflowError::OutcomeUnknown(format!(
                "Workflow Instance {id} has no live owner for its running Action"
            ))),
            state => Err(WorkflowError::InvalidState(format!(
                "cannot pause {} Workflow",
                state.as_str()
            ))),
        }
    }

    pub async fn cancel(&self, id: Uuid, actor: &str) -> WorkflowResult<bool> {
        validate_actor(actor)?;
        if let Some(control) = self.live_control(id)? {
            let instance = self.required_instance(id).await?;
            self.policy.check(
                WorkflowOperation::Cancel,
                &instance.definition,
                Some(&instance),
                actor,
            )?;
            control.cancel_as(actor);
            return Ok(true);
        }
        let mut instance = self.required_instance(id).await?;
        if instance.state == WorkflowState::Cancelled {
            return Ok(false);
        }
        self.policy.check(
            WorkflowOperation::Cancel,
            &instance.definition,
            Some(&instance),
            actor,
        )?;
        if instance.state.is_terminal() {
            return Err(WorkflowError::InvalidState(format!(
                "cannot cancel {} Workflow",
                instance.state.as_str()
            )));
        }
        let active = instance
            .action_progress()
            .find(|value| {
                matches!(
                    value.state,
                    WorkItemState::Prepared | WorkItemState::Running | WorkItemState::Waiting
                )
            })
            .and_then(|value| value.binding.clone());
        if instance.state == WorkflowState::Running {
            let binding = active.ok_or_else(|| {
                WorkflowError::OutcomeUnknown(
                    "running Workflow has no recoverable external binding".into(),
                )
            })?;
            if !self.engine.cancel(&binding, actor).await? {
                return Err(WorkflowError::OutcomeUnknown(
                    "external Action terminal state must be reconciled before cancellation".into(),
                ));
            }
        } else if let Some(binding) = active {
            let _ = self.engine.cancel(&binding, actor).await?;
        }
        if let Some(progress) = instance.action_progress_mut_active() {
            progress.state = WorkItemState::Cancelled;
            progress.error = Some("Workflow cancelled by operator".into());
            progress.completed_at = Some(Utc::now());
            progress.updated_at = Utc::now();
            refresh_progress(&mut instance);
        }
        self.transition_and_commit(
            &mut instance,
            WorkflowState::Cancelled,
            actor,
            "Workflow cancelled",
        )
        .await?;
        Ok(true)
    }

    pub async fn archive(&self, id: Uuid, actor: &str) -> WorkflowResult<WorkflowInstance> {
        validate_actor(actor)?;
        let mut instance = self.required_instance(id).await?;
        self.policy.check(
            WorkflowOperation::Archive,
            &instance.definition,
            Some(&instance),
            actor,
        )?;
        self.transition_and_commit(
            &mut instance,
            WorkflowState::Archived,
            actor,
            "Workflow archived",
        )
        .await?;
        Ok(instance)
    }

    pub async fn snapshot(
        &self,
        id: Uuid,
        label: impl Into<String>,
        actor: &str,
    ) -> WorkflowResult<WorkflowSnapshot> {
        validate_actor(actor)?;
        let instance = self.required_instance(id).await?;
        if !matches!(
            instance.state,
            WorkflowState::Paused
                | WorkflowState::Waiting
                | WorkflowState::Completed
                | WorkflowState::Failed
                | WorkflowState::Cancelled
                | WorkflowState::Archived
        ) {
            return Err(WorkflowError::InvalidState(
                "Workflow Snapshot requires a safe boundary".into(),
            ));
        }
        self.policy.check(
            WorkflowOperation::Snapshot,
            &instance.definition,
            Some(&instance),
            actor,
        )?;
        let snapshot = WorkflowSnapshot::capture(&instance, label, actor)?;
        self.store.save_snapshot(&snapshot, actor).await?;
        Ok(snapshot)
    }

    pub async fn restore(
        &self,
        snapshot_id: Uuid,
        actor: &str,
    ) -> WorkflowResult<WorkflowInstance> {
        validate_actor(actor)?;
        let snapshot = self
            .store
            .find_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(snapshot_id.to_string()))?;
        let current = self.required_instance(snapshot.instance_id).await?;
        if current.version != snapshot.sequence
            || !matches!(
                snapshot.content.state,
                WorkflowState::Paused | WorkflowState::Waiting
            )
        {
            return Err(WorkflowError::Conflict(
                "Workflow Snapshot is not the current safe-boundary version".into(),
            ));
        }
        self.policy.check(
            WorkflowOperation::Restore,
            &current.definition,
            Some(&current),
            actor,
        )?;
        let mut restored = snapshot.content;
        restored.version = current.version.saturating_add(1);
        restored.actor = actor.into();
        restored.updated_at = Utc::now().max(current.updated_at);
        restored.validate()?;
        self.store
            .commit_instance(
                &WorkflowInstanceCommit::update(restored.clone(), current.version, None),
                actor,
            )
            .await?;
        Ok(restored)
    }

    pub async fn find_workflow(&self, id: Uuid) -> WorkflowResult<Option<WorkflowIdentity>> {
        self.store.find_workflow(id).await
    }

    pub async fn list_workflows(&self) -> WorkflowResult<Vec<WorkflowIdentity>> {
        self.store.list_workflows().await
    }

    pub async fn find_definition(
        &self,
        workflow_id: Uuid,
        version: u64,
    ) -> WorkflowResult<Option<WorkflowDefinition>> {
        self.store.find_definition(workflow_id, version).await
    }

    pub async fn list_definitions(
        &self,
        workflow_id: Uuid,
    ) -> WorkflowResult<Vec<WorkflowDefinition>> {
        self.store.list_definitions(workflow_id).await
    }

    pub async fn find_instance(&self, id: Uuid) -> WorkflowResult<Option<WorkflowInstance>> {
        self.store.find_instance(id).await
    }

    pub async fn list_instances(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowInstance>> {
        self.store.list_instances(workflow_id).await
    }

    pub async fn list_states(&self, id: Uuid) -> WorkflowResult<Vec<WorkflowStateRecord>> {
        self.store.list_states(id).await
    }

    pub async fn list_snapshots(&self, id: Uuid) -> WorkflowResult<Vec<WorkflowSnapshot>> {
        self.store.list_snapshots(id).await
    }

    async fn run_registered(
        &self,
        instance: WorkflowInstance,
        control: WorkflowControl,
    ) -> WorkflowResult<WorkflowInstance> {
        self.install_live_control(instance.id, control.clone())?;
        let id = instance.id;
        let result = self.run_loop(instance, &control).await;
        self.live
            .lock()
            .map_err(|_| WorkflowError::Internal("workflow live lock poisoned".into()))?
            .remove(&id);
        result
    }

    fn install_live_control(&self, id: Uuid, control: WorkflowControl) -> WorkflowResult<()> {
        let mut live = self
            .live
            .lock()
            .map_err(|_| WorkflowError::Internal("workflow live lock poisoned".into()))?;
        match live.entry(id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(control);
                Ok(())
            }
            std::collections::hash_map::Entry::Occupied(_) => Err(WorkflowError::Conflict(
                format!("Workflow Instance {id} is already active"),
            )),
        }
    }

    async fn run_loop(
        &self,
        mut instance: WorkflowInstance,
        control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowInstance> {
        let run_actor = instance.actor.clone();
        self.policy.check(
            WorkflowOperation::Run,
            &instance.definition,
            Some(&instance),
            &run_actor,
        )?;
        if matches!(
            instance.state,
            WorkflowState::Scheduled | WorkflowState::Paused | WorkflowState::Waiting
        ) {
            self.transition_and_commit(
                &mut instance,
                WorkflowState::Running,
                &run_actor,
                "Workflow running",
            )
            .await?;
        } else if instance.state != WorkflowState::Running {
            return Err(WorkflowError::InvalidState(format!(
                "cannot run {} Workflow",
                instance.state.as_str()
            )));
        }

        loop {
            if control.is_cancelled() {
                let actor = control
                    .cancellation_actor()
                    .unwrap_or_else(|| run_actor.clone());
                self.transition_and_commit(
                    &mut instance,
                    WorkflowState::Cancelled,
                    &actor,
                    "Workflow cancelled at safe boundary",
                )
                .await?;
                return Ok(instance);
            }
            if control.is_pause_requested() {
                let actor = control.pause_actor().unwrap_or_else(|| run_actor.clone());
                self.transition_and_commit(
                    &mut instance,
                    WorkflowState::Paused,
                    &actor,
                    "Workflow paused at safe boundary",
                )
                .await?;
                return Ok(instance);
            }
            let Some(cursor) = self.scheduler.next(&instance)? else {
                self.transition_and_commit(
                    &mut instance,
                    WorkflowState::Completed,
                    &run_actor,
                    "Workflow completed",
                )
                .await?;
                return Ok(instance);
            };
            let action = action_definition(&instance, cursor)?.clone();
            let state = instance.progress[cursor.stage_index].activities[cursor.activity_index]
                .actions[cursor.action_index]
                .state;
            if state == WorkItemState::Pending {
                let attempt = instance.progress[cursor.stage_index].activities
                    [cursor.activity_index]
                    .actions[cursor.action_index]
                    .attempts
                    .saturating_add(1);
                let context = action_context(&instance, cursor, attempt, &run_actor);
                let binding = match self.engine.prepare(&action, &context).await {
                    Ok(binding) if binding.dispatch_id == context.dispatch_id => binding,
                    Ok(_) => {
                        return Err(WorkflowError::Validation(
                            "WorkflowEngine returned a binding for another dispatch".into(),
                        ))
                    }
                    Err(error) => {
                        let progress = action_progress_mut(&mut instance, cursor)?;
                        progress.state = WorkItemState::Failed;
                        progress.attempts = attempt;
                        progress.error = Some(bounded_error(&error));
                        progress.completed_at = Some(Utc::now());
                        progress.updated_at = Utc::now();
                        refresh_progress(&mut instance);
                        self.transition_and_commit(
                            &mut instance,
                            WorkflowState::Failed,
                            &run_actor,
                            "Workflow Action preparation failed",
                        )
                        .await?;
                        return Ok(instance);
                    }
                };
                {
                    let progress = action_progress_mut(&mut instance, cursor)?;
                    progress.state = WorkItemState::Prepared;
                    progress.attempts = attempt;
                    progress.binding = Some(binding.clone());
                    progress.started_at.get_or_insert_with(Utc::now);
                    progress.error = None;
                    progress.updated_at = Utc::now();
                }
                refresh_progress(&mut instance);
                if let Err(error) = self.touch_and_commit(&mut instance, &run_actor).await {
                    let _ = self.engine.cancel(&binding, &run_actor).await;
                    return Err(error);
                }
            }
            let (binding, attempt) = {
                let progress = action_progress_mut(&mut instance, cursor)?;
                progress.state = WorkItemState::Running;
                progress.error = None;
                progress.updated_at = Utc::now();
                (
                    progress.binding.clone().ok_or_else(|| {
                        WorkflowError::OutcomeUnknown(
                            "active Workflow Action has no external binding".into(),
                        )
                    })?,
                    progress.attempts,
                )
            };
            refresh_progress(&mut instance);
            self.touch_and_commit(&mut instance, &run_actor).await?;
            let context = action_context(&instance, cursor, attempt, &run_actor);
            let timeout_ms = action
                .timeout_ms
                .unwrap_or(instance.definition.policy.action_timeout_ms);
            let outcome = match tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                self.engine.execute(&binding, &action, &context, control),
            )
            .await
            {
                Ok(Ok(value)) => value,
                Ok(Err(error @ WorkflowError::OutcomeUnknown(_))) => return Err(error),
                Ok(Err(error)) => WorkflowActionOutcome::Failed(bounded_error(&error)),
                Err(_) => {
                    match self.engine.cancel(&binding, &run_actor).await {
                        Ok(true) => WorkflowActionOutcome::Failed(format!(
                            "Workflow Action exceeded {timeout_ms}ms and cancellation was accepted"
                        )),
                        Ok(false) => {
                            return Err(WorkflowError::OutcomeUnknown(format!(
                                "Workflow Action exceeded {timeout_ms}ms but external state is already terminal"
                            )))
                        }
                        Err(error) => {
                            return Err(WorkflowError::OutcomeUnknown(format!(
                                "Workflow Action exceeded {timeout_ms}ms and cancellation could not be confirmed: {error}"
                            )))
                        }
                    }
                }
            };
            match outcome {
                WorkflowActionOutcome::Completed(result) => {
                    result.validate()?;
                    let progress = action_progress_mut(&mut instance, cursor)?;
                    progress.state = WorkItemState::Completed;
                    progress.result = Some(result);
                    progress.error = None;
                    progress.completed_at = Some(Utc::now());
                    progress.updated_at = Utc::now();
                    refresh_progress(&mut instance);
                    self.touch_and_commit(&mut instance, &run_actor).await?;
                }
                WorkflowActionOutcome::Waiting(reason) => {
                    set_stopped_action(&mut instance, cursor, WorkItemState::Waiting, &reason)?;
                    self.transition_and_commit(
                        &mut instance,
                        WorkflowState::Waiting,
                        &run_actor,
                        &reason,
                    )
                    .await?;
                    return Ok(instance);
                }
                WorkflowActionOutcome::Paused(reason) => {
                    set_stopped_action(&mut instance, cursor, WorkItemState::Waiting, &reason)?;
                    let actor = control.pause_actor().unwrap_or_else(|| run_actor.clone());
                    self.transition_and_commit(
                        &mut instance,
                        WorkflowState::Paused,
                        &actor,
                        &reason,
                    )
                    .await?;
                    return Ok(instance);
                }
                WorkflowActionOutcome::Failed(reason) => {
                    set_stopped_action(&mut instance, cursor, WorkItemState::Failed, &reason)?;
                    self.transition_and_commit(
                        &mut instance,
                        WorkflowState::Failed,
                        &run_actor,
                        &reason,
                    )
                    .await?;
                    return Ok(instance);
                }
                WorkflowActionOutcome::Cancelled(reason) => {
                    set_stopped_action(&mut instance, cursor, WorkItemState::Cancelled, &reason)?;
                    let actor = control
                        .cancellation_actor()
                        .unwrap_or_else(|| run_actor.clone());
                    self.transition_and_commit(
                        &mut instance,
                        WorkflowState::Cancelled,
                        &actor,
                        &reason,
                    )
                    .await?;
                    return Ok(instance);
                }
            }
        }
    }

    async fn load_definition(
        &self,
        key: &str,
        requested_version: Option<u64>,
    ) -> WorkflowResult<WorkflowDefinition> {
        let identity = self
            .store
            .find_workflow_by_key(key)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(key.into()))?;
        if !identity.enabled {
            return Err(WorkflowError::InvalidState(
                "disabled Workflow cannot start".into(),
            ));
        }
        let version = requested_version.unwrap_or(identity.current_definition_version);
        if let Some(value) = self.registry.find(identity.id, version)? {
            return Ok(value);
        }
        let value = self
            .store
            .find_definition(identity.id, version)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(format!("{}@{version}", identity.key)))?;
        self.registry.register(value.clone())?;
        Ok(value)
    }

    async fn required_instance(&self, id: Uuid) -> WorkflowResult<WorkflowInstance> {
        self.store
            .find_instance(id)
            .await?
            .ok_or_else(|| WorkflowError::NotFound(id.to_string()))
    }

    fn live_control(&self, id: Uuid) -> WorkflowResult<Option<WorkflowControl>> {
        Ok(self
            .live
            .lock()
            .map_err(|_| WorkflowError::Internal("workflow live lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    async fn transition_and_commit(
        &self,
        instance: &mut WorkflowInstance,
        next: WorkflowState,
        actor: &str,
        reason: &str,
    ) -> WorkflowResult<()> {
        let expected = instance.version;
        let from = instance.state;
        self.lifecycle.transition(instance, next, actor)?;
        let record = WorkflowStateRecord {
            id: Uuid::new_v4(),
            instance_id: instance.id,
            sequence: instance.version,
            from_state: Some(from),
            to_state: next,
            reason: bounded_reason(reason),
            actor: actor.into(),
            created_at: instance.updated_at,
        };
        self.store
            .commit_instance(
                &WorkflowInstanceCommit::update(instance.clone(), expected, Some(record)),
                actor,
            )
            .await?;
        self.notify(
            WorkflowOperation::Run,
            WorkflowStage::Lifecycle,
            true,
            Some(instance.workflow_id),
            Some(instance.definition_id),
            Some(instance.id),
            instance.current_ids().2,
            actor,
            reason,
        );
        Ok(())
    }

    async fn touch_and_commit(
        &self,
        instance: &mut WorkflowInstance,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        let expected = instance.version;
        instance.version = instance.version.saturating_add(1);
        instance.actor = actor.into();
        instance.updated_at = Utc::now().max(instance.updated_at);
        instance.validate()?;
        self.store
            .commit_instance(
                &WorkflowInstanceCommit::update(instance.clone(), expected, None),
                actor,
            )
            .await
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        operation: WorkflowOperation,
        stage: WorkflowStage,
        success: bool,
        workflow_id: Option<Uuid>,
        definition_id: Option<Uuid>,
        instance_id: Option<Uuid>,
        action_id: Option<Uuid>,
        actor: &str,
        reason: &str,
    ) {
        let value = WorkflowObservation {
            operation,
            stage,
            success,
            workflow_id,
            definition_id,
            instance_id,
            action_id,
            actor: actor.into(),
            reason: bounded_reason(reason),
            occurred_at: Utc::now(),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&value)));
        }
    }
}

fn action_context(
    instance: &WorkflowInstance,
    cursor: crate::infrastructure::WorkflowCursor,
    attempt: u32,
    actor: &str,
) -> WorkflowActionContext {
    let progress = &instance.progress[cursor.stage_index].activities[cursor.activity_index].actions
        [cursor.action_index];
    WorkflowActionContext {
        instance_id: instance.id,
        workflow_id: instance.workflow_id,
        definition_id: instance.definition_id,
        definition_version: instance.definition_version,
        stage_id: cursor.stage_id,
        activity_id: cursor.activity_id,
        action_id: cursor.action_id,
        dispatch_id: progress.dispatch_id,
        attempt,
        variables: instance.variables.clone(),
        actor: actor.into(),
    }
}

fn set_stopped_action(
    instance: &mut WorkflowInstance,
    cursor: crate::infrastructure::WorkflowCursor,
    state: WorkItemState,
    reason: &str,
) -> WorkflowResult<()> {
    let progress = action_progress_mut(instance, cursor)?;
    progress.state = state;
    progress.result = None;
    progress.error = Some(bounded_reason(reason));
    if matches!(state, WorkItemState::Failed | WorkItemState::Cancelled) {
        progress.completed_at = Some(Utc::now());
    }
    progress.updated_at = Utc::now();
    refresh_progress(instance);
    Ok(())
}

fn bounded_error(error: &WorkflowError) -> String {
    bounded_reason(&error.to_string())
}

fn bounded_reason(value: &str) -> String {
    let value = value.trim();
    let mut result = value.chars().take(1024).collect::<String>();
    if result.is_empty() {
        result = "Workflow operation failed without a reason".into();
    }
    result
}

trait ActiveProgress {
    fn action_progress_mut_active(&mut self) -> Option<&mut crate::domain::ActionProgress>;
}

impl ActiveProgress for WorkflowInstance {
    fn action_progress_mut_active(&mut self) -> Option<&mut crate::domain::ActionProgress> {
        self.progress
            .iter_mut()
            .flat_map(|stage| &mut stage.activities)
            .flat_map(|activity| &mut activity.actions)
            .find(|value| {
                matches!(
                    value.state,
                    WorkItemState::Prepared | WorkItemState::Running | WorkItemState::Waiting
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_live_registration_never_overwrites_original_control() {
        let manager = WorkflowManager::builder().build();
        let id = Uuid::new_v4();
        let original = WorkflowControl::default();
        let replacement = WorkflowControl::default();

        manager.install_live_control(id, original.clone()).unwrap();
        assert!(matches!(
            manager.install_live_control(id, replacement.clone()),
            Err(WorkflowError::Conflict(_))
        ));

        replacement.request_pause_as("racer");
        assert!(!original.is_pause_requested());
        manager
            .live_control(id)
            .unwrap()
            .unwrap()
            .request_pause_as("operator");
        assert!(original.is_pause_requested());
    }
}
