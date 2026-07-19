use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use core_agent_execution::{
    ActionExecutor, CommandFailure, CommandResult, ExecuteRequest, ExecutionCommand,
    ExecutionControl, ExecutionError, ExecutionInterceptor, ExecutionManager, ExecutionObservation,
    ExecutionObserver, ExecutionOperation, ExecutionPolicy, ExecutionStage, ExecutionStatus,
    InMemoryExecutionStore, RetryPolicy, RollbackStatus, SequentialDispatcher,
    SqliteExecutionStore,
};
use core_agent_execution::{DispatchItem, Dispatcher};
use core_agent_plan::{CreateGoalRequest, CreatePlanRequest, PlanningContext, PlanningManager};
use tokio::sync::Notify;

async fn approved_plan() -> core_agent_plan::Plan {
    let planning = PlanningManager::builder().build();
    let goal = planning
        .create_goal(CreateGoalRequest::new("execute", "execute approved plan"))
        .await
        .unwrap();
    planning
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap()
}

#[tokio::test]
async fn default_runtime_executes_an_approved_plan_sequentially() {
    let plan = approved_plan().await;
    let expected_steps = plan
        .tasks
        .values()
        .map(|task| task.steps.len())
        .sum::<usize>();
    let manager = ExecutionManager::builder().build();
    let execution = manager
        .execute(plan, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);
    assert_eq!(execution.completed_order.len(), expected_steps);
    assert!(execution.steps.values().all(|step| step.attempts == 1));
    assert!(manager.list_checkpoints(execution.id).await.unwrap().len() >= expected_steps + 3);
}

#[derive(Default)]
struct CountingStartPolicy {
    starts: AtomicUsize,
}

