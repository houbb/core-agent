use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use core_agent_workflow::{
    InMemoryWorkflowStore, SqliteWorkflowStore, StartWorkflowRequest, WorkflowAction,
    WorkflowActionContext, WorkflowActionOutcome, WorkflowActionResult, WorkflowActivity,
    WorkflowBinding, WorkflowControl, WorkflowDefinition, WorkflowEngine, WorkflowError,
    WorkflowInstance, WorkflowInstanceCommit, WorkflowInterceptor, WorkflowManager,
    WorkflowObserver, WorkflowResult, WorkflowStageDefinition, WorkflowState, WorkflowStateRecord,
    WorkflowStore,
};
use rusqlite::Connection;
use tempfile::tempdir;
use tokio::sync::Notify;
use uuid::Uuid;

fn definition() -> WorkflowDefinition {
    WorkflowDefinition::new(
        "rca",
        "RCA",
        vec![
            WorkflowStageDefinition::new(
                "collect",
                "Collect",
                vec![WorkflowActivity::new(
                    "collect-data",
                    "Collect data",
                    vec![
                        WorkflowAction::new("logs", "Read logs", "execution.plan"),
                        WorkflowAction::new("metrics", "Read metrics", "execution.plan"),
                    ],
                )],
            ),
            WorkflowStageDefinition::new(
                "report",
                "Report",
                vec![WorkflowActivity::new(
                    "write-report",
                    "Write report",
                    vec![WorkflowAction::new(
                        "report-action",
                        "Generate report",
                        "execution.plan",
                    )],
                )],
            ),
        ],
        "designer",
    )
    .unwrap()
}

fn completed(summary: impl Into<String>) -> WorkflowActionOutcome {
    WorkflowActionOutcome::Completed(WorkflowActionResult {
        summary: summary.into(),
        external_state: "COMPLETED".into(),
        completed_at: Utc::now(),
    })
}

struct ScriptEngine {
    prepares: AtomicUsize,
    executions: AtomicUsize,
    order: Mutex<Vec<String>>,
    outcomes: Mutex<VecDeque<WorkflowActionOutcome>>,
}

impl ScriptEngine {
    fn new(outcomes: Vec<WorkflowActionOutcome>) -> Self {
        Self {
            prepares: AtomicUsize::new(0),
            executions: AtomicUsize::new(0),
            order: Mutex::new(Vec::new()),
            outcomes: Mutex::new(outcomes.into()),
        }
    }
}

#[async_trait]
impl WorkflowEngine for ScriptEngine {
    async fn prepare(
        &self,
        _action: &WorkflowAction,
        context: &WorkflowActionContext,
    ) -> WorkflowResult<WorkflowBinding> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        Ok(WorkflowBinding {
            dispatch_id: context.dispatch_id,
            external_id: Uuid::new_v5(&context.dispatch_id, b"external"),
            external_kind: "test-execution".into(),
            prepared_at: Utc::now(),
        })
    }

    async fn execute(
        &self,
        _binding: &WorkflowBinding,
        action: &WorkflowAction,
        _context: &WorkflowActionContext,
        _control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowActionOutcome> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        self.order.lock().unwrap().push(action.key.clone());
        Ok(self
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| completed(format!("{} completed", action.key))))
    }

    async fn cancel(&self, _binding: &WorkflowBinding, _actor: &str) -> WorkflowResult<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn sequential_workflow_runs_in_business_order_and_records_timeline() {
    let engine = Arc::new(ScriptEngine::new(Vec::new()));
    let manager = WorkflowManager::builder().engine(engine.clone()).build();
    let definition = manager.register(definition()).await.unwrap();
    let instance = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    assert_eq!(instance.state, WorkflowState::Completed);
    assert_eq!(
        *engine.order.lock().unwrap(),
        vec!["logs", "metrics", "report-action"]
    );
    assert_eq!(engine.prepares.load(Ordering::SeqCst), 3);
    let states = manager.list_states(instance.id).await.unwrap();
    assert_eq!(states.first().unwrap().to_state, WorkflowState::Created);
    assert_eq!(states.last().unwrap().to_state, WorkflowState::Completed);
    assert_eq!(definition.action_count(), 3);
}

