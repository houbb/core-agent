use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use core_agent_plan::Plan;
use uuid::Uuid;

use crate::domain::{
    CommandFailure, CommandResult, Execution, ExecutionCheckpoint, ExecutionCommand,
    ExecutionStateRecord, ExecutionStatus, RetryRecord, RollbackRecord,
};
use crate::error::{ExecutionError, ExecutionResult};
use crate::infrastructure::{
    ActionExecutor, DispatchItem, Dispatcher, ExecutionCommit, ExecutionControl,
    ExecutionLifecycle, ExecutionOperation, ExecutionPolicy, ExecutionStore, RetryPolicy,
    RollbackPolicy, StateMachine,
};

pub struct SequentialDispatcher;

impl Dispatcher for SequentialDispatcher {
    fn next(&self, plan: &Plan, execution: &Execution) -> ExecutionResult<Option<DispatchItem>> {
        let task_completed = |task_id: Uuid| {
            plan.tasks.get(&task_id).is_some_and(|task| {
                task.steps.keys().all(|step_id| {
                    execution.steps.get(step_id).is_some_and(|progress| {
                        progress.status == crate::domain::ActionExecutionStatus::Completed
                    })
                })
            })
        };
        let step_completed = |step_id: Uuid| {
            execution.steps.get(&step_id).is_some_and(|progress| {
                progress.status == crate::domain::ActionExecutionStatus::Completed
            })
        };

        let mut tasks = plan.tasks.values().collect::<Vec<_>>();
        tasks.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.key.cmp(&right.key))
                .then_with(|| left.id.cmp(&right.id))
        });
        for task in tasks {
            if !task.dependencies.iter().all(|id| task_completed(*id)) {
                continue;
            }
            let mut steps = task.steps.values().collect::<Vec<_>>();
            steps.sort_by(|left, right| left.key.cmp(&right.key).then(left.id.cmp(&right.id)));
            for step in steps {
                let progress = execution.steps.get(&step.id).ok_or_else(|| {
                    ExecutionError::Validation(format!("missing progress for step {}", step.id))
                })?;
                if progress.status == crate::domain::ActionExecutionStatus::Pending
                    && step.dependencies.iter().all(|id| step_completed(*id))
                {
                    return Ok(Some(DispatchItem {
                        task_id: task.id,
                        step_id: step.id,
                    }));
                }
            }
        }
        Ok(None)
    }
}

pub struct DefaultActionExecutor;