impl ExecutionPolicy for CountingStartPolicy {
    fn evaluate(
        &self,
        operation: ExecutionOperation,
        _plan: &core_agent_plan::Plan,
        _execution: &core_agent_execution::Execution,
        _command: Option<&ExecutionCommand>,
    ) -> core_agent_execution::ExecutionResult<()> {
        if operation == ExecutionOperation::Start {
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[tokio::test]
async fn prepare_approves_once_and_start_does_not_double_evaluate_policy() {
    let policy = Arc::new(CountingStartPolicy::default());
    let manager = ExecutionManager::builder().policy(policy.clone()).build();
    let prepared = manager
        .prepare(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(prepared.status, ExecutionStatus::Ready);
    let completed = manager.start(prepared.id).await.unwrap();
    assert_eq!(completed.status, ExecutionStatus::Completed);
    assert_eq!(policy.starts.load(Ordering::SeqCst), 1);
}

#[derive(Default)]
struct RevocableStartPolicy {
    deny_start: AtomicBool,
}

impl ExecutionPolicy for RevocableStartPolicy {
    fn evaluate(
        &self,
        operation: ExecutionOperation,
        _plan: &core_agent_plan::Plan,
        _execution: &core_agent_execution::Execution,
        _command: Option<&ExecutionCommand>,
    ) -> core_agent_execution::ExecutionResult<()> {
        if operation == ExecutionOperation::Start && self.deny_start.load(Ordering::SeqCst) {
            return Err(ExecutionError::PolicyDenied("Start was revoked".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn start_rechecks_policy_after_a_prepared_execution_waits() {
    let policy = Arc::new(RevocableStartPolicy::default());
    let executor = Arc::new(CountingSuccess::default());
    let manager = ExecutionManager::builder()
        .policy(policy.clone())
        .executor(executor.clone())
        .build();
    let prepared = manager
        .prepare(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    policy.deny_start.store(true, Ordering::SeqCst);
    assert!(matches!(
        manager.start(prepared.id).await,
        Err(ExecutionError::PolicyDenied(_))
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        manager.find(prepared.id).await.unwrap().unwrap().status,
        ExecutionStatus::Ready
    );
}

#[tokio::test]
async fn resume_cannot_bypass_start_policy_for_a_ready_execution() {
    let policy = Arc::new(RevocableStartPolicy::default());
    let executor = Arc::new(CountingSuccess::default());
    let manager = ExecutionManager::builder()
        .policy(policy.clone())
        .executor(executor.clone())
        .build();
    let prepared = manager
        .prepare(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    policy.deny_start.store(true, Ordering::SeqCst);

    assert!(matches!(
        manager.resume(prepared.id, "resumer").await,
        Err(ExecutionError::PolicyDenied(_))
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        manager.find(prepared.id).await.unwrap().unwrap().status,
        ExecutionStatus::Ready
    );
}

#[tokio::test]
async fn paused_before_start_still_requires_start_policy_on_resume() {
    let policy = Arc::new(RevocableStartPolicy::default());
    let executor = Arc::new(CountingSuccess::default());
    let manager = ExecutionManager::builder()
        .policy(policy.clone())
        .executor(executor.clone())
        .build();
    let prepared = manager
        .prepare(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert!(manager.pause(prepared.id).await.unwrap());
    policy.deny_start.store(true, Ordering::SeqCst);

    assert!(matches!(
        manager.resume(prepared.id, "resumer").await,
        Err(ExecutionError::PolicyDenied(_))
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        manager.find(prepared.id).await.unwrap().unwrap().status,
        ExecutionStatus::Paused
    );
}

struct ZeroRetry;

impl RetryPolicy for ZeroRetry {
    fn retry_delay(
        &self,
        failure: &CommandFailure,
        attempt: u32,
        max_attempts: u32,
    ) -> Option<Duration> {
        (failure.retryable && attempt < max_attempts).then_some(Duration::ZERO)
    }
}

struct SlowRetry;

impl RetryPolicy for SlowRetry {
    fn retry_delay(
        &self,
        failure: &CommandFailure,
        attempt: u32,
        max_attempts: u32,
    ) -> Option<Duration> {
        (failure.retryable && attempt < max_attempts).then_some(Duration::from_secs(5))
    }
}

#[derive(Default)]
struct FailOnceExecutor {
    calls: AtomicUsize,
}

#[async_trait]
impl ActionExecutor for FailOnceExecutor {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(CommandFailure::new("TEMPORARY", "try again", true))
        } else {
            Ok(CommandResult::acknowledged(command.action_kind))
        }
    }
}

#[tokio::test]
async fn retry_is_bounded_by_the_plan_step_attempt_limit() {
    let mut plan = approved_plan().await;
    for task in plan.tasks.values_mut() {
        for step in task.steps.values_mut() {
            step.max_attempts = 2;
        }
    }
    let executor = Arc::new(FailOnceExecutor::default());
    let manager = ExecutionManager::builder()
        .executor(executor.clone())
        .retry_policy(Arc::new(ZeroRetry))
        .build();
    let execution = manager
        .execute(plan, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        execution.steps.len() + 1
    );
    let retries = manager.list_retries(execution.id).await.unwrap();
    assert_eq!(retries.len(), 2);
}

struct FailSecondWithCompensation {
    calls: AtomicUsize,
    rolled_back: Mutex<Vec<uuid::Uuid>>,
}

#[derive(Default)]
struct RecordingExecutionObserver {
    values: Mutex<Vec<ExecutionObservation>>,
}

impl ExecutionObserver for RecordingExecutionObserver {
    fn on_observation(&self, observation: &ExecutionObservation) {
        self.values.lock().unwrap().push(observation.clone());
    }
}

#[async_trait]
impl ActionExecutor for FailSecondWithCompensation {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
            Err(CommandFailure::new("PERMANENT", "failed", false))
        } else {
            Ok(CommandResult::acknowledged(command.action_kind))
        }
    }

    fn supports_rollback(&self, _command: &ExecutionCommand) -> bool {
        true
    }

    async fn rollback(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<(), CommandFailure> {
        self.rolled_back.lock().unwrap().push(command.step_id);
        Ok(())
    }
}

#[tokio::test]
async fn permanent_failure_runs_only_explicit_compensation() {
    let executor = Arc::new(FailSecondWithCompensation {
        calls: AtomicUsize::new(0),
        rolled_back: Mutex::new(Vec::new()),
    });
    let observer = Arc::new(RecordingExecutionObserver::default());
    let manager = ExecutionManager::builder()
        .executor(executor.clone())
        .observer(observer.clone())
        .build();
    let execution = manager
        .execute(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Failed);
    let records = manager.list_rollbacks(execution.id).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].status, RollbackStatus::Success);
    assert_eq!(executor.rolled_back.lock().unwrap().len(), 1);
    let observations = observer.values.lock().unwrap();
    let rollback = observations
        .iter()
        .find(|value| value.stage == ExecutionStage::Rollback)
        .unwrap();
    assert_eq!(rollback.step_id, Some(records[0].step_id));
    assert_eq!(rollback.command_id, Some(records[0].command_id));
}

struct GateExecutor {
    entered: Arc<Notify>,
    release: Arc<Notify>,
    calls: AtomicUsize,
}

struct BlockingTerminalDispatcher {
    inner: SequentialDispatcher,
    entered: Arc<Notify>,
    release: Arc<AtomicBool>,
}

impl Dispatcher for BlockingTerminalDispatcher {
    fn next(
        &self,
        plan: &core_agent_plan::Plan,
        execution: &core_agent_execution::Execution,
    ) -> core_agent_execution::ExecutionResult<Option<DispatchItem>> {
        let next = self.inner.next(plan, execution)?;
        if next.is_none()
            && execution.steps.values().all(|progress| {
                progress.status == core_agent_execution::ActionExecutionStatus::Completed
            })
        {
            self.entered.notify_one();
            while !self.release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
        Ok(next)
    }
}

#[async_trait]
impl ActionExecutor for GateExecutor {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(CommandResult::acknowledged(command.action_kind))
    }
}

#[tokio::test]
async fn pause_waits_for_a_safe_boundary_and_resume_finishes() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let store = Arc::new(InMemoryExecutionStore::default());
    let manager = Arc::new(
        ExecutionManager::builder()
            .store(store)
            .executor(Arc::new(GateExecutor {
                entered: entered.clone(),
                release: release.clone(),
                calls: AtomicUsize::new(0),
            }))
            .build(),
    );
    let plan = approved_plan().await;
    let plan_id = plan.id;
    let task = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.execute(plan, ExecuteRequest::new("tester")).await })
    };
    entered.notified().await;
    let id = manager.list(plan_id).await.unwrap()[0].id;
    assert!(manager.pause(id).await.unwrap());
    release.notify_one();
    let paused = task.await.unwrap().unwrap();
    assert_eq!(paused.status, ExecutionStatus::Paused);

    let checkpoint = manager.list_checkpoints(id).await.unwrap().pop().unwrap();
    let restored = manager
        .restore_checkpoint(checkpoint.id, "restorer")
        .await
        .unwrap();
    assert_eq!(restored.status, ExecutionStatus::Paused);
    assert_eq!(restored.version, paused.version + 1);
    let completed = manager.resume(id, "resumer").await.unwrap();
    assert_eq!(completed.status, ExecutionStatus::Completed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_wins_at_the_last_command_completion_boundary() {
    let entered = Arc::new(Notify::new());
    let release = Arc::new(AtomicBool::new(false));
    let manager = Arc::new(
        ExecutionManager::builder()
            .dispatcher(Arc::new(BlockingTerminalDispatcher {
                inner: SequentialDispatcher,
                entered: Arc::clone(&entered),
                release: Arc::clone(&release),
            }))
            .build(),
    );
    let plan = approved_plan().await;
    let plan_id = plan.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.execute(plan, ExecuteRequest::new("tester")).await })
    };
    entered.notified().await;
    let execution_id = manager.list(plan_id).await.unwrap()[0].id;
    assert!(manager.pause(execution_id).await.unwrap());
    release.store(true, Ordering::Release);

    let paused = running.await.unwrap().unwrap();
    assert_eq!(paused.status, ExecutionStatus::Paused);
}

struct CancelExecutor {
    entered: Arc<Notify>,
}

#[async_trait]
impl ActionExecutor for CancelExecutor {
    async fn execute(
        &self,
        _command: &ExecutionCommand,
        control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        self.entered.notify_one();
        control.cancelled().await;
        Err(CommandFailure::cancelled("cancelled by test"))
    }
}

#[tokio::test]
async fn in_flight_cancel_is_cooperative_and_durable() {
    let entered = Arc::new(Notify::new());
    let manager = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(CancelExecutor {
                entered: entered.clone(),
            }))
            .build(),
    );
    let plan = approved_plan().await;
    let plan_id = plan.id;
    let task = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.execute(plan, ExecuteRequest::new("tester")).await })
    };
    entered.notified().await;
    let id = manager.list(plan_id).await.unwrap()[0].id;
    assert!(manager.cancel(id, "canceller").await.unwrap());
    let execution = task.await.unwrap().unwrap();
    assert_eq!(execution.status, ExecutionStatus::Cancelled);
    assert_eq!(execution.actor, "canceller");
    assert_eq!(
        manager.find(id).await.unwrap().unwrap().status,
        ExecutionStatus::Cancelled
    );
}