#[tokio::test]
async fn definition_versions_are_immutable_and_instances_pin_the_requested_version() {
    let engine = Arc::new(ScriptEngine::new(Vec::new()));
    let manager = WorkflowManager::builder().engine(engine).build();
    let first = manager.register(definition()).await.unwrap();
    let mut second = first.revise(first.stages.clone(), "designer-v2").unwrap();
    second.name = "RCA v2".into();
    let second = manager.register(second).await.unwrap();
    assert_eq!(
        manager
            .list_definitions(first.workflow_id)
            .await
            .unwrap()
            .len(),
        2
    );

    let mut request = StartWorkflowRequest::new("rca", "operator");
    request.definition_version = Some(1);
    let instance = manager.start(request).await.unwrap();
    assert_eq!(instance.definition_id, first.id);
    assert_eq!(instance.definition_version, 1);
    assert_ne!(instance.definition_id, second.id);
}

#[tokio::test]
async fn waiting_resume_reuses_the_same_binding_without_preparing_again() {
    let engine = Arc::new(ScriptEngine::new(vec![
        WorkflowActionOutcome::Waiting("waiting for external completion".into()),
        completed("external completion observed"),
    ]));
    let manager = WorkflowManager::builder().engine(engine.clone()).build();
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    manager.register(one).await.unwrap();
    let waiting = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    assert_eq!(waiting.state, WorkflowState::Waiting);
    let binding = waiting
        .action_progress()
        .next()
        .unwrap()
        .binding
        .clone()
        .unwrap();

    let completed = manager.resume(waiting.id, "resumer").await.unwrap();
    assert_eq!(completed.state, WorkflowState::Completed);
    assert_eq!(engine.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(engine.executions.load(Ordering::SeqCst), 2);
    assert_eq!(
        completed
            .action_progress()
            .next()
            .unwrap()
            .binding
            .as_ref()
            .unwrap(),
        &binding
    );
}

#[tokio::test]
async fn action_failure_stops_later_business_steps() {
    let engine = Arc::new(ScriptEngine::new(vec![WorkflowActionOutcome::Failed(
        "analysis failed".into(),
    )]));
    let manager = WorkflowManager::builder().engine(engine.clone()).build();
    manager.register(definition()).await.unwrap();
    let instance = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    assert_eq!(instance.state, WorkflowState::Failed);
    assert_eq!(engine.executions.load(Ordering::SeqCst), 1);
    assert_eq!(*engine.order.lock().unwrap(), vec!["logs"]);
}

struct GateEngine {
    calls: AtomicUsize,
    started: Notify,
}

#[async_trait]
impl WorkflowEngine for GateEngine {
    async fn prepare(
        &self,
        _action: &WorkflowAction,
        context: &WorkflowActionContext,
    ) -> WorkflowResult<WorkflowBinding> {
        Ok(WorkflowBinding {
            dispatch_id: context.dispatch_id,
            external_id: Uuid::new_v4(),
            external_kind: "gate".into(),
            prepared_at: Utc::now(),
        })
    }

    async fn execute(
        &self,
        _binding: &WorkflowBinding,
        _action: &WorkflowAction,
        _context: &WorkflowActionContext,
        control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowActionOutcome> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call > 0 {
            return Ok(completed("resumed"));
        }
        self.started.notify_waiters();
        tokio::select! {
            _ = control.pause_requested() => Ok(WorkflowActionOutcome::Paused("paused".into())),
            _ = control.cancelled() => Ok(WorkflowActionOutcome::Cancelled("cancelled".into())),
        }
    }

    async fn cancel(&self, _binding: &WorkflowBinding, _actor: &str) -> WorkflowResult<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn live_pause_reaches_execution_boundary_then_resume_finishes() {
    let engine = Arc::new(GateEngine {
        calls: AtomicUsize::new(0),
        started: Notify::new(),
    });
    let manager = Arc::new(WorkflowManager::builder().engine(engine.clone()).build());
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    let workflow_id = manager.register(one).await.unwrap().workflow_id;
    let started = engine.started.notified();
    let running_manager = manager.clone();
    let run = tokio::spawn(async move {
        running_manager
            .start(StartWorkflowRequest::new("rca", "operator"))
            .await
    });
    started.await;
    let id = manager.list_instances(workflow_id).await.unwrap()[0].id;
    assert!(manager.pause(id, "pauser").await.unwrap());
    let paused = run.await.unwrap().unwrap();
    assert_eq!(paused.state, WorkflowState::Paused);
    assert_eq!(paused.actor, "pauser");

    let completed = manager.resume(id, "resumer").await.unwrap();
    assert_eq!(completed.state, WorkflowState::Completed);
}

#[tokio::test]
async fn live_cancel_reaches_engine_and_persists_operator_actor() {
    let engine = Arc::new(GateEngine {
        calls: AtomicUsize::new(0),
        started: Notify::new(),
    });
    let manager = Arc::new(WorkflowManager::builder().engine(engine.clone()).build());
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    let workflow_id = manager.register(one).await.unwrap().workflow_id;
    let started = engine.started.notified();
    let running_manager = manager.clone();
    let run = tokio::spawn(async move {
        running_manager
            .start(StartWorkflowRequest::new("rca", "operator"))
            .await
    });
    started.await;
    let id = manager.list_instances(workflow_id).await.unwrap()[0].id;
    assert!(manager.cancel(id, "canceller").await.unwrap());
    let cancelled = run.await.unwrap().unwrap();
    assert_eq!(cancelled.state, WorkflowState::Cancelled);
    assert_eq!(cancelled.actor, "canceller");
}

struct UncertainOnceEngine {
    prepares: AtomicUsize,
    calls: AtomicUsize,
}

#[async_trait]
impl WorkflowEngine for UncertainOnceEngine {
    async fn prepare(
        &self,
        _action: &WorkflowAction,
        context: &WorkflowActionContext,
    ) -> WorkflowResult<WorkflowBinding> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        Ok(WorkflowBinding {
            dispatch_id: context.dispatch_id,
            external_id: Uuid::new_v5(&context.dispatch_id, b"uncertain"),
            external_kind: "uncertain".into(),
            prepared_at: Utc::now(),
        })
    }

    async fn execute(
        &self,
        _binding: &WorkflowBinding,
        _action: &WorkflowAction,
        _context: &WorkflowActionContext,
        _control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowActionOutcome> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(WorkflowError::OutcomeUnknown(
                "external result was not committed locally".into(),
            ))
        } else {
            Ok(completed("reconciled"))
        }
    }

    async fn cancel(&self, _binding: &WorkflowBinding, _actor: &str) -> WorkflowResult<bool> {
        Err(WorkflowError::OutcomeUnknown(
            "cancellation is not terminal".into(),
        ))
    }
}