#[async_trait]
impl ActionExecutor for DefaultActionExecutor {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        if control.is_cancelled() {
            return Err(CommandFailure::cancelled("execution cancelled"));
        }
        match command.kind {
            crate::domain::CommandKind::Builtin => {
                Ok(CommandResult::acknowledged(command.action_kind))
            }
            crate::domain::CommandKind::Tool => Err(CommandFailure::new(
                "TOOL_EXECUTOR_MISSING",
                "Tool command requires an ActionExecutor adapter",
                false,
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExponentialRetryPolicy {
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for ExponentialRetryPolicy {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy for ExponentialRetryPolicy {
    fn retry_delay(
        &self,
        failure: &CommandFailure,
        attempt: u32,
        max_attempts: u32,
    ) -> Option<Duration> {
        if !failure.retryable || attempt >= max_attempts {
            return None;
        }
        let multiplier = 1_u32
            .checked_shl(attempt.saturating_sub(1).min(20))
            .unwrap_or(u32::MAX);
        Some(
            self.base_delay
                .saturating_mul(multiplier)
                .min(self.max_delay),
        )
    }
}

pub struct ExplicitRollbackPolicy;

impl RollbackPolicy for ExplicitRollbackPolicy {
    fn should_rollback(&self, execution: &Execution, failure: &CommandFailure) -> bool {
        !failure.cancelled && !execution.completed_order.is_empty()
    }
}

pub struct DefaultStateMachine;

impl StateMachine for DefaultStateMachine {
    fn can_transition(&self, current: ExecutionStatus, next: ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        matches!(
            (current, next),
            (Pending, Ready | Cancelled | Failed)
                | (Ready, Running | Paused | Cancelled | Failed)
                | (
                    Running,
                    Retrying | Waiting | Paused | Completed | Failed | Cancelled
                )
                | (Retrying, Running | Paused | Failed | Cancelled)
                | (Paused, Running | Cancelled)
                | (Waiting, Running | Failed | Cancelled)
        )
    }
}

pub struct DefaultExecutionLifecycle {
    state_machine: Arc<dyn StateMachine>,
}

impl Default for DefaultExecutionLifecycle {
    fn default() -> Self {
        Self {
            state_machine: Arc::new(DefaultStateMachine),
        }
    }
}

impl DefaultExecutionLifecycle {
    pub fn new(state_machine: Arc<dyn StateMachine>) -> Self {
        Self { state_machine }
    }

    fn record(
        execution: &mut Execution,
        from_status: Option<ExecutionStatus>,
        reason: &str,
    ) -> ExecutionResult<ExecutionStateRecord> {
        if reason.trim().is_empty() || reason.len() > 1024 {
            return Err(ExecutionError::Validation(
                "execution state reason must contain 1..=1024 characters".into(),
            ));
        }
        execution.updated_at = Utc::now();
        execution.version = execution
            .version
            .checked_add(1)
            .ok_or_else(|| ExecutionError::Internal("execution version overflow".into()))?;
        Ok(ExecutionStateRecord {
            id: Uuid::new_v4(),
            execution_id: execution.id,
            sequence: execution.version,
            from_status,
            to_status: execution.status,
            reason: reason.into(),
            created_at: execution.updated_at,
        })
    }
}

impl ExecutionLifecycle for DefaultExecutionLifecycle {
    fn transition(
        &self,
        execution: &mut Execution,
        next: ExecutionStatus,
        reason: &str,
    ) -> ExecutionResult<ExecutionStateRecord> {
        let previous = execution.status;
        if !self.state_machine.can_transition(previous, next) {
            return Err(ExecutionError::InvalidState(format!(
                "{} -> {}",
                previous.as_str(),
                next.as_str()
            )));
        }
        execution.status = next;
        if next == ExecutionStatus::Running && execution.started_at.is_none() {
            execution.started_at = Some(Utc::now());
        }
        if next.is_terminal() {
            execution.completed_at = Some(Utc::now());
        }
        Self::record(execution, Some(previous), reason)
    }

    fn record_progress(
        &self,
        execution: &mut Execution,
        reason: &str,
    ) -> ExecutionResult<ExecutionStateRecord> {
        Self::record(execution, Some(execution.status), reason)
    }
}

pub struct AllowAllExecutionPolicy;

impl ExecutionPolicy for AllowAllExecutionPolicy {
    fn evaluate(
        &self,
        _operation: ExecutionOperation,
        _plan: &Plan,
        _execution: &Execution,
        _command: Option<&ExecutionCommand>,
    ) -> ExecutionResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryExecutionStore {
    inner: Mutex<InMemoryData>,
}

#[derive(Default)]
struct InMemoryData {
    executions: HashMap<Uuid, Execution>,
    checkpoints: BTreeMap<Uuid, ExecutionCheckpoint>,
    states: BTreeMap<Uuid, ExecutionStateRecord>,
    retries: BTreeMap<Uuid, RetryRecord>,
    rollbacks: BTreeMap<Uuid, RollbackRecord>,
}

#[async_trait]
impl ExecutionStore for InMemoryExecutionStore {
    async fn commit(&self, commit: &ExecutionCommit, actor: &str) -> ExecutionResult<()> {
        commit.validate(actor)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ExecutionError::Internal("execution store lock poisoned".into()))?;
        match (
            inner.executions.get(&commit.execution.id),
            commit.expected_version,
        ) {
            (None, None) => {}
            (Some(current), Some(expected)) if current.version == expected => {}
            (Some(_), None) => {
                return Err(ExecutionError::Conflict(format!(
                    "execution {} already exists",
                    commit.execution.id
                )))
            }
            (_, Some(expected)) => {
                return Err(ExecutionError::Conflict(format!(
                    "execution {} expected version {expected}",
                    commit.execution.id
                )))
            }
        }
        if commit
            .state
            .as_ref()
            .is_some_and(|value| inner.states.contains_key(&value.id))
            || commit
                .checkpoint
                .as_ref()
                .is_some_and(|value| inner.checkpoints.contains_key(&value.id))
            || commit
                .retry
                .as_ref()
                .is_some_and(|value| inner.retries.contains_key(&value.id))
            || commit
                .rollback
                .as_ref()
                .is_some_and(|value| inner.rollbacks.contains_key(&value.id))
        {
            return Err(ExecutionError::Conflict(
                "execution record already exists".into(),
            ));
        }
        inner
            .executions
            .insert(commit.execution.id, commit.execution.clone());
        if let Some(value) = &commit.state {
            inner.states.insert(value.id, value.clone());
        }
        if let Some(value) = &commit.checkpoint {
            inner.checkpoints.insert(value.id, value.clone());
        }
        if let Some(value) = &commit.retry {
            inner.retries.insert(value.id, value.clone());
        }
        if let Some(value) = &commit.rollback {
            inner.rollbacks.insert(value.id, value.clone());
        }
        Ok(())
    }

    async fn find_execution(&self, id: Uuid) -> ExecutionResult<Option<Execution>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| ExecutionError::Internal("execution store lock poisoned".into()))?
            .executions
            .get(&id)
            .cloned())
    }

    async fn list_executions(&self, plan_id: Uuid) -> ExecutionResult<Vec<Execution>> {
        let mut values = self
            .inner
            .lock()
            .map_err(|_| ExecutionError::Internal("execution store lock poisoned".into()))?
            .executions
            .values()
            .filter(|value| value.plan_id == plan_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| value.created_at);
        Ok(values)
    }

    async fn list_checkpoints(
        &self,
        execution_id: Uuid,
    ) -> ExecutionResult<Vec<ExecutionCheckpoint>> {
        select_records(
            &self.inner,
            |data| &data.checkpoints,
            execution_id,
            |value| value.execution_id,
            |value| (value.sequence, value.created_at),
        )
    }

    async fn find_checkpoint(&self, id: Uuid) -> ExecutionResult<Option<ExecutionCheckpoint>> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| ExecutionError::Internal("execution store lock poisoned".into()))?
            .checkpoints
            .get(&id)
            .cloned())
    }

