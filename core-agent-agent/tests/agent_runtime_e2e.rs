use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use core_agent_agent::{
    Agent, AgentCommit, AgentCoordinator, AgentError, AgentExecutionControl, AgentGoalRequest,
    AgentManager, AgentObservation, AgentObserver, AgentOperation, AgentPolicy,
    AgentPolicyDecision, AgentPolicyDefinition, AgentProfile, AgentResult, AgentRunReference,
    AgentSnapshot, AgentStage, AgentState, AgentStateRecord, AgentStore, CreateAgentRequest,
    InMemoryAgentStore, RuntimeAgentCoordinator, SqliteAgentStore,
};
use core_agent_execution::{
    ActionExecutor, CommandFailure, CommandResult, Execution, ExecutionCommand, ExecutionControl,
    ExecutionManager, ExecutionOperation, ExecutionPolicy, ExecutionStatus,
};
use core_agent_plan::{
    CreateGoalRequest, PlanBuilder, PlanDraft, PlanError, PlanningContext, PlanningManager,
    ToolReference,
};
use rusqlite::Connection;
use tempfile::tempdir;
use tokio::sync::Notify;

struct BlockingStateStore {
    inner: Arc<InMemoryAgentStore>,
    target: Mutex<Option<AgentState>>,
    fail_target: Mutex<Option<AgentState>>,
    blocked: Notify,
    release: Notify,
}

impl Default for BlockingStateStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(InMemoryAgentStore::default()),
            target: Mutex::new(None),
            fail_target: Mutex::new(None),
            blocked: Notify::new(),
            release: Notify::new(),
        }
    }
}

impl BlockingStateStore {
    fn block_next(&self, state: AgentState) {
        *self.target.lock().unwrap() = Some(state);
    }

    fn fail_next(&self, state: AgentState) {
        *self.fail_target.lock().unwrap() = Some(state);
    }