#[tokio::test]
async fn sqlite_cold_recovery_is_strict_and_checkpoints_survive() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("execution.db");
    let store = Arc::new(SqliteExecutionStore::new(&database).unwrap());
    let manager = ExecutionManager::new(store);
    let execution = manager
        .execute(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    drop(manager);

    let cold = ExecutionManager::new(Arc::new(SqliteExecutionStore::new(&database).unwrap()));
    assert_eq!(cold.find(execution.id).await.unwrap().unwrap(), execution);
    assert!(!cold
        .list_checkpoints(execution.id)
        .await
        .unwrap()
        .is_empty());
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE execution SET status = 'FAILED' WHERE id = ?1",
            [execution.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        cold.find(execution.id).await,
        Err(ExecutionError::Validation(_))
    ));
}

struct DenyDispatch;

impl ExecutionPolicy for DenyDispatch {
    fn evaluate(
        &self,
        operation: ExecutionOperation,
        _plan: &core_agent_plan::Plan,
        _execution: &core_agent_execution::Execution,
        _command: Option<&ExecutionCommand>,
    ) -> core_agent_execution::ExecutionResult<()> {
        if operation == ExecutionOperation::Dispatch {
            Err(ExecutionError::PolicyDenied("dispatch denied".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn dispatch_policy_denial_becomes_a_durable_failed_execution() {
    let manager = ExecutionManager::builder()
        .policy(Arc::new(DenyDispatch))
        .build();
    let execution = manager
        .execute(approved_plan().await, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Failed);
    let failure = execution
        .steps
        .values()
        .find_map(|step| step.error.as_ref())
        .unwrap();
    assert_eq!(failure.kind, "COMMAND_PREFLIGHT");
    assert_eq!(
        manager.find(execution.id).await.unwrap().unwrap(),
        execution
    );
}

#[tokio::test]
async fn recovered_in_flight_command_is_never_replayed_automatically() {
    let entered = Arc::new(Notify::new());
    let store = Arc::new(InMemoryExecutionStore::default());
    let manager = Arc::new(
        ExecutionManager::builder()
            .store(store.clone())
            .executor(Arc::new(CancelExecutor {
                entered: entered.clone(),
            }))
            .build(),
    );
    let plan = approved_plan().await;
    let plan_id = plan.id;
    let task = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.execute(plan, ExecuteRequest::new("tester")).await })
    };
    entered.notified().await;
    let id = manager.list(plan_id).await.unwrap()[0].id;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    drop(manager);

    let cold = ExecutionManager::new(store);
    assert!(matches!(
        cold.resume(id, "recovery").await,
        Err(ExecutionError::OutcomeUnknown(_))
    ));
    assert_eq!(
        cold.find(id).await.unwrap().unwrap().status,
        ExecutionStatus::Running
    );
}

#[tokio::test]
async fn sqlite_rejects_tampered_retry_columns() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("retry-corrupt.db");
    let mut plan = approved_plan().await;
    for task in plan.tasks.values_mut() {
        for step in task.steps.values_mut() {
            step.max_attempts = 2;
        }
    }
    let manager = ExecutionManager::builder()
        .store(Arc::new(SqliteExecutionStore::new(&database).unwrap()))
        .executor(Arc::new(FailOnceExecutor::default()))
        .retry_policy(Arc::new(ZeroRetry))
        .build();
    let execution = manager
        .execute(plan, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE retry SET status = 'TAMPERED' WHERE execution_id = ?1",
            [execution.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        manager.list_retries(execution.id).await,
        Err(ExecutionError::Validation(_))
    ));
}

#[derive(Default)]
struct CountingSuccess {
    calls: AtomicUsize,
}

#[async_trait]
impl ActionExecutor for CountingSuccess {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CommandResult::acknowledged(command.action_kind))
    }
}

