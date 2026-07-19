use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use chrono::Utc;
use core_agent_plan::{Plan, Step};
use uuid::Uuid;

use crate::defaults::{
    AllowAllExecutionPolicy, CheckpointManager, DefaultActionExecutor, DefaultExecutionLifecycle,
    ExplicitRollbackPolicy, ExponentialRetryPolicy, InMemoryExecutionStore, RetryManager,
    RollbackManager, SequentialDispatcher,
};
use crate::domain::{
    ActionExecutionStatus, CommandFailure, ExecuteRequest, Execution, ExecutionCheckpoint,
    ExecutionCommand, ExecutionStateRecord, ExecutionStatus, RetryRecord, RetryStatus,
    RollbackRecord, RollbackStatus,
};
use crate::error::{ExecutionError, ExecutionResult};
use crate::infrastructure::{
    ActionExecutor, Dispatcher, ExecutionCommit, ExecutionControl, ExecutionInterceptor,
    ExecutionLifecycle, ExecutionObservation, ExecutionObserver, ExecutionOperation,
    ExecutionPolicy, ExecutionStage, ExecutionStore, RetryPolicy, RollbackPolicy,
};

pub struct ExecutionEngine {
    dispatcher: Arc<dyn Dispatcher>,
    executor: Arc<dyn ActionExecutor>,
    retry_policy: Arc<dyn RetryPolicy>,
    rollback_policy: Arc<dyn RollbackPolicy>,
    lifecycle: Arc<dyn ExecutionLifecycle>,
    policy: Arc<dyn ExecutionPolicy>,
    interceptors: Vec<Arc<dyn ExecutionInterceptor>>,
    observers: Vec<Arc<dyn ExecutionObserver>>,
}

pub struct ExecutionManagerBuilder {
    store: Arc<dyn ExecutionStore>,
    dispatcher: Arc<dyn Dispatcher>,
    executor: Arc<dyn ActionExecutor>,
    retry_policy: Arc<dyn RetryPolicy>,
    rollback_policy: Arc<dyn RollbackPolicy>,
    lifecycle: Arc<dyn ExecutionLifecycle>,
    policy: Arc<dyn ExecutionPolicy>,
    interceptors: Vec<Arc<dyn ExecutionInterceptor>>,
    observers: Vec<Arc<dyn ExecutionObserver>>,
}