    async fn list_states(&self, execution_id: Uuid) -> ExecutionResult<Vec<ExecutionStateRecord>> {
        select_records(
            &self.inner,
            |data| &data.states,
            execution_id,
            |value| value.execution_id,
            |value| (value.sequence, value.created_at),
        )
    }

    async fn list_retries(&self, execution_id: Uuid) -> ExecutionResult<Vec<RetryRecord>> {
        select_records(
            &self.inner,
            |data| &data.retries,
            execution_id,
            |value| value.execution_id,
            |value| (value.attempt as u64, value.created_at),
        )
    }

    async fn list_rollbacks(&self, execution_id: Uuid) -> ExecutionResult<Vec<RollbackRecord>> {
        select_records(
            &self.inner,
            |data| &data.rollbacks,
            execution_id,
            |value| value.execution_id,
            |value| (0, value.created_at),
        )
    }
}

fn select_records<T: Clone, K: Ord>(
    store: &Mutex<InMemoryData>,
    select: impl Fn(&InMemoryData) -> &BTreeMap<Uuid, T>,
    execution_id: Uuid,
    owner: impl Fn(&T) -> Uuid,
    order: impl Fn(&T) -> K,
) -> ExecutionResult<Vec<T>> {
    let inner = store
        .lock()
        .map_err(|_| ExecutionError::Internal("execution store lock poisoned".into()))?;
    let mut values = select(&inner)
        .values()
        .filter(|value| owner(value) == execution_id)
        .cloned()
        .collect::<Vec<_>>();
    values.sort_by_key(order);
    Ok(values)
}

pub struct CheckpointManager;

impl CheckpointManager {
    pub fn capture(execution: &Execution, label: &str) -> ExecutionResult<ExecutionCheckpoint> {
        ExecutionCheckpoint::capture(execution, label)
    }

    pub fn restore(checkpoint: &ExecutionCheckpoint) -> ExecutionResult<Execution> {
        checkpoint.validate()?;
        if checkpoint.content.status.is_terminal() || checkpoint.content.has_uncertain_action() {
            return Err(ExecutionError::InvalidState(
                "terminal or outcome-unknown checkpoint cannot be restored".into(),
            ));
        }
        Ok(checkpoint.content.clone())
    }
}

pub struct RetryManager;

impl RetryManager {
    pub fn delay(
        policy: &dyn RetryPolicy,
        failure: &CommandFailure,
        attempt: u32,
        max_attempts: u32,
    ) -> Option<Duration> {
        policy.retry_delay(failure, attempt, max_attempts)
    }
}

pub struct RollbackManager;

impl RollbackManager {
    pub fn planned_steps(execution: &Execution) -> impl Iterator<Item = Uuid> + '_ {
        execution.completed_order.iter().rev().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_rejects_terminal_restart() {
        let machine = DefaultStateMachine;
        assert!(machine.can_transition(ExecutionStatus::Paused, ExecutionStatus::Running));
        assert!(!machine.can_transition(ExecutionStatus::Completed, ExecutionStatus::Running));
    }

    #[test]
    fn retry_policy_respects_retryability_and_attempt_bound() {
        let policy = ExponentialRetryPolicy::default();
        let failure = CommandFailure::new("TEMPORARY", "retry", true);
        assert!(policy.retry_delay(&failure, 1, 2).is_some());
        assert!(policy.retry_delay(&failure, 2, 2).is_none());
        assert!(policy
            .retry_delay(&CommandFailure::new("DENIED", "no", false), 1, 2)
            .is_none());
    }
}