struct FailingAfterCommand;

impl ExecutionInterceptor for FailingAfterCommand {
    fn after_command(
        &self,
        _execution: &core_agent_execution::Execution,
        _command: &ExecutionCommand,
        _result: &mut CommandResult,
    ) -> core_agent_execution::ExecutionResult<()> {
        Err(ExecutionError::Extension("audit hook unavailable".into()))
    }
}

#[tokio::test]
async fn post_command_extension_failure_never_replays_a_successful_side_effect() {
    let plan = approved_plan().await;
    let step_count = plan
        .tasks
        .values()
        .map(|task| task.steps.len())
        .sum::<usize>();
    let executor = Arc::new(CountingSuccess::default());
    let execution = ExecutionManager::builder()
        .executor(executor.clone())
        .interceptor(Arc::new(FailingAfterCommand))
        .build()
        .execute(plan, ExecuteRequest::new("tester"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), step_count);
}

struct AlwaysRetryable {
    entered: Arc<Notify>,
}

#[async_trait]
impl ActionExecutor for AlwaysRetryable {
    async fn execute(
        &self,
        _command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        self.entered.notify_one();
        Err(CommandFailure::new("TEMPORARY", "retry later", true))
    }
}

#[tokio::test]
async fn cancel_during_retry_delay_closes_step_and_clears_current_identity() {
    let mut plan = approved_plan().await;
    for task in plan.tasks.values_mut() {
        for step in task.steps.values_mut() {
            step.max_attempts = 2;
        }
    }
    let plan_id = plan.id;
    let entered = Arc::new(Notify::new());
    let manager = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(AlwaysRetryable {
                entered: entered.clone(),
            }))
            .retry_policy(Arc::new(SlowRetry))
            .build(),
    );
    let task = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.execute(plan, ExecuteRequest::new("tester")).await })
    };
    entered.notified().await;
    let id = manager.list(plan_id).await.unwrap()[0].id;
    for _ in 0..100 {
        if manager.find(id).await.unwrap().unwrap().status == ExecutionStatus::Retrying {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(manager.cancel(id, "canceller").await.unwrap());
    let execution = task.await.unwrap().unwrap();
    assert_eq!(execution.status, ExecutionStatus::Cancelled);
    assert!(execution.current_task_id.is_none());
    assert!(execution.current_step_id.is_none());
    assert!(execution
        .steps
        .values()
        .any(|step| step.status == core_agent_execution::ActionExecutionStatus::Cancelled));
}