impl Default for ExecutionManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryExecutionStore::default()),
            dispatcher: Arc::new(SequentialDispatcher),
            executor: Arc::new(DefaultActionExecutor),
            retry_policy: Arc::new(ExponentialRetryPolicy::default()),
            rollback_policy: Arc::new(ExplicitRollbackPolicy),
            lifecycle: Arc::new(DefaultExecutionLifecycle::default()),
            policy: Arc::new(AllowAllExecutionPolicy),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl ExecutionManagerBuilder {
    pub fn store(mut self, value: Arc<dyn ExecutionStore>) -> Self {
        self.store = value;
        self
    }

    pub fn dispatcher(mut self, value: Arc<dyn Dispatcher>) -> Self {
        self.dispatcher = value;
        self
    }

    pub fn executor(mut self, value: Arc<dyn ActionExecutor>) -> Self {
        self.executor = value;
        self
    }

    pub fn retry_policy(mut self, value: Arc<dyn RetryPolicy>) -> Self {
        self.retry_policy = value;
        self
    }

    pub fn rollback_policy(mut self, value: Arc<dyn RollbackPolicy>) -> Self {
        self.rollback_policy = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn ExecutionLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn ExecutionPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn ExecutionInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn ExecutionObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> ExecutionManager {
        ExecutionManager {
            store: self.store,
            engine: Arc::new(ExecutionEngine {
                dispatcher: self.dispatcher,
                executor: self.executor,
                retry_policy: self.retry_policy,
                rollback_policy: self.rollback_policy,
                lifecycle: self.lifecycle,
                policy: self.policy,
                interceptors: self.interceptors,
                observers: self.observers,
            }),
            live: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

pub struct ExecutionManager {
    store: Arc<dyn ExecutionStore>,
    engine: Arc<ExecutionEngine>,
    live: Arc<RwLock<HashMap<Uuid, Arc<LiveExecutionEntry>>>>,
}

struct LiveExecutionEntry {
    control: ExecutionControl,
    accepting_pause: AtomicBool,
}

impl ExecutionManager {
    pub fn builder() -> ExecutionManagerBuilder {
        ExecutionManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn ExecutionStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn execute(&self, plan: Plan, request: ExecuteRequest) -> ExecutionResult<Execution> {
        let execution = self.prepare(plan, request).await?;
        self.start(execution.id).await
    }

    /// Persists an approved Plan as a controllable READY Execution without
    /// dispatching commands. Higher runtimes can now publish its identity
    /// before any side effect starts.
    pub async fn prepare(&self, plan: Plan, request: ExecuteRequest) -> ExecutionResult<Execution> {
        let mut execution = Execution::new(plan, request)?;
        self.engine.policy.evaluate(
            ExecutionOperation::Prepare,
            &execution.plan,
            &execution,
            None,
        )?;
        let initial_state = ExecutionStateRecord {
            id: Uuid::new_v4(),
            execution_id: execution.id,
            sequence: execution.version,
            from_status: None,
            to_status: ExecutionStatus::Pending,
            reason: "execution created".into(),
            created_at: execution.created_at,
        };
        self.store
            .commit(
                &ExecutionCommit::create(execution.clone(), initial_state),
                &execution.actor,
            )
            .await?;
        self.transition(
            &mut execution,
            ExecutionStatus::Ready,
            "approved plan accepted",
            true,
            None,
            None,
        )
        .await?;
        Ok(execution)
    }

    /// Starts a previously prepared READY Execution.
    pub async fn start(&self, id: Uuid) -> ExecutionResult<Execution> {
        self.start_with_control(id, ExecutionControl::default())
            .await
    }

    /// Starts a prepared Execution with a caller-owned cooperative control.
    /// A pause/cancel requested before registration is preserved.
    pub async fn start_with_control(
        &self,
        id: Uuid,
        control: ExecutionControl,
    ) -> ExecutionResult<Execution> {
        if self.live_control(id)?.is_some() {
            return Err(ExecutionError::Conflict(format!(
                "execution {id} is already active"
            )));
        }
        let mut execution = self.required(id).await?;
        if execution.status != ExecutionStatus::Ready {
            return Err(ExecutionError::InvalidState(format!(
                "cannot start {} execution",
                execution.status.as_str()
            )));
        }
        self.engine.policy.evaluate(
            ExecutionOperation::Start,
            &execution.plan,
            &execution,
            None,
        )?;
        self.transition(
            &mut execution,
            ExecutionStatus::Running,
            "execution started",
            true,
            None,
            None,
        )
        .await?;
        self.run_registered_with_control(execution, control).await
    }

    pub async fn pause(&self, id: Uuid) -> ExecutionResult<bool> {
        if let Some(live) = self.live_entry(id)? {
            let execution = self.required(id).await?;
            self.engine.policy.evaluate(
                ExecutionOperation::Pause,
                &execution.plan,
                &execution,
                None,
            )?;
            self.accept_live_pause(id, &live)?;
            return Ok(true);
        }
        let mut execution = self.required(id).await?;
        if execution.status == ExecutionStatus::Paused {
            return Ok(false);
        }
        if execution.status.is_terminal() {
            return Err(ExecutionError::InvalidState(format!(
                "cannot pause {} execution",
                execution.status.as_str()
            )));
        }
        if execution.has_uncertain_action() {
            return Err(ExecutionError::OutcomeUnknown(format!(
                "execution {id} has an in-flight command without a live owner"
            )));
        }
        self.engine.policy.evaluate(
            ExecutionOperation::Pause,
            &execution.plan,
            &execution,
            None,
        )?;
        self.transition(
            &mut execution,
            ExecutionStatus::Paused,
            "pause requested at recovered safe boundary",
            true,
            None,
            None,
        )
        .await?;
        Ok(true)
    }

    pub async fn resume(&self, id: Uuid, actor: impl Into<String>) -> ExecutionResult<Execution> {
        self.resume_with_control(id, actor, ExecutionControl::default())
            .await
    }

    /// Resumes with a caller-owned cooperative control so a pause requested
    /// before the lower runtime is registered cannot be lost.
    pub async fn resume_with_control(
        &self,
        id: Uuid,
        actor: impl Into<String>,
        control: ExecutionControl,
    ) -> ExecutionResult<Execution> {
        if self.live_control(id)?.is_some() {
            return Err(ExecutionError::Conflict(format!(
                "execution {id} is already active"
            )));
        }
        let mut execution = self.required(id).await?;
        if execution.has_uncertain_action() {
            return Err(ExecutionError::OutcomeUnknown(format!(
                "execution {id} cannot replay an outcome-unknown command"
            )));
        }
        let actor = actor.into();
        if actor.trim().is_empty() || actor.len() > 256 {
            return Err(ExecutionError::Validation("resume actor is invalid".into()));
        }
        execution.actor = actor;
        match execution.status {
            ExecutionStatus::Pending => {
                self.transition(
                    &mut execution,
                    ExecutionStatus::Ready,
                    "pending execution recovered",
                    true,
                    None,
                    None,
                )
                .await?;
            }
            ExecutionStatus::Retrying => {
                for progress in execution.steps.values_mut() {
                    if progress.status == ActionExecutionStatus::Retrying {
                        progress.status = ActionExecutionStatus::Pending;
                    }
                }
                execution.current_task_id = None;
                execution.current_step_id = None;
                self.transition(
                    &mut execution,
                    ExecutionStatus::Paused,
                    "retry boundary recovered after process restart",
                    true,
                    None,
                    None,
                )
                .await?;
            }
            ExecutionStatus::Running => {
                self.transition(
                    &mut execution,
                    ExecutionStatus::Paused,
                    "safe command boundary recovered after process restart",
                    true,
                    None,
                    None,
                )
                .await?;
            }
            ExecutionStatus::Ready | ExecutionStatus::Paused => {}
            status => {
                return Err(ExecutionError::InvalidState(format!(
                    "cannot resume {} execution",
                    status.as_str()
                )))
            }
        }
        self.engine.policy.evaluate(
            ExecutionOperation::Resume,
            &execution.plan,
            &execution,
            None,
        )?;
        if execution.started_at.is_none() {
            self.engine.policy.evaluate(
                ExecutionOperation::Start,
                &execution.plan,
                &execution,
                None,
            )?;
        }
        self.transition(
            &mut execution,
            ExecutionStatus::Running,
            "execution resumed",
            true,
            None,
            None,
        )
        .await?;
        self.run_registered_with_control(execution, control).await
    }

    pub async fn cancel(&self, id: Uuid, actor: impl Into<String>) -> ExecutionResult<bool> {
        let actor = actor.into();
        if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
            return Err(ExecutionError::Validation("cancel actor is invalid".into()));
        }
        if let Some(control) = self.live_control(id)? {
            let execution = self.required(id).await?;
            self.engine.policy.evaluate(
                ExecutionOperation::Cancel,
                &execution.plan,
                &execution,
                None,
            )?;
            control.cancel_as(actor);
            return Ok(true);
        }
        let mut execution = self.required(id).await?;
        if execution.status == ExecutionStatus::Cancelled {
            return Ok(false);
        }
        if execution.status.is_terminal() {
            return Err(ExecutionError::InvalidState(format!(
                "cannot cancel {} execution",
                execution.status.as_str()
            )));
        }
        if execution.has_uncertain_action() {
            return Err(ExecutionError::OutcomeUnknown(format!(
                "execution {id} has an in-flight command without a live owner"
            )));
        }
        execution.actor = actor;
        self.engine.policy.evaluate(
            ExecutionOperation::Cancel,
            &execution.plan,
            &execution,
            None,
        )?;
        self.transition(
            &mut execution,
            ExecutionStatus::Cancelled,
            "execution cancelled",
            true,
            None,
            None,
        )
        .await?;
        Ok(true)
    }

    pub async fn find(&self, id: Uuid) -> ExecutionResult<Option<Execution>> {
        self.store.find_execution(id).await
    }

    pub async fn list(&self, plan_id: Uuid) -> ExecutionResult<Vec<Execution>> {
        self.store.list_executions(plan_id).await
    }

    pub async fn list_checkpoints(&self, id: Uuid) -> ExecutionResult<Vec<ExecutionCheckpoint>> {
        self.store.list_checkpoints(id).await
    }

    pub async fn restore_checkpoint(
        &self,
        checkpoint_id: Uuid,
        actor: impl Into<String>,
    ) -> ExecutionResult<Execution> {
        let checkpoint = self
            .store
            .find_checkpoint(checkpoint_id)
            .await?
            .ok_or_else(|| ExecutionError::NotFound(checkpoint_id.to_string()))?;
        if self.live_control(checkpoint.execution_id)?.is_some() {
            return Err(ExecutionError::Conflict(format!(
                "execution {} is active",
                checkpoint.execution_id
            )));
        }
        let current = self.required(checkpoint.execution_id).await?;
        if current.version != checkpoint.sequence {
            return Err(ExecutionError::Conflict(
                "only the latest checkpoint can be restored without replaying side effects".into(),
            ));
        }
        let mut execution = CheckpointManager::restore(&checkpoint)?;
        let actor = actor.into();
        if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
            return Err(ExecutionError::Validation(
                "restore actor is invalid".into(),
            ));
        }
        execution.actor = actor;
        self.engine.policy.evaluate(
            ExecutionOperation::Restore,
            &execution.plan,
            &execution,
            None,
        )?;
        match execution.status {
            ExecutionStatus::Running => {
                self.transition(
                    &mut execution,
                    ExecutionStatus::Paused,
                    "safe command-boundary checkpoint restored",
                    true,
                    None,
                    None,
                )
                .await?;
            }
            ExecutionStatus::Retrying => {
                for progress in execution.steps.values_mut() {
                    if progress.status == ActionExecutionStatus::Retrying {
                        progress.status = ActionExecutionStatus::Pending;
                    }
                }
                execution.current_task_id = None;
                execution.current_step_id = None;
                self.transition(
                    &mut execution,
                    ExecutionStatus::Paused,
                    "retry-boundary checkpoint restored",
                    true,
                    None,
                    None,
                )
                .await?;
            }
            ExecutionStatus::Pending | ExecutionStatus::Ready | ExecutionStatus::Paused => {
                self.progress(&mut execution, "checkpoint restored", true, None, None)
                    .await?;
            }
            _ => {
                return Err(ExecutionError::InvalidState(
                    "checkpoint is not at a recoverable boundary".into(),
                ))
            }
        }
        Ok(execution)
    }

    pub async fn list_states(&self, id: Uuid) -> ExecutionResult<Vec<ExecutionStateRecord>> {
        self.store.list_states(id).await
    }

    pub async fn list_retries(&self, id: Uuid) -> ExecutionResult<Vec<RetryRecord>> {
        self.store.list_retries(id).await
    }

    pub async fn list_rollbacks(&self, id: Uuid) -> ExecutionResult<Vec<RollbackRecord>> {
        self.store.list_rollbacks(id).await
    }

    async fn run_registered_with_control(
        &self,
        execution: Execution,
        control: ExecutionControl,
    ) -> ExecutionResult<Execution> {
        let execution_id = execution.id;
        let live_entry = Arc::new(LiveExecutionEntry {
            control: control.clone(),
            accepting_pause: AtomicBool::new(true),
        });
        {
            let mut live = self
                .live
                .write()
                .map_err(|_| ExecutionError::Internal("live execution lock poisoned".into()))?;
            match live.entry(execution.id) {
                std::collections::hash_map::Entry::Occupied(_) => {
                    return Err(ExecutionError::Conflict(format!(
                        "execution {} is already active",
                        execution.id
                    )))
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(Arc::clone(&live_entry));
                }
            }
        }
        let _guard = LiveExecutionGuard {
            execution_id,
            live: Arc::clone(&self.live),
        };
        self.run(execution, control).await
    }

    async fn run(
        &self,
        mut execution: Execution,
        control: ExecutionControl,
    ) -> ExecutionResult<Execution> {
        loop {
            if control.is_cancelled() {
                apply_cancellation_actor(&mut execution, &control);
                self.transition(
                    &mut execution,
                    ExecutionStatus::Cancelled,
                    "cancelled at command boundary",
                    true,
                    None,
                    None,
                )
                .await?;
                return Ok(execution);
            }
            if control.is_pause_requested() {
                self.close_pause_acceptance(execution.id)?;
                execution.current_task_id = None;
                execution.current_step_id = None;
                self.transition(
                    &mut execution,
                    ExecutionStatus::Paused,
                    "paused at command boundary",
                    true,
                    None,
                    None,
                )
                .await?;
                return Ok(execution);
            }

            let Some(item) = self.engine.dispatcher.next(&execution.plan, &execution)? else {
                if execution
                    .steps
                    .values()
                    .all(|progress| progress.status == ActionExecutionStatus::Completed)
                {
                    if !self.close_pause_acceptance(execution.id)? {
                        continue;
                    }
                    self.transition(
                        &mut execution,
                        ExecutionStatus::Completed,
                        "all commands completed",
                        true,
                        None,
                        None,
                    )
                    .await?;
                    return Ok(execution);
                }
                if !self.close_pause_acceptance(execution.id)? {
                    continue;
                }
                self.transition(
                    &mut execution,
                    ExecutionStatus::Failed,
                    "no dependency-ready command remains",
                    true,
                    None,
                    None,
                )
                .await?;
                return Ok(execution);
            };
            let step = execution
                .plan
                .tasks
                .get(&item.task_id)
                .and_then(|task| task.steps.get(&item.step_id))
                .cloned()
                .ok_or_else(|| {
                    ExecutionError::Validation("dispatcher returned unknown step".into())
                })?;
            self.run_step(&mut execution, &step, item.task_id, &control)
                .await?;
            if execution.status.is_terminal() || execution.status == ExecutionStatus::Paused {
                return Ok(execution);
            }
        }
    }

    async fn run_step(
        &self,
        execution: &mut Execution,
        step: &Step,
        task_id: Uuid,
        control: &ExecutionControl,
    ) -> ExecutionResult<()> {
        loop {
            if control.is_cancelled() {
                apply_cancellation_actor(execution, control);
                self.transition(
                    execution,
                    ExecutionStatus::Cancelled,
                    "cancelled before command dispatch",
                    true,
                    None,
                    None,
                )
                .await?;
                return Ok(());
            }
            let attempt = execution
                .steps
                .get(&step.id)
                .map(|value| value.attempts + 1)
                .ok_or_else(|| ExecutionError::Validation("step progress missing".into()))?;
            let mut command = ExecutionCommand::from_action(
                execution.id,
                task_id,
                step.id,
                &step.action,
                attempt,
            )?;
            let identity = (
                command.id,
                command.execution_id,
                command.task_id,
                command.step_id,
                command.action_id,
                command.kind,
                command.action_kind,
                command.tool_key.clone(),
                command.capability.clone(),
                command.target_uri.clone(),
            );
            let preflight = (|| -> ExecutionResult<()> {
                for interceptor in &self.engine.interceptors {
                    interceptor.before_command(execution, &mut command)?;
                }
                command.validate()?;
                if identity
                    != (
                        command.id,
                        command.execution_id,
                        command.task_id,
                        command.step_id,
                        command.action_id,
                        command.kind,
                        command.action_kind,
                        command.tool_key.clone(),
                        command.capability.clone(),
                        command.target_uri.clone(),
                    )
                {
                    return Err(ExecutionError::Validation(
                        "execution interceptor changed command identity".into(),
                    ));
                }
                self.engine.policy.evaluate(
                    ExecutionOperation::Dispatch,
                    &execution.plan,
                    execution,
                    Some(&command),
                )
            })();
            if let Err(error) = preflight {
                self.fail_step(
                    execution,
                    step,
                    &command,
                    CommandFailure::new("COMMAND_PREFLIGHT", error.to_string(), false).bounded(),
                    control,
                )
                .await?;
                return Ok(());
            }
            execution.current_task_id = Some(task_id);
            execution.current_step_id = Some(step.id);
            {
                let progress = required_progress_mut(execution, step.id)?;
                progress.status = ActionExecutionStatus::Running;
                progress.attempts = attempt;
                progress.command_id = Some(command.id);
                progress.error = None;
                progress.started_at.get_or_insert_with(Utc::now);
            }
            self.progress(
                execution,
                "command dispatch intent persisted",
                true,
                None,
                None,
            )
            .await?;
            self.observe(
                execution,
                ExecutionStage::Command,
                Some(step.id),
                Some(command.id),
                true,
                None,
            );

            let result = self.engine.executor.execute(&command, control).await;
            let result = match result {
                Ok(mut result) => {
                    let original = result.clone();
                    for interceptor in &self.engine.interceptors {
                        if let Err(error) =
                            interceptor.after_command(execution, &command, &mut result)
                        {
                            result = original;
                            self.observe(
                                execution,
                                ExecutionStage::Command,
                                Some(step.id),
                                Some(command.id),
                                false,
                                Some(format!("after_command extension failed: {error}")),
                            );
                            break;
                        }
                    }
                    Ok(result.bounded())
                }
                Err(error) => Err(error),
            }
            .map_err(CommandFailure::bounded);

            match result {
                Ok(result) => {
                    let progress = required_progress_mut(execution, step.id)?;
                    progress.status = ActionExecutionStatus::Completed;
                    progress.result = Some(result);
                    progress.error = None;
                    progress.completed_at = Some(Utc::now());
                    execution.completed_order.push(step.id);
                    execution.current_task_id = None;
                    execution.current_step_id = None;
                    self.progress(execution, "command completed", true, None, None)
                        .await?;
                    self.observe(
                        execution,
                        ExecutionStage::Command,
                        Some(step.id),
                        Some(command.id),
                        true,
                        Some("command completed".into()),
                    );
                    return Ok(());
                }
                Err(failure) if failure.cancelled || control.is_cancelled() => {
                    apply_cancellation_actor(execution, control);
                    let progress = required_progress_mut(execution, step.id)?;
                    progress.status = ActionExecutionStatus::Cancelled;
                    progress.error = Some(failure);
                    progress.completed_at = Some(Utc::now());
                    execution.current_task_id = None;
                    execution.current_step_id = None;
                    self.transition(
                        execution,
                        ExecutionStatus::Cancelled,
                        "in-flight command cancelled",
                        true,
                        None,
                        None,
                    )
                    .await?;
                    self.observe(
                        execution,
                        ExecutionStage::Command,
                        Some(step.id),
                        Some(command.id),
                        false,
                        Some("command cancelled".into()),
                    );
                    return Ok(());
                }
                Err(mut failure) => {
                    let mut delay = RetryManager::delay(
                        self.engine.retry_policy.as_ref(),
                        &failure,
                        attempt,
                        step.max_attempts,
                    );
                    if delay.is_some() {
                        if let Err(error) = self.engine.policy.evaluate(
                            ExecutionOperation::Retry,
                            &execution.plan,
                            execution,
                            Some(&command),
                        ) {
                            failure = CommandFailure::new(
                                "RETRY_POLICY_DENIED",
                                error.to_string(),
                                false,
                            )
                            .bounded();
                            delay = None;
                        }
                    }
                    if let Some(delay) = delay {
                        let retry = RetryRecord {
                            id: Uuid::new_v4(),
                            execution_id: execution.id,
                            step_id: step.id,
                            action_id: step.action.id,
                            attempt,
                            delay_ms: delay.as_millis().min(u64::MAX as u128) as u64,
                            error_kind: bounded(&failure.kind, 64),
                            error_message: bounded(&failure.message, 1024),
                            status: RetryStatus::Scheduled,
                            created_at: Utc::now(),
                        };
                        let progress = required_progress_mut(execution, step.id)?;
                        progress.status = ActionExecutionStatus::Retrying;
                        progress.error = Some(failure);
                        self.transition(
                            execution,
                            ExecutionStatus::Retrying,
                            "retry scheduled",
                            true,
                            Some(retry),
                            None,
                        )
                        .await?;
                        tokio::select! {
                            _ = control.cancelled() => {
                                apply_cancellation_actor(execution, control);
                                let progress = required_progress_mut(execution, step.id)?;
                                progress.status = ActionExecutionStatus::Cancelled;
                                progress.error = Some(CommandFailure::cancelled(
                                    "cancelled during retry delay",
                                ));
                                progress.completed_at = Some(Utc::now());
                                execution.current_task_id = None;
                                execution.current_step_id = None;
                                self.transition(
                                    execution,
                                    ExecutionStatus::Cancelled,
                                    "cancelled during retry delay",
                                    true,
                                    None,
                                    None,
                                ).await?;
                                return Ok(());
                            }
                            _ = tokio::time::sleep(delay) => {}
                        }
                        if control.is_pause_requested() {
                            let progress = required_progress_mut(execution, step.id)?;
                            progress.status = ActionExecutionStatus::Pending;
                            execution.current_task_id = None;
                            execution.current_step_id = None;
                            self.transition(
                                execution,
                                ExecutionStatus::Paused,
                                "paused during retry delay",
                                true,
                                None,
                                None,
                            )
                            .await?;
                            return Ok(());
                        }
                        let retry = RetryRecord {
                            id: Uuid::new_v4(),
                            execution_id: execution.id,
                            step_id: step.id,
                            action_id: step.action.id,
                            attempt,
                            delay_ms: 0,
                            error_kind: "RETRY_RESUMED".into(),
                            error_message: "retry delay elapsed".into(),
                            status: RetryStatus::Resumed,
                            created_at: Utc::now(),
                        };
                        self.transition(
                            execution,
                            ExecutionStatus::Running,
                            "retry resumed",
                            false,
                            Some(retry),
                            None,
                        )
                        .await?;
                        continue;
                    }

                    self.fail_step(execution, step, &command, failure, control)
                        .await?;
                    return Ok(());
                }
            }
        }
    }

    async fn fail_step(
        &self,
        execution: &mut Execution,
        step: &Step,
        command: &ExecutionCommand,
        failure: CommandFailure,
        control: &ExecutionControl,
    ) -> ExecutionResult<()> {
        execution.current_task_id = Some(command.task_id);
        execution.current_step_id = Some(step.id);
        {
            let progress = required_progress_mut(execution, step.id)?;
            progress.status = ActionExecutionStatus::Failed;
            progress.attempts = command.attempt;
            progress.command_id = Some(command.id);
            progress.error = Some(failure.clone());
            progress.completed_at = Some(Utc::now());
        }
        self.progress(execution, "command failed permanently", true, None, None)
            .await?;
        self.observe(
            execution,
            ExecutionStage::Command,
            Some(step.id),
            Some(command.id),
            false,
            Some(failure.kind.clone()),
        );
        if self
            .engine
            .rollback_policy
            .should_rollback(execution, &failure)
        {
            self.rollback(execution, control).await?;
        }
        execution.current_task_id = None;
        execution.current_step_id = None;
        self.transition(
            execution,
            ExecutionStatus::Failed,
            "command attempts exhausted",
            true,
            None,
            None,
        )
        .await
    }

    async fn rollback(
        &self,
        execution: &mut Execution,
        control: &ExecutionControl,
    ) -> ExecutionResult<()> {
        self.engine.policy.evaluate(
            ExecutionOperation::Rollback,
            &execution.plan,
            execution,
            None,
        )?;
        let step_ids = RollbackManager::planned_steps(execution).collect::<Vec<_>>();
        for step_id in step_ids {
            let (task_id, step) = find_step(&execution.plan, step_id)?;
            let attempt = execution
                .steps
                .get(&step_id)
                .map(|progress| progress.attempts.max(1))
                .unwrap_or(1);
            let command = ExecutionCommand::from_action(
                execution.id,
                task_id,
                step.id,
                &step.action,
                attempt,
            )?;
            let (status, failure) = if self.engine.executor.supports_rollback(&command) {
                match self.engine.executor.rollback(&command, control).await {
                    Ok(()) => (RollbackStatus::Success, None),
                    Err(error) => (RollbackStatus::Failed, Some(error)),
                }
            } else {
                (RollbackStatus::Skipped, None)
            };
            let record = RollbackRecord {
                id: Uuid::new_v4(),
                execution_id: execution.id,
                step_id,
                action_id: step.action.id,
                command_id: command.id,
                status,
                error_kind: failure.as_ref().map(|value| bounded(&value.kind, 64)),
                error_message: failure.as_ref().map(|value| bounded(&value.message, 1024)),
                created_at: Utc::now(),
            };
            if status != RollbackStatus::Skipped {
                let progress = required_progress_mut(execution, step_id)?;
                progress.status = if status == RollbackStatus::Success {
                    ActionExecutionStatus::RolledBack
                } else {
                    ActionExecutionStatus::RollbackFailed
                };
            }
            self.progress(
                execution,
                "rollback outcome recorded",
                true,
                None,
                Some(record),
            )
            .await?;
        }
        Ok(())
    }

    async fn transition(
        &self,
        execution: &mut Execution,
        next: ExecutionStatus,
        reason: &str,
        checkpoint: bool,
        retry: Option<RetryRecord>,
        rollback: Option<RollbackRecord>,
    ) -> ExecutionResult<()> {
        let expected = execution.version;
        let state = self.engine.lifecycle.transition(execution, next, reason)?;
        let checkpoint = checkpoint
            .then(|| CheckpointManager::capture(execution, reason))
            .transpose()?;
        let has_checkpoint = checkpoint.is_some();
        let has_retry = retry.is_some();
        let retry_step = retry.as_ref().map(|value| value.step_id);
        let rollback_observation = rollback.as_ref().map(|value| {
            (
                value.step_id,
                value.command_id,
                value.status == RollbackStatus::Success,
            )
        });
        let mut commit = ExecutionCommit::update(execution.clone(), expected);
        commit.state = Some(state);
        commit.checkpoint = checkpoint;
        commit.retry = retry;
        commit.rollback = rollback;
        self.store.commit(&commit, &execution.actor).await?;
        if has_checkpoint {
            self.observe(
                execution,
                ExecutionStage::Checkpoint,
                execution.current_step_id,
                None,
                true,
                Some(reason.into()),
            );
        }
        if has_retry {
            self.observe(
                execution,
                ExecutionStage::Retry,
                retry_step.or(execution.current_step_id),
                None,
                true,
                Some(reason.into()),
            );
        }
        if let Some((step_id, command_id, success)) = rollback_observation {
            self.observe(
                execution,
                ExecutionStage::Rollback,
                Some(step_id),
                Some(command_id),
                success,
                Some(reason.into()),
            );
        }
        self.observe(
            execution,
            ExecutionStage::Lifecycle,
            execution.current_step_id,
            None,
            true,
            Some(reason.into()),
        );
        Ok(())
    }

    async fn progress(
        &self,
        execution: &mut Execution,
        reason: &str,
        checkpoint: bool,
        retry: Option<RetryRecord>,
        rollback: Option<RollbackRecord>,
    ) -> ExecutionResult<()> {
        let expected = execution.version;
        let state = self.engine.lifecycle.record_progress(execution, reason)?;
        let checkpoint = checkpoint
            .then(|| CheckpointManager::capture(execution, reason))
            .transpose()?;
        let has_checkpoint = checkpoint.is_some();
        let has_retry = retry.is_some();
        let retry_step = retry.as_ref().map(|value| value.step_id);
        let rollback_observation = rollback.as_ref().map(|value| {
            (
                value.step_id,
                value.command_id,
                value.status == RollbackStatus::Success,
            )
        });
        let mut commit = ExecutionCommit::update(execution.clone(), expected);
        commit.state = Some(state);
        commit.checkpoint = checkpoint;
        commit.retry = retry;
        commit.rollback = rollback;
        self.store.commit(&commit, &execution.actor).await?;
        if has_checkpoint {
            self.observe(
                execution,
                ExecutionStage::Checkpoint,
                retry_step.or(execution.current_step_id),
                None,
                true,
                Some(reason.into()),
            );
        }
        if has_retry {
            self.observe(
                execution,
                ExecutionStage::Retry,
                execution.current_step_id,
                None,
                true,
                Some(reason.into()),
            );
        }
        if let Some((step_id, command_id, success)) = rollback_observation {
            self.observe(
                execution,
                ExecutionStage::Rollback,
                Some(step_id),
                Some(command_id),
                success,
                Some(reason.into()),
            );
        }
        Ok(())
    }

    fn observe(
        &self,
        execution: &Execution,
        stage: ExecutionStage,
        step_id: Option<Uuid>,
        command_id: Option<Uuid>,
        success: bool,
        message: Option<String>,
    ) {
        let observation = ExecutionObservation {
            execution_id: execution.id,
            plan_id: execution.plan_id,
            step_id,
            command_id,
            stage,
            status: execution.status,
            success,
            message,
        };
        for observer in &self.engine.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }

    fn live_control(&self, id: Uuid) -> ExecutionResult<Option<ExecutionControl>> {
        Ok(self
            .live
            .read()
            .map_err(|_| ExecutionError::Internal("live execution lock poisoned".into()))?
            .get(&id)
            .map(|entry| entry.control.clone()))
    }

    fn live_entry(&self, id: Uuid) -> ExecutionResult<Option<Arc<LiveExecutionEntry>>> {
        Ok(self
            .live
            .read()
            .map_err(|_| ExecutionError::Internal("live execution lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    fn accept_live_pause(
        &self,
        id: Uuid,
        expected: &Arc<LiveExecutionEntry>,
    ) -> ExecutionResult<()> {
        let live = self
            .live
            .read()
            .map_err(|_| ExecutionError::Internal("live execution lock poisoned".into()))?;
        let current = live.get(&id).ok_or_else(|| {
            ExecutionError::Conflict(format!("execution {id} is no longer active"))
        })?;
        if !Arc::ptr_eq(current, expected) || !current.accepting_pause.load(Ordering::SeqCst) {
            return Err(ExecutionError::Conflict(format!(
                "execution {id} no longer accepts pause"
            )));
        }
        current.control.request_pause();
        Ok(())
    }

    fn close_pause_acceptance(&self, id: Uuid) -> ExecutionResult<bool> {
        let live = self
            .live
            .write()
            .map_err(|_| ExecutionError::Internal("live execution lock poisoned".into()))?;
        let current = live
            .get(&id)
            .ok_or_else(|| ExecutionError::Internal(format!("live execution {id} disappeared")))?;
        current.accepting_pause.store(false, Ordering::SeqCst);
        Ok(!current.control.is_pause_requested())
    }

    async fn required(&self, id: Uuid) -> ExecutionResult<Execution> {
        self.store
            .find_execution(id)
            .await?
            .ok_or_else(|| ExecutionError::NotFound(id.to_string()))
    }
}

fn find_step(plan: &Plan, step_id: Uuid) -> ExecutionResult<(Uuid, Step)> {
    plan.tasks
        .values()
        .find_map(|task| {
            task.steps
                .get(&step_id)
                .cloned()
                .map(|step| (task.id, step))
        })
        .ok_or_else(|| ExecutionError::Validation(format!("unknown step {step_id}")))
}

fn bounded(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn apply_cancellation_actor(execution: &mut Execution, control: &ExecutionControl) {
    if let Some(actor) = control.cancellation_actor() {
        execution.actor = actor;
    }
}

fn required_progress_mut(
    execution: &mut Execution,
    step_id: Uuid,
) -> ExecutionResult<&mut crate::domain::StepExecution> {
    execution.steps.get_mut(&step_id).ok_or_else(|| {
        ExecutionError::Validation(format!("execution progress is missing step {step_id}"))
    })
}

struct LiveExecutionGuard {
    execution_id: Uuid,
    live: Arc<RwLock<HashMap<Uuid, Arc<LiveExecutionEntry>>>>,
}

impl Drop for LiveExecutionGuard {
    fn drop(&mut self) {
        if let Ok(mut live) = self.live.write() {
            live.remove(&self.execution_id);
        }
    }
}