    async fn wait_blocked(&self) {
        self.blocked.notified().await;
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

#[async_trait]
impl AgentStore for BlockingStateStore {
    async fn commit(&self, commit: &AgentCommit, actor: &str) -> AgentResult<()> {
        let should_fail = {
            let mut target = self.fail_target.lock().unwrap();
            if *target == Some(commit.agent.state) {
                target.take();
                true
            } else {
                false
            }
        };
        if should_fail {
            return Err(AgentError::Internal("injected Agent commit failure".into()));
        }
        let should_block = {
            let mut target = self.target.lock().unwrap();
            if *target == Some(commit.agent.state) {
                target.take();
                true
            } else {
                false
            }
        };
        if should_block {
            self.blocked.notify_one();
            self.release.notified().await;
        }
        self.inner.commit(commit, actor).await
    }

    async fn find_agent(&self, id: uuid::Uuid) -> AgentResult<Option<Agent>> {
        self.inner.find_agent(id).await
    }

    async fn list_agents(&self) -> AgentResult<Vec<Agent>> {
        self.inner.list_agents().await
    }

    async fn list_states(&self, id: uuid::Uuid) -> AgentResult<Vec<AgentStateRecord>> {
        self.inner.list_states(id).await
    }

    async fn save_profile(&self, value: &AgentProfile, actor: &str) -> AgentResult<()> {
        self.inner.save_profile(value, actor).await
    }

    async fn find_profile(&self, id: uuid::Uuid) -> AgentResult<Option<AgentProfile>> {
        self.inner.find_profile(id).await
    }

    async fn list_profiles(&self) -> AgentResult<Vec<AgentProfile>> {
        self.inner.list_profiles().await
    }

    async fn save_policy(&self, value: &AgentPolicyDefinition, actor: &str) -> AgentResult<()> {
        self.inner.save_policy(value, actor).await
    }

    async fn find_policy(&self, id: uuid::Uuid) -> AgentResult<Option<AgentPolicyDefinition>> {
        self.inner.find_policy(id).await
    }

    async fn list_policies(&self) -> AgentResult<Vec<AgentPolicyDefinition>> {
        self.inner.list_policies().await
    }

    async fn save_snapshot(&self, value: &AgentSnapshot, actor: &str) -> AgentResult<()> {
        self.inner.save_snapshot(value, actor).await
    }

    async fn find_snapshot(&self, id: uuid::Uuid) -> AgentResult<Option<AgentSnapshot>> {
        self.inner.find_snapshot(id).await
    }

    async fn list_snapshots(&self, id: uuid::Uuid) -> AgentResult<Vec<AgentSnapshot>> {
        self.inner.list_snapshots(id).await
    }
}

#[derive(Default)]
struct RecordingAgentObserver {
    values: Mutex<Vec<AgentObservation>>,
}

impl AgentObserver for RecordingAgentObserver {
    fn on_observation(&self, observation: &AgentObservation) {
        self.values.lock().unwrap().push(observation.clone());
    }
}

struct FailingPlanBuilder;

#[async_trait]
impl PlanBuilder for FailingPlanBuilder {
    fn key(&self) -> &str {
        "fail"
    }

    async fn build(
        &self,
        _goal: &core_agent_plan::Goal,
        _context: &PlanningContext,
    ) -> core_agent_plan::PlanResult<PlanDraft> {
        Err(PlanError::Validation("planned failure".into()))
    }
}

struct BlockingFailingPlanBuilder {
    started: Notify,
    release: Notify,
}

struct UnicodeFailingPlanBuilder;

#[async_trait]
impl PlanBuilder for UnicodeFailingPlanBuilder {
    fn key(&self) -> &str {
        "unicode-fail"
    }

    async fn build(
        &self,
        _goal: &core_agent_plan::Goal,
        _context: &PlanningContext,
    ) -> core_agent_plan::PlanResult<PlanDraft> {
        Err(PlanError::Validation("错误".repeat(600)))
    }
}

#[async_trait]
impl PlanBuilder for BlockingFailingPlanBuilder {
    fn key(&self) -> &str {
        "blocking-fail"
    }

    async fn build(
        &self,
        _goal: &core_agent_plan::Goal,
        _context: &PlanningContext,
    ) -> core_agent_plan::PlanResult<PlanDraft> {
        self.started.notify_one();
        self.release.notified().await;
        Err(PlanError::Validation("planned failure".into()))
    }
}

struct OperatorOnlyStopPolicy;

impl AgentPolicy for OperatorOnlyStopPolicy {
    fn evaluate(&self, operation: AgentOperation, _agent: &Agent, actor: &str) -> AgentResult<()> {
        if operation == AgentOperation::Stop && actor != "operator" {
            Err(AgentError::PolicyDenied(format!(
                "{actor} cannot stop Agent"
            )))
        } else {
            Ok(())
        }
    }
}

struct BlockingStopPolicy {
    entered: Notify,
    release: AtomicBool,
}

impl AgentPolicy for BlockingStopPolicy {
    fn evaluate(&self, operation: AgentOperation, _agent: &Agent, _actor: &str) -> AgentResult<()> {
        if operation == AgentOperation::Stop {
            self.entered.notify_one();
            while !self.release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
        Ok(())
    }
}

fn runtime(
    store: Arc<dyn core_agent_agent::AgentStore>,
    execution: Arc<ExecutionManager>,
) -> Arc<AgentManager> {
    let planning = Arc::new(PlanningManager::builder().build());
    Arc::new(
        AgentManager::builder()
            .store(store)
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
            .build(),
    )
}

async fn ready_agent(manager: &AgentManager, profile: AgentProfile) -> core_agent_agent::Agent {
    let profile = manager.register_profile(profile, "test").await.unwrap();
    let agent = manager
        .create(CreateAgentRequest::new("coding-agent", profile.id))
        .await
        .unwrap();
    manager.start(agent.id, "test").await.unwrap()
}

#[test]
fn profile_rejects_nested_secrets_and_normalizes_capabilities() {
    let capability = core_agent_agent::AgentCapability::new("Code.Write").unwrap();
    assert_eq!(capability.as_str(), "code.write");
    let mut profile = AgentProfile::new("unsafe", "Unsafe");
    profile.config = serde_json::json!({"nested": {"api_token": "hidden"}});
    assert!(matches!(profile.validate(), Err(AgentError::Validation(_))));
    profile.config = serde_json::json!({"max_tokens": 4096, "token_count": 12});
    assert!(profile.validate().is_ok());
}

#[tokio::test]
async fn one_agent_accepts_multiple_goals_then_finishes() {
    let manager = runtime(
        Arc::new(InMemoryAgentStore::default()),
        Arc::new(ExecutionManager::builder().build()),
    );
    let agent = ready_agent(&manager, AgentProfile::new("general", "General Agent")).await;

    let first = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("first", "complete first Goal"),
                PlanningContext::default(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(first.execution_status, ExecutionStatus::Completed);
    assert_eq!(first.agent.state, AgentState::Waiting);
    assert_eq!(first.agent.completed_goals, 1);
    assert!(manager.stop(agent.id, "test").await.unwrap());
    let restarted = manager.start(agent.id, "test").await.unwrap();
    assert_eq!(restarted.state, AgentState::Ready);
    assert!(restarted.current_execution_id.is_none());

    let second = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("second", "complete second Goal"),
                PlanningContext::default(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(first.reference.goal_id, second.reference.goal_id);
    assert_eq!(second.agent.completed_goals, 2);
    let completed = manager.finish(agent.id, "test").await.unwrap();
    assert_eq!(completed.state, AgentState::Completed);
    assert!(manager.list_states(agent.id).await.unwrap().len() >= 8);
}

#[tokio::test]
async fn profile_toolset_is_a_fail_closed_upper_bound() {
    let manager = runtime(
        Arc::new(InMemoryAgentStore::default()),
        Arc::new(ExecutionManager::builder().build()),
    );
    let agent = ready_agent(&manager, AgentProfile::new("restricted", "Restricted")).await;
    let mut context = PlanningContext::default();
    context.tools.push(ToolReference {
        key: "builtin/write@1".into(),
        name: "Write".into(),
        capabilities: vec!["write".into()],
    });
    let error = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(CreateGoalRequest::new("write", "write output"), context),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AgentError::PolicyDenied(_)));
    let failed = manager.find(agent.id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert_eq!(failed.last_error_kind.as_deref(), Some("POLICY_DENIED"));
}

#[tokio::test]
async fn embedded_policy_denies_without_changing_catalog_profile() {
    let manager = AgentManager::builder().build();
    let mut policy = AgentPolicyDefinition::new("no-run", "No Run");
    policy
        .rules
        .insert(AgentOperation::Run, AgentPolicyDecision::Deny);
    manager
        .register_policy(policy.clone(), "test")
        .await
        .unwrap();
    let mut profile = AgentProfile::new("guarded", "Guarded");
    profile.policy_id = Some(policy.id);
    let profile = manager.register_profile(profile, "test").await.unwrap();
    let agent = manager
        .create(CreateAgentRequest::new("guarded-agent", profile.id))
        .await
        .unwrap();
    manager.start(agent.id, "test").await.unwrap();
    let error = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("blocked", "must not run"),
                PlanningContext::default(),
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AgentError::PolicyDenied(_)));
}

#[tokio::test]
async fn custom_policy_receives_the_requested_operation_actor() {
    let manager = AgentManager::builder()
        .policy(Arc::new(OperatorOnlyStopPolicy))
        .build();
    let agent = ready_agent(&manager, AgentProfile::new("actor-policy", "Actor Policy")).await;
    assert!(matches!(
        manager.stop(agent.id, "guest").await,
        Err(AgentError::PolicyDenied(_))
    ));
    assert_eq!(
        manager.find(agent.id).await.unwrap().unwrap().state,
        AgentState::Ready
    );
    assert!(manager.stop(agent.id, "operator").await.unwrap());
    assert_eq!(
        manager.find(agent.id).await.unwrap().unwrap().actor,
        "operator"
    );
}

struct GateExecutor {
    started: Arc<Notify>,
    release: Arc<Notify>,
    blocked_once: AtomicBool,
}

struct BlockingCoordinator {
    inner: RuntimeAgentCoordinator,
    planning_started: Arc<Notify>,
    release_planning: Arc<Notify>,
    runs: Arc<AtomicUsize>,
}

#[async_trait]
impl AgentCoordinator for BlockingCoordinator {
    async fn next(
        &self,
        agent: &Agent,
        request: AgentGoalRequest,
    ) -> AgentResult<AgentRunReference> {
        self.planning_started.notify_one();
        self.release_planning.notified().await;
        self.inner.next(agent, request).await
    }

    async fn run(
        &self,
        reference: &AgentRunReference,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        self.inner.run(reference, control).await
    }

    async fn resume(
        &self,
        execution_id: uuid::Uuid,
        actor: &str,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        self.inner.resume(execution_id, actor, control).await
    }

    async fn pause(&self, execution_id: uuid::Uuid) -> AgentResult<Execution> {
        self.inner.pause(execution_id).await
    }

    async fn find_execution(&self, execution_id: uuid::Uuid) -> AgentResult<Option<Execution>> {
        self.inner.find_execution(execution_id).await
    }
}

#[derive(Default)]
struct CountingExecutor {
    calls: AtomicUsize,
}

struct DenyPause;

impl ExecutionPolicy for DenyPause {
    fn evaluate(
        &self,
        operation: ExecutionOperation,
        _plan: &core_agent_plan::Plan,
        _execution: &Execution,
        _command: Option<&ExecutionCommand>,
    ) -> core_agent_execution::ExecutionResult<()> {
        if operation == ExecutionOperation::Pause {
            Err(core_agent_execution::ExecutionError::PolicyDenied(
                "Pause denied".into(),
            ))
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl ActionExecutor for CountingExecutor {
    async fn execute(
        &self,
        command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(CommandResult::acknowledged(command.action_kind))
    }
}

#[async_trait]
impl ActionExecutor for GateExecutor {
    async fn execute(
        &self,
        _command: &ExecutionCommand,
        _control: &ExecutionControl,
    ) -> Result<CommandResult, CommandFailure> {
        if !self.blocked_once.swap(true, Ordering::AcqRel) {
            self.started.notify_one();
            self.release.notified().await;
        }
        Ok(CommandResult {
            summary: "released".into(),
            duration_ms: 0,
            output_bytes: 0,
        })
    }
}

#[tokio::test]
async fn live_stop_pauses_execution_at_a_safe_boundary_and_start_resumes() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: Arc::clone(&started),
                release: Arc::clone(&release),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = runtime(Arc::new(InMemoryAgentStore::default()), execution);
    let agent = ready_agent(&manager, AgentProfile::new("pausable", "Pausable")).await;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent.id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("pause", "pause safely"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    started.notified().await;
    assert!(manager.stop(agent.id, "operator").await.unwrap());
    release.notify_waiters();
    let paused = running.await.unwrap().unwrap();
    assert_eq!(paused.execution_status, ExecutionStatus::Paused);
    assert_eq!(paused.agent.state, AgentState::Paused);
    assert_eq!(paused.agent.actor, "operator");
    assert_eq!(
        manager
            .list_states(agent.id)
            .await
            .unwrap()
            .last()
            .unwrap()
            .actor,
        "operator"
    );

    let resumed = manager.start(agent.id, "operator").await.unwrap();
    assert_eq!(resumed.state, AgentState::Waiting);
    assert_eq!(resumed.completed_goals, 1);
}

#[tokio::test]
async fn resume_outcome_commit_failure_moves_the_agent_to_failed() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let store = Arc::new(BlockingStateStore::default());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: Arc::clone(&started),
                release: Arc::clone(&release),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = runtime(store.clone(), execution);
    let agent = ready_agent(&manager, AgentProfile::new("resume-fail", "Resume Fail")).await;
    let agent_id = agent.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("pause", "resume after pause"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    started.notified().await;
    assert!(manager.stop(agent_id, "operator").await.unwrap());
    release.notify_one();
    assert_eq!(
        running.await.unwrap().unwrap().agent.state,
        AgentState::Paused
    );

    store.fail_next(AgentState::Waiting);
    assert!(matches!(
        manager.start(agent_id, "resumer").await,
        Err(AgentError::Internal(_))
    ));
    let failed = manager.find(agent_id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert_eq!(failed.completed_goals, 0);
    assert_eq!(failed.failed_goals, 1);
    assert!(failed.current_execution_id.is_some());
}

#[tokio::test]
async fn stop_requested_during_resume_commit_is_not_lost() {
    let command_started = Arc::new(Notify::new());
    let release_command = Arc::new(Notify::new());
    let store = Arc::new(BlockingStateStore::default());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: command_started.clone(),
                release: release_command.clone(),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = runtime(store.clone(), execution);
    let agent = ready_agent(&manager, AgentProfile::new("resume-stop", "Resume Stop")).await;
    let agent_id = agent.id;
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("pause", "reach Paused first"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    command_started.notified().await;
    manager.stop(agent_id, "first-stop").await.unwrap();
    release_command.notify_one();
    assert_eq!(
        running.await.unwrap().unwrap().agent.state,
        AgentState::Paused
    );

    store.block_next(AgentState::Running);
    let resuming = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.start(agent_id, "resume").await })
    };
    store.wait_blocked().await;
    assert!(manager.stop(agent_id, "second-stop").await.unwrap());
    store.release();
    let still_paused = resuming.await.unwrap().unwrap();
    assert_eq!(still_paused.state, AgentState::Paused);
    assert_eq!(still_paused.actor, "second-stop");
    assert_eq!(still_paused.completed_goals, 0);
}

#[tokio::test]
async fn failed_pause_commit_during_resume_never_leaves_agent_running() {
    let command_started = Arc::new(Notify::new());
    let release_command = Arc::new(Notify::new());
    let store = Arc::new(BlockingStateStore::default());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: command_started.clone(),
                release: release_command.clone(),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = runtime(store.clone(), execution);
    let agent = ready_agent(
        &manager,
        AgentProfile::new("resume-pause-fail", "Resume Pause Fail"),
    )
    .await;
    let agent_id = agent.id;
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("pause", "reach Paused first"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    command_started.notified().await;
    manager.stop(agent_id, "first-stop").await.unwrap();
    release_command.notify_one();
    assert_eq!(
        running.await.unwrap().unwrap().agent.state,
        AgentState::Paused
    );

    store.block_next(AgentState::Running);
    let resuming = {
        let manager = manager.clone();
        tokio::spawn(async move { manager.start(agent_id, "resume").await })
    };
    store.wait_blocked().await;
    assert!(manager.stop(agent_id, "second-stop").await.unwrap());
    store.fail_next(AgentState::Paused);
    store.release();
    assert!(matches!(
        resuming.await.unwrap(),
        Err(AgentError::Internal(_))
    ));
    let failed = manager.find(agent_id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert!(failed.current_execution_id.is_some());
}

#[tokio::test]
async fn late_failed_stop_does_not_reassign_a_completed_goal() {
    let store = Arc::new(BlockingStateStore::default());
    let manager = runtime(store.clone(), Arc::new(ExecutionManager::builder().build()));
    let agent = ready_agent(&manager, AgentProfile::new("late-stop", "Late Stop")).await;
    let agent_id = agent.id;
    store.block_next(AgentState::Waiting);
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("complete", "finish before late stop"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    store.wait_blocked().await;
    assert!(manager.stop(agent_id, "late-operator").await.is_err());
    store.release();
    let completed = running.await.unwrap().unwrap();
    assert_eq!(completed.agent.state, AgentState::Waiting);
    assert_eq!(completed.agent.actor, "system");
    assert_eq!(
        manager
            .list_states(agent_id)
            .await
            .unwrap()
            .last()
            .unwrap()
            .actor,
        "system"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_live_stop_cannot_report_success_after_goal_completion() {
    let command_started = Arc::new(Notify::new());
    let release_command = Arc::new(Notify::new());
    let policy = Arc::new(BlockingStopPolicy {
        entered: Notify::new(),
        release: AtomicBool::new(false),
    });
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: Arc::clone(&command_started),
                release: Arc::clone(&release_command),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let planning = Arc::new(PlanningManager::builder().build());
    let manager = Arc::new(
        AgentManager::builder()
            .policy(policy.clone())
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("stale-stop", "Stale Stop")).await;
    let agent_id = agent.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("complete", "complete before stale stop"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    command_started.notified().await;
    let stopping = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.stop(agent_id, "operator").await })
    };
    policy.entered.notified().await;
    release_command.notify_one();
    let completed = running.await.unwrap().unwrap();
    assert_eq!(completed.agent.state, AgentState::Waiting);
    policy.release.store(true, Ordering::Release);

    assert!(matches!(
        stopping.await.unwrap(),
        Err(AgentError::Conflict(_))
    ));
    assert_eq!(
        manager.find(agent_id).await.unwrap().unwrap().state,
        AgentState::Waiting
    );
}

#[tokio::test]
async fn conflicting_run_never_replaces_the_live_stop_control_during_planning() {
    let planning_started = Arc::new(Notify::new());
    let release_planning = Arc::new(Notify::new());
    let runs = Arc::new(AtomicUsize::new(0));
    let executor = Arc::new(CountingExecutor::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(executor.clone())
            .build(),
    );
    let coordinator = BlockingCoordinator {
        inner: RuntimeAgentCoordinator::new(planning, execution),
        planning_started: Arc::clone(&planning_started),
        release_planning: Arc::clone(&release_planning),
        runs: Arc::clone(&runs),
    };
    let manager = Arc::new(
        AgentManager::builder()
            .coordinator(Arc::new(coordinator))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("exclusive", "Exclusive")).await;
    let first = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent.id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("first", "hold planning"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    let conflict = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("second", "must conflict"),
                PlanningContext::default(),
            ),
        )
        .await;
    assert!(matches!(conflict, Err(AgentError::Conflict(_))));
    assert!(matches!(
        manager.reconcile(agent.id, "recovery").await,
        Err(AgentError::Conflict(_))
    ));
    assert!(matches!(
        manager.finish(agent.id, "operator").await,
        Err(AgentError::Conflict(_))
    ));
    assert_eq!(
        manager.find(agent.id).await.unwrap().unwrap().state,
        AgentState::Running
    );
    assert!(manager.stop(agent.id, "operator").await.unwrap());
    release_planning.notify_one();
    let paused = first.await.unwrap().unwrap();
    assert_eq!(paused.execution_status, ExecutionStatus::Paused);
    assert_eq!(runs.load(Ordering::SeqCst), 0);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn denied_lower_pause_never_leaves_the_agent_running() {
    let planning_started = Arc::new(Notify::new());
    let release_planning = Arc::new(Notify::new());
    let executor = Arc::new(CountingExecutor::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(
        ExecutionManager::builder()
            .policy(Arc::new(DenyPause))
            .executor(executor.clone())
            .build(),
    );
    let manager = Arc::new(
        AgentManager::builder()
            .coordinator(Arc::new(BlockingCoordinator {
                inner: RuntimeAgentCoordinator::new(planning, execution),
                planning_started: Arc::clone(&planning_started),
                release_planning: Arc::clone(&release_planning),
                runs: Arc::new(AtomicUsize::new(0)),
            }))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("deny-pause", "Deny Pause")).await;
    let agent_id = agent.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("stop", "deny lower pause"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    assert!(manager.stop(agent_id, "operator").await.unwrap());
    release_planning.notify_one();

    assert!(matches!(
        running.await.unwrap(),
        Err(AgentError::Execution(
            core_agent_execution::ExecutionError::PolicyDenied(_)
        ))
    ));
    let failed = manager.find(agent_id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert!(failed.current_goal_id.is_some());
    assert!(failed.current_plan_id.is_some());
    assert!(failed.current_execution_id.is_some());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn denied_live_pause_does_not_set_the_shared_execution_control() {
    let command_started = Arc::new(Notify::new());
    let release_command = Arc::new(Notify::new());
    let execution = Arc::new(
        ExecutionManager::builder()
            .policy(Arc::new(DenyPause))
            .executor(Arc::new(GateExecutor {
                started: Arc::clone(&command_started),
                release: Arc::clone(&release_command),
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = runtime(Arc::new(InMemoryAgentStore::default()), execution);
    let agent = ready_agent(
        &manager,
        AgentProfile::new("live-deny-pause", "Live Deny Pause"),
    )
    .await;
    let agent_id = agent.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("complete", "deny live pause"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    command_started.notified().await;
    assert!(matches!(
        manager.stop(agent_id, "operator").await,
        Err(AgentError::Execution(
            core_agent_execution::ExecutionError::PolicyDenied(_)
        ))
    ));
    release_command.notify_one();

    let completed = running.await.unwrap().unwrap();
    assert_eq!(completed.agent.state, AgentState::Waiting);
    assert_eq!(completed.execution_status, ExecutionStatus::Completed);
}

#[tokio::test]
async fn a_new_goal_clears_previous_references_before_planning() {
    let planning_started = Arc::new(Notify::new());
    let release_planning = Arc::new(Notify::new());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(ExecutionManager::builder().build());
    let manager = Arc::new(
        AgentManager::builder()
            .coordinator(Arc::new(BlockingCoordinator {
                inner: RuntimeAgentCoordinator::new(planning, execution),
                planning_started: Arc::clone(&planning_started),
                release_planning: Arc::clone(&release_planning),
                runs: Arc::new(AtomicUsize::new(0)),
            }))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("sequential", "Sequential")).await;
    let agent_id = agent.id;

    let first = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("first", "complete first"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    release_planning.notify_one();
    let first = first.await.unwrap().unwrap();
    let previous_execution_id = first.reference.execution_id;
    assert_eq!(first.agent.completed_goals, 1);

    let second = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("second", "pause while planning"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    let planning = manager.find(agent_id).await.unwrap().unwrap();
    assert_eq!(planning.state, AgentState::Running);
    assert_eq!(planning.completed_goals, 1);
    assert!(planning.current_goal_id.is_none());
    assert!(planning.current_plan_id.is_none());
    assert!(planning.current_execution_id.is_none());
    assert!(matches!(
        manager.reconcile(agent_id, "recovery").await,
        Err(AgentError::Conflict(_))
    ));
    assert!(manager.stop(agent_id, "operator").await.unwrap());
    release_planning.notify_one();
    let paused = second.await.unwrap().unwrap();
    assert_eq!(paused.execution_status, ExecutionStatus::Paused);
    assert_eq!(paused.agent.completed_goals, 1);
    assert_ne!(paused.reference.execution_id, previous_execution_id);
}

#[tokio::test]
async fn interrupted_second_goal_planning_never_recounts_the_first_goal() {
    let planning_started = Arc::new(Notify::new());
    let release_planning = Arc::new(Notify::new());
    let store = Arc::new(InMemoryAgentStore::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(ExecutionManager::builder().build());
    let coordinator = Arc::new(BlockingCoordinator {
        inner: RuntimeAgentCoordinator::new(Arc::clone(&planning), Arc::clone(&execution)),
        planning_started: Arc::clone(&planning_started),
        release_planning: Arc::clone(&release_planning),
        runs: Arc::new(AtomicUsize::new(0)),
    });
    let manager = Arc::new(
        AgentManager::builder()
            .store(store.clone())
            .coordinator(coordinator)
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("recovery", "Recovery")).await;
    let agent_id = agent.id;

    let first = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("first", "complete first"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    release_planning.notify_one();
    assert_eq!(first.await.unwrap().unwrap().agent.completed_goals, 1);

    let second = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("second", "interrupt planning"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    second.abort();
    let _ = second.await;

    let recovered = AgentManager::builder()
        .store(store)
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
        .build()
        .reconcile(agent_id, "recovery")
        .await
        .unwrap();
    assert_eq!(recovered.state, AgentState::Failed);
    assert_eq!(recovered.completed_goals, 1);
    assert_eq!(
        recovered.last_error_kind.as_deref(),
        Some("PLANNING_INTERRUPTED")
    );
}

#[tokio::test]
async fn stop_conflicts_with_non_stoppable_lifecycle_operations() {
    let store = Arc::new(BlockingStateStore::default());
    let manager = Arc::new(AgentManager::builder().store(store.clone()).build());

    let finish_agent = ready_agent(&manager, AgentProfile::new("finish-race", "Finish Race")).await;
    store.block_next(AgentState::Completed);
    let finishing = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.finish(finish_agent.id, "finisher").await })
    };
    store.wait_blocked().await;
    assert!(matches!(
        manager.stop(finish_agent.id, "operator").await,
        Err(AgentError::Conflict(_))
    ));
    store.release();
    assert_eq!(
        finishing.await.unwrap().unwrap().state,
        AgentState::Completed
    );

    let destroy_agent =
        ready_agent(&manager, AgentProfile::new("destroy-race", "Destroy Race")).await;
    store.block_next(AgentState::Destroyed);
    let destroying = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.destroy(destroy_agent.id, "destroyer").await })
    };
    store.wait_blocked().await;
    assert!(matches!(
        manager.stop(destroy_agent.id, "operator").await,
        Err(AgentError::Conflict(_))
    ));
    store.release();
    assert_eq!(
        destroying.await.unwrap().unwrap().state,
        AgentState::Destroyed
    );
}

#[tokio::test]
async fn failed_agent_reference_commit_keeps_all_lower_runtime_ids() {
    let planning_started = Arc::new(Notify::new());
    let release_planning = Arc::new(Notify::new());
    let store = Arc::new(BlockingStateStore::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(ExecutionManager::builder().build());
    let coordinator = BlockingCoordinator {
        inner: RuntimeAgentCoordinator::new(planning, execution.clone()),
        planning_started: planning_started.clone(),
        release_planning: release_planning.clone(),
        runs: Arc::new(AtomicUsize::new(0)),
    };
    let manager = Arc::new(
        AgentManager::builder()
            .store(store.clone())
            .coordinator(Arc::new(coordinator))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("commit-fail", "Commit Fail")).await;
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent.id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("commit", "fail Agent reference commit"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    planning_started.notified().await;
    store.fail_next(AgentState::Running);
    release_planning.notify_one();
    let error = running.await.unwrap().unwrap_err();
    let (_, _, execution_id) = error.partial_reference().unwrap();
    let failed = manager.find(agent.id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert!(failed.current_goal_id.is_some());
    assert!(failed.current_plan_id.is_some());
    assert_eq!(failed.current_execution_id, execution_id);
    assert_eq!(
        execution
            .find(execution_id.unwrap())
            .await
            .unwrap()
            .unwrap()
            .status,
        ExecutionStatus::Paused
    );
}

#[tokio::test]
async fn cold_reconcile_finishes_an_execution_completed_before_agent_commit() {
    let store = Arc::new(BlockingStateStore::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(ExecutionManager::builder().build());
    let manager = Arc::new(
        AgentManager::builder()
            .store(store.clone())
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(
                planning.clone(),
                execution.clone(),
            )))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("recover", "Recover")).await;
    store.block_next(AgentState::Waiting);
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent.id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("recover", "complete before commit"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    store.wait_blocked().await;
    running.abort();
    let _ = running.await;
    let orphan = store.find_agent(agent.id).await.unwrap().unwrap();
    assert_eq!(orphan.state, AgentState::Running);
    assert!(orphan.current_execution_id.is_some());

    let recovered_manager = AgentManager::builder()
        .store(store)
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
        .build();
    let recovered = recovered_manager
        .reconcile(agent.id, "recovery")
        .await
        .unwrap();
    assert_eq!(recovered.state, AgentState::Waiting);
    assert_eq!(recovered.completed_goals, 1);
}

#[tokio::test]
async fn cold_reconcile_marks_in_flight_outcome_as_unknown() {
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let store = Arc::new(InMemoryAgentStore::default());
    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(GateExecutor {
                started: started.clone(),
                release,
                blocked_once: AtomicBool::new(false),
            }))
            .build(),
    );
    let manager = Arc::new(
        AgentManager::builder()
            .store(store.clone())
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(
                planning.clone(),
                execution.clone(),
            )))
            .build(),
    );
    let agent = ready_agent(&manager, AgentProfile::new("uncertain", "Uncertain")).await;
    let running = {
        let manager = manager.clone();
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent.id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("uncertain", "lose command owner"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    started.notified().await;
    running.abort();
    let _ = running.await;

    let recovered_manager = AgentManager::builder()
        .store(store)
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
        .build();
    let recovered = recovered_manager
        .reconcile(agent.id, "recovery")
        .await
        .unwrap();
    assert_eq!(recovered.state, AgentState::Failed);
    assert_eq!(
        recovered.last_error_kind.as_deref(),
        Some("OUTCOME_UNKNOWN")
    );
}

#[tokio::test]
async fn planning_failure_keeps_partial_goal_reference_and_failure_observation() {
    let planning = Arc::new(
        PlanningManager::builder()
            .builder(Arc::new(FailingPlanBuilder))
            .build(),
    );
    let observer = Arc::new(RecordingAgentObserver::default());
    let manager = AgentManager::builder()
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(
            planning.clone(),
            Arc::new(ExecutionManager::builder().build()),
        )))
        .observer(observer.clone())
        .build();
    let mut profile = AgentProfile::new("fails-plan", "Fails Plan");
    profile.planner_key = Some("fail".into());
    let agent = ready_agent(&manager, profile).await;
    let error = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("partial", "retain the Goal"),
                PlanningContext::default(),
            ),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, AgentError::PartialCoordination { .. }));
    let failed = manager.find(agent.id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert!(failed.current_goal_id.is_some());
    assert!(failed.current_plan_id.is_none());
    assert_eq!(planning.list_goals().await.unwrap().len(), 1);
    assert!(observer.values.lock().unwrap().iter().any(|value| {
        value.stage == AgentStage::Planning && !value.success && value.goal_id.is_some()
    }));
}

#[tokio::test]
async fn unicode_failure_message_is_truncated_to_the_durable_byte_limit() {
    let planning = Arc::new(
        PlanningManager::builder()
            .builder(Arc::new(UnicodeFailingPlanBuilder))
            .build(),
    );
    let manager = AgentManager::builder()
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(
            planning,
            Arc::new(ExecutionManager::builder().build()),
        )))
        .build();
    let mut profile = AgentProfile::new("unicode-failure", "Unicode Failure");
    profile.planner_key = Some("unicode-fail".into());
    let agent = ready_agent(&manager, profile).await;

    assert!(matches!(
        manager
            .run_goal(
                agent.id,
                AgentGoalRequest::new(
                    CreateGoalRequest::new("unicode", "return a long unicode error"),
                    PlanningContext::default(),
                ),
            )
            .await,
        Err(AgentError::PartialCoordination { .. })
    ));
    let failed = manager.find(agent.id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    let message = failed.last_error_message.unwrap();
    assert!(message.len() <= 1024);
    assert!(message.is_char_boundary(message.len()));
}

#[tokio::test]
async fn partial_reference_survives_a_transient_agent_commit_failure() {
    let store = Arc::new(BlockingStateStore::default());
    let builder = Arc::new(BlockingFailingPlanBuilder {
        started: Notify::new(),
        release: Notify::new(),
    });
    let planning = Arc::new(PlanningManager::builder().builder(builder.clone()).build());
    let manager = Arc::new(
        AgentManager::builder()
            .store(store.clone())
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(
                planning.clone(),
                Arc::new(ExecutionManager::builder().build()),
            )))
            .build(),
    );
    let mut profile = AgentProfile::new("partial-commit", "Partial Commit");
    profile.planner_key = Some("blocking-fail".into());
    let agent = ready_agent(&manager, profile).await;
    let agent_id = agent.id;
    let running = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move {
            manager
                .run_goal(
                    agent_id,
                    AgentGoalRequest::new(
                        CreateGoalRequest::new("partial", "retain durable Goal id"),
                        PlanningContext::default(),
                    ),
                )
                .await
        })
    };
    builder.started.notified().await;
    store.fail_next(AgentState::Running);
    builder.release.notify_one();

    assert!(matches!(
        running.await.unwrap(),
        Err(AgentError::Internal(_))
    ));
    let failed = manager.find(agent_id).await.unwrap().unwrap();
    assert_eq!(failed.state, AgentState::Failed);
    assert!(failed.current_goal_id.is_some());
    assert!(failed.current_plan_id.is_none());
    assert!(failed.current_execution_id.is_none());
    assert_eq!(planning.list_goals().await.unwrap().len(), 1);
}

#[tokio::test]
async fn snapshot_restore_rejects_replay_after_version_moves() {
    let observer = Arc::new(RecordingAgentObserver::default());
    let manager = AgentManager::builder().observer(observer.clone()).build();
    let agent = ready_agent(&manager, AgentProfile::new("snapshot", "Snapshot")).await;
    let snapshot = manager
        .save_snapshot(agent.id, "ready boundary", "test")
        .await
        .unwrap();
    assert!(matches!(
        manager
            .save_snapshot(agent.id, "duplicate boundary", "test")
            .await,
        Err(AgentError::Conflict(_))
    ));
    let restored = manager.restore_snapshot(snapshot.id, "test").await.unwrap();
    assert_eq!(restored.state, AgentState::Ready);
    assert_eq!(restored.version, snapshot.agent_version + 1);
    assert!(observer.values.lock().unwrap().iter().any(|value| {
        value.operation == AgentOperation::Restore
            && value.stage == AgentStage::Snapshot
            && value.actor == "test"
    }));
    assert!(matches!(
        manager.restore_snapshot(snapshot.id, "test").await,
        Err(AgentError::Conflict(_))
    ));
}

#[tokio::test]
async fn sqlite_round_trip_has_five_audited_tables_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("agent.db");
    let store = Arc::new(SqliteAgentStore::new(&path).unwrap());
    let manager = AgentManager::builder().store(store).build();
    let agent = ready_agent(&manager, AgentProfile::new("sqlite", "SQLite")).await;
    let mut profile = manager.list_profiles().await.unwrap().remove(0);
    profile.version += 1;
    profile.name = "SQLite Updated".into();
    profile.updated_at = Utc::now();
    manager
        .register_profile(profile.clone(), "test")
        .await
        .unwrap();
    assert_eq!(
        manager
            .find_profile(profile.id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "SQLite Updated"
    );
    let mut invalid_profile = profile;
    invalid_profile.version += 1;
    invalid_profile.created_at += ChronoDuration::seconds(1);
    invalid_profile.updated_at = Utc::now() + ChronoDuration::seconds(2);
    assert!(matches!(
        manager.register_profile(invalid_profile, "test").await,
        Err(AgentError::Validation(_))
    ));

    let policy = manager
        .register_policy(
            AgentPolicyDefinition::new("sqlite-policy", "SQLite Policy"),
            "test",
        )
        .await
        .unwrap();
    let mut updated_policy = policy.clone();
    updated_policy.version += 1;
    updated_policy.name = "SQLite Policy Updated".into();
    updated_policy.updated_at = Utc::now();
    manager
        .register_policy(updated_policy.clone(), "test")
        .await
        .unwrap();
    let mut invalid_policy = updated_policy;
    invalid_policy.version += 1;
    invalid_policy.key = "changed-key".into();
    invalid_policy.updated_at = Utc::now();
    assert!(matches!(
        manager.register_policy(invalid_policy, "test").await,
        Err(AgentError::Validation(_))
    ));
    let snapshot = manager
        .save_snapshot(agent.id, "ready", "test")
        .await
        .unwrap();
    assert_eq!(
        manager
            .list_snapshots(agent.id)
            .await
            .unwrap()
            .first()
            .unwrap()
            .hash,
        snapshot.hash
    );

    let connection = Connection::open(&path).unwrap();
    for table in [
        "agent",
        "agent_profile",
        "agent_snapshot",
        "agent_state",
        "agent_policy",
    ] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for audit in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(
                columns.iter().any(|value| value == audit),
                "{table}.{audit}"
            );
        }
        let foreign_keys: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0, "{table}");
    }
    connection
        .execute(
            "UPDATE agent SET state='FAILED' WHERE id=?1",
            [agent.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        manager.find(agent.id).await,
        Err(AgentError::Validation(_))
    ));
}