#[tokio::test]
async fn outcome_unknown_stays_running_and_resume_reuses_binding() {
    let engine = Arc::new(UncertainOnceEngine {
        prepares: AtomicUsize::new(0),
        calls: AtomicUsize::new(0),
    });
    let manager = WorkflowManager::builder().engine(engine.clone()).build();
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    let workflow_id = manager.register(one).await.unwrap().workflow_id;
    assert!(matches!(
        manager
            .start(StartWorkflowRequest::new("rca", "operator"))
            .await,
        Err(WorkflowError::OutcomeUnknown(_))
    ));
    let running = manager.list_instances(workflow_id).await.unwrap()[0].clone();
    assert_eq!(running.state, WorkflowState::Running);
    let binding = running.action_progress().next().unwrap().binding.clone();

    let completed = manager.resume(running.id, "reconciler").await.unwrap();
    assert_eq!(completed.state, WorkflowState::Completed);
    assert_eq!(engine.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(completed.action_progress().next().unwrap().binding, binding);
}

#[tokio::test]
async fn current_safe_snapshot_restores_once_and_rejects_stale_replay() {
    let engine = Arc::new(ScriptEngine::new(vec![WorkflowActionOutcome::Waiting(
        "wait".into(),
    )]));
    let manager = WorkflowManager::builder().engine(engine).build();
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    manager.register(one).await.unwrap();
    let waiting = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    let snapshot = manager
        .snapshot(waiting.id, "waiting boundary", "operator")
        .await
        .unwrap();
    let restored = manager.restore(snapshot.id, "restorer").await.unwrap();
    assert_eq!(restored.version, waiting.version + 1);
    assert!(matches!(
        manager.restore(snapshot.id, "restorer").await,
        Err(WorkflowError::Conflict(_))
    ));
}

struct PanickingObserver;

impl WorkflowObserver for PanickingObserver {
    fn on_observation(&self, _value: &core_agent_workflow::WorkflowObservation) {
        panic!("observer panic")
    }
}

struct PanickingInterceptor;

impl WorkflowInterceptor for PanickingInterceptor {
    fn before_start(
        &self,
        _definition: &WorkflowDefinition,
        _variables: &mut core_agent_workflow::WorkflowVariables,
    ) -> WorkflowResult<()> {
        panic!("interceptor panic")
    }
}

#[tokio::test]
async fn observer_panics_do_not_change_workflow_outcome() {
    let manager = WorkflowManager::builder()
        .engine(Arc::new(ScriptEngine::new(Vec::new())))
        .observer(Arc::new(PanickingObserver))
        .build();
    manager.register(definition()).await.unwrap();
    let instance = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    assert_eq!(instance.state, WorkflowState::Completed);
}

#[tokio::test]
async fn interceptor_panic_fails_before_instance_persistence() {
    let manager = WorkflowManager::builder()
        .engine(Arc::new(ScriptEngine::new(Vec::new())))
        .interceptor(Arc::new(PanickingInterceptor))
        .build();
    let definition = manager.register(definition()).await.unwrap();
    assert!(matches!(
        manager
            .start(StartWorkflowRequest::new("rca", "operator"))
            .await,
        Err(WorkflowError::Extension(_))
    ));
    assert!(manager
        .list_instances(definition.workflow_id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn stale_instance_commit_conflicts_without_losing_winner() {
    let store = Arc::new(InMemoryWorkflowStore::default());
    let engine = Arc::new(ScriptEngine::new(vec![WorkflowActionOutcome::Waiting(
        "wait".into(),
    )]));
    let manager = WorkflowManager::builder()
        .store(store.clone())
        .engine(engine)
        .build();
    manager.register(definition()).await.unwrap();
    let waiting = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    let expected = waiting.version;
    let mut winner = waiting.clone();
    winner.version += 1;
    winner.actor = "winner".into();
    winner.updated_at = Utc::now().max(winner.updated_at);
    let stale = winner.clone();
    store
        .commit_instance(
            &WorkflowInstanceCommit::update(winner, expected, None),
            "winner",
        )
        .await
        .unwrap();
    assert!(matches!(
        store
            .commit_instance(
                &WorkflowInstanceCommit::update(stale, expected, None),
                "stale"
            )
            .await,
        Err(WorkflowError::Conflict(_))
    ));
}

#[tokio::test]
async fn store_rejects_missing_timeline_definition_drift_and_aggregate_tampering() {
    let store = Arc::new(InMemoryWorkflowStore::default());
    let manager = WorkflowManager::builder()
        .store(store.clone())
        .engine(Arc::new(ScriptEngine::new(vec![
            WorkflowActionOutcome::Waiting("wait".into()),
        ])))
        .build();
    manager.register(definition()).await.unwrap();
    let waiting = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();

    let mut missing_timeline = waiting.clone();
    missing_timeline.state = WorkflowState::Paused;
    missing_timeline.version += 1;
    missing_timeline.actor = "tamper".into();
    missing_timeline.updated_at = Utc::now().max(missing_timeline.updated_at);
    assert!(matches!(
        store
            .commit_instance(
                &WorkflowInstanceCommit::update(missing_timeline, waiting.version, None,),
                "tamper",
            )
            .await,
        Err(WorkflowError::Conflict(_) | WorkflowError::Validation(_))
    ));

    let mut drifted_definition = waiting.clone();
    drifted_definition.definition.name = "Drifted Definition".into();
    drifted_definition.version += 1;
    drifted_definition.actor = "tamper".into();
    drifted_definition.updated_at = Utc::now().max(drifted_definition.updated_at);
    assert!(matches!(
        store
            .commit_instance(
                &WorkflowInstanceCommit::update(drifted_definition, waiting.version, None,),
                "tamper",
            )
            .await,
        Err(WorkflowError::Conflict(_) | WorkflowError::Validation(_))
    ));

    let mut aggregate_tamper = waiting.clone();
    aggregate_tamper.progress[0].state = core_agent_workflow::WorkItemState::Completed;
    aggregate_tamper.version += 1;
    aggregate_tamper.actor = "tamper".into();
    aggregate_tamper.updated_at = Utc::now().max(aggregate_tamper.updated_at);
    assert!(matches!(
        store
            .commit_instance(
                &WorkflowInstanceCommit::update(aggregate_tamper, waiting.version, None),
                "tamper",
            )
            .await,
        Err(WorkflowError::Validation(_))
    ));
}

#[tokio::test]
async fn created_instance_recovers_after_startup_crash_window() {
    let store = Arc::new(InMemoryWorkflowStore::default());
    let manager = WorkflowManager::builder()
        .store(store.clone())
        .engine(Arc::new(ScriptEngine::new(Vec::new())))
        .build();
    let registered = manager.register(definition()).await.unwrap();
    let instance = WorkflowInstance::new(registered, Default::default(), "creator").unwrap();
    let record = WorkflowStateRecord {
        id: Uuid::new_v4(),
        instance_id: instance.id,
        sequence: instance.version,
        from_state: None,
        to_state: WorkflowState::Created,
        reason: "Workflow created before process crash".into(),
        actor: "creator".into(),
        created_at: Utc::now(),
    };
    store
        .commit_instance(
            &WorkflowInstanceCommit::create(instance.clone(), record),
            "creator",
        )
        .await
        .unwrap();

    let completed = manager
        .resume(instance.id, "recovery-worker")
        .await
        .unwrap();
    assert_eq!(completed.state, WorkflowState::Completed);
    assert_eq!(
        store
            .list_states(instance.id)
            .await
            .unwrap()
            .into_iter()
            .map(|value| value.to_state)
            .collect::<Vec<_>>(),
        vec![
            WorkflowState::Created,
            WorkflowState::Scheduled,
            WorkflowState::Running,
            WorkflowState::Completed,
        ]
    );
}

#[tokio::test]
async fn cold_resume_uses_persisted_binding_without_prepare() {
    let store = Arc::new(InMemoryWorkflowStore::default());
    let first_engine = Arc::new(ScriptEngine::new(vec![WorkflowActionOutcome::Waiting(
        "wait".into(),
    )]));
    let first = WorkflowManager::builder()
        .store(store.clone())
        .engine(first_engine)
        .build();
    let mut one = definition();
    one.stages.truncate(1);
    one.stages[0].activities[0].actions.truncate(1);
    let definition = first.register(one).await.unwrap();
    let waiting = first
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    drop(first);

    let recovered_engine = Arc::new(ScriptEngine::new(vec![completed("recovered")]));
    let recovered = WorkflowManager::builder()
        .store(store)
        .engine(recovered_engine.clone())
        .build();
    recovered
        .bind_existing(definition.workflow_id, definition.version)
        .await
        .unwrap();
    let completed = recovered
        .resume(waiting.id, "recovery-worker")
        .await
        .unwrap();
    assert_eq!(completed.state, WorkflowState::Completed);
    assert_eq!(recovered_engine.prepares.load(Ordering::SeqCst), 0);
    assert_eq!(recovered_engine.executions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("workflow.db");
    let store = Arc::new(SqliteWorkflowStore::new(&path).unwrap());
    let manager = WorkflowManager::builder()
        .store(store.clone())
        .engine(Arc::new(ScriptEngine::new(Vec::new())))
        .build();
    let definition = manager.register(definition()).await.unwrap();
    let instance = manager
        .start(StartWorkflowRequest::new("rca", "operator"))
        .await
        .unwrap();
    let snapshot = manager
        .snapshot(instance.id, "completed", "operator")
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened = SqliteWorkflowStore::new(&path).unwrap();
    assert_eq!(
        reopened
            .find_instance(instance.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        WorkflowState::Completed
    );
    assert_eq!(
        reopened
            .find_definition(definition.workflow_id, 1)
            .await
            .unwrap()
            .unwrap()
            .id,
        definition.id
    );
    assert_eq!(
        reopened
            .find_snapshot(snapshot.id)
            .await
            .unwrap()
            .unwrap()
            .instance_id,
        instance.id
    );
    assert_eq!(reopened.list_states(instance.id).await.unwrap().len(), 4);
    let connection = Connection::open(&path).unwrap();
    for table in [
        "workflow",
        "workflow_definition",
        "workflow_instance",
        "workflow_snapshot",
        "workflow_state",
    ] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<std::collections::BTreeSet<_>, _>>()
            .unwrap();
        for required in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.contains(required), "{table} is missing {required}");
        }
        let foreign_keys: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0);
    }
    connection
        .execute(
            "UPDATE workflow_instance SET state = 'FAILED' WHERE id = ?1",
            [instance.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        reopened.find_instance(instance.id).await,
        Err(WorkflowError::Validation(_))
    ));
}
