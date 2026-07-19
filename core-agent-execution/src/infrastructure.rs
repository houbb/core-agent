use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use core_agent_plan::Plan;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::{
    CommandFailure, CommandResult, Execution, ExecutionCheckpoint, ExecutionCommand,
    ExecutionStateRecord, ExecutionStatus, RetryRecord, RollbackRecord,
};
use crate::error::ExecutionResult;

#[derive(Debug, Clone)]
pub struct DispatchItem {
    pub task_id: Uuid,
    pub step_id: Uuid,
}

pub trait Dispatcher: Send + Sync {
    fn next(&self, plan: &Plan, execution: &Execution) -> ExecutionResult<Option<DispatchItem>>;
}

#[derive(Clone)]
pub struct ExecutionControl {
    cancellation: CancellationToken,
    pause_requested: Arc<AtomicBool>,
    cancellation_actor: Arc<RwLock<Option<String>>>,
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            pause_requested: Arc::new(AtomicBool::new(false)),
            cancellation_actor: Arc::new(RwLock::new(None)),
        }
    }
}

impl ExecutionControl {
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn cancel_as(&self, actor: impl Into<String>) {
        if let Ok(mut value) = self.cancellation_actor.write() {
            *value = Some(actor.into());
        }
        self.cancellation.cancel();
    }

    pub fn cancellation_actor(&self) -> Option<String> {
        self.cancellation_actor
            .read()
            .ok()
            .and_then(|value| value.clone())
    }

    pub fn request_pause(&self) {
        self.pause_requested.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn is_pause_requested(&self) -> bool {
        self.pause_requested.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }
}

#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure>;

    fn supports_rollback(&self, _command: &ExecutionCommand) -> bool {
        false
    }

    async fn rollback(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<(), CommandFailure> {
        Err(CommandFailure::new(
            "ROLLBACK_UNSUPPORTED",
            format!("command {} has no compensation", command.id),
            false,
        ))
    }
}

pub trait RetryPolicy: Send + Sync {
    fn retry_delay(
        &self,
        failure: &CommandFailure,
        attempt: u32,
        max_attempts: u32,
    ) -> Option<Duration>;
}

pub trait RollbackPolicy: Send + Sync {
    fn should_rollback(&self, execution: &Execution, failure: &CommandFailure) -> bool;
}

pub trait CheckpointStore: ExecutionStore {}
impl<T> CheckpointStore for T where T: ExecutionStore {}

pub trait StateMachine: Send + Sync {
    fn can_transition(&self, current: ExecutionStatus, next: ExecutionStatus) -> bool;
}

pub trait ExecutionLifecycle: Send + Sync {
    fn transition(
        &self,
        execution: &mut Execution,
        next: ExecutionStatus,
        reason: &str,
    ) -> ExecutionResult<ExecutionStateRecord>;

    fn record_progress(
        &self,
        execution: &mut Execution,
        reason: &str,
    ) -> ExecutionResult<ExecutionStateRecord>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOperation {
    Prepare,
    Start,
    Dispatch,
    Pause,
    Resume,
    Cancel,
    Retry,
    Rollback,
    Restore,
}

pub trait ExecutionPolicy: Send + Sync {
    fn evaluate(
        &self,
        operation: ExecutionOperation,
        plan: &Plan,
        execution: &Execution,
        command: Option<&ExecutionCommand>,
    ) -> ExecutionResult<()>;
}

pub trait ExecutionInterceptor: Send + Sync {
    fn before_command(
        &self,
        _execution: &Execution,
        _command: &mut ExecutionCommand,
    ) -> ExecutionResult<()> {
        Ok(())
    }

    fn after_command(
        &self,
        _execution: &Execution,
        _command: &ExecutionCommand,
        _result: &mut CommandResult,
    ) -> ExecutionResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStage {
    Lifecycle,
    Dispatch,
    Command,
    Retry,
    Checkpoint,
    Rollback,
}

#[derive(Debug, Clone)]
pub struct ExecutionObservation {
    pub execution_id: Uuid,
    pub plan_id: Uuid,
    pub step_id: Option<Uuid>,
    pub command_id: Option<Uuid>,
    pub stage: ExecutionStage,
    pub status: ExecutionStatus,
    pub success: bool,
    pub message: Option<String>,
}

pub trait ExecutionObserver: Send + Sync {
    fn on_observation(&self, observation: &ExecutionObservation);
}

#[derive(Debug, Clone)]
pub struct ExecutionCommit {
    pub execution: Execution,
    pub expected_version: Option<u64>,
    pub state: Option<ExecutionStateRecord>,
    pub checkpoint: Option<ExecutionCheckpoint>,
    pub retry: Option<RetryRecord>,
    pub rollback: Option<RollbackRecord>,
}

impl ExecutionCommit {
    pub fn create(execution: Execution, state: ExecutionStateRecord) -> Self {
        Self {
            execution,
            expected_version: None,
            state: Some(state),
            checkpoint: None,
            retry: None,
            rollback: None,
        }
    }

    pub fn update(execution: Execution, expected_version: u64) -> Self {
        Self {
            execution,
            expected_version: Some(expected_version),
            state: None,
            checkpoint: None,
            retry: None,
            rollback: None,
        }
    }

    pub fn validate(&self, actor: &str) -> ExecutionResult<()> {
        self.execution.validate()?;
        if actor.trim().is_empty()
            || actor.len() > 256
            || actor.chars().any(char::is_control)
            || self
                .expected_version
                .is_some_and(|expected| expected.checked_add(1) != Some(self.execution.version))
        {
            return Err(crate::error::ExecutionError::Validation(
                "execution commit actor or version is invalid".into(),
            ));
        }
        if let Some(state) = &self.state {
            if state.execution_id != self.execution.id
                || state.sequence != self.execution.version
                || state.to_status != self.execution.status
                || state.reason.trim().is_empty()
                || state.reason.len() > 1024
            {
                return Err(crate::error::ExecutionError::Validation(
                    "execution state record does not match aggregate".into(),
                ));
            }
        }
        if let Some(checkpoint) = &self.checkpoint {
            checkpoint.validate()?;
            if checkpoint.execution_id != self.execution.id {
                return Err(crate::error::ExecutionError::Validation(
                    "checkpoint belongs to another execution".into(),
                ));
            }
        }
        if let Some(retry) = &self.retry {
            let action_matches = self
                .execution
                .steps
                .get(&retry.step_id)
                .is_some_and(|step| step.action_id == retry.action_id);
            if retry.execution_id != self.execution.id
                || !action_matches
                || retry.attempt == 0
                || retry.attempt > 100
                || retry.delay_ms > i64::MAX as u64
                || retry.error_kind.trim().is_empty()
                || retry.error_kind.len() > 64
                || retry.error_message.len() > 1024
            {
                return Err(crate::error::ExecutionError::Validation(
                    "retry record is invalid".into(),
                ));
            }
        }
        if let Some(rollback) = &self.rollback {
            let action_matches = self
                .execution
                .steps
                .get(&rollback.step_id)
                .is_some_and(|step| step.action_id == rollback.action_id);
            if rollback.execution_id != self.execution.id
                || !action_matches
                || rollback
                    .error_kind
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 64)
                || rollback
                    .error_message
                    .as_ref()
                    .is_some_and(|value| value.len() > 1024)
            {
                return Err(crate::error::ExecutionError::Validation(
                    "rollback record is invalid".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait ExecutionStore: Send + Sync {
    async fn commit(&self, commit: &ExecutionCommit, actor: &str) -> ExecutionResult<()>;
    async fn find_execution(&self, id: Uuid) -> ExecutionResult<Option<Execution>>;
    async fn list_executions(&self, plan_id: Uuid) -> ExecutionResult<Vec<Execution>>;
    async fn list_checkpoints(
        &self,
        execution_id: Uuid,
    ) -> ExecutionResult<Vec<ExecutionCheckpoint>>;
    async fn find_checkpoint(&self, id: Uuid) -> ExecutionResult<Option<ExecutionCheckpoint>>;
    async fn list_states(&self, execution_id: Uuid) -> ExecutionResult<Vec<ExecutionStateRecord>>;
    async fn list_retries(&self, execution_id: Uuid) -> ExecutionResult<Vec<RetryRecord>>;
    async fn list_rollbacks(&self, execution_id: Uuid) -> ExecutionResult<Vec<RollbackRecord>>;
}
