use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use core_agent_tool::{
    FixedToolPermission, FunctionTool, PermissionDecision, RawToolOutput, SqliteToolStore,
    StaticToolProvider, ToolCapability, ToolCatalog, ToolDefinition, ToolError,
    ToolExecutionRecord, ToolInterceptor, ToolLifecycle, ToolLifecycleStatus, ToolManager,
    ToolObservation, ToolObserver, ToolPermissionStore, ToolProviderDefinition, ToolProviderKind,
    ToolRegistration, ToolRequest, ToolRuntimeResult, ToolStage,
};
use tempfile::tempdir;
use tokio::sync::Notify;

fn echo_definition(permission: PermissionDecision) -> ToolDefinition {
    let mut definition = ToolDefinition::new(
        "builtin",
        "echo",
        "1.0.0",
        serde_json::json!({
            "type":"object",
            "required":["text"],
            "properties":{"text":{"type":"string","minLength":1}},
            "additionalProperties":false
        }),
    );
    definition.description = "Returns supplied text".into();
    definition.category = "utility".into();
    definition
        .capabilities
        .insert(ToolCapability::new("utility.echo").unwrap());
    definition.default_permission = permission;
    definition
}

fn echo_provider(
    permission: PermissionDecision,
    calls: Arc<AtomicUsize>,
) -> (StaticToolProvider, String) {
    let definition = echo_definition(permission);
    let key = definition.key.clone();
    let tool = FunctionTool::new(key.clone(), move |request, _context| {
        let calls = Arc::clone(&calls);
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(RawToolOutput::text(
                request.parameters["text"].as_str().unwrap_or_default(),
            ))
        }
    });
    let provider = StaticToolProvider::new(
        ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin),
        vec![ToolRegistration::new(definition, Arc::new(tool))],
    );
    (provider, key)
}

#[tokio::test]
async fn provider_discovery_execution_and_sqlite_audit_form_one_flow() {
    let store = Arc::new(SqliteToolStore::new(":memory:").unwrap());
    let manager = ToolManager::builder()
        .catalog(store.clone())
        .permission(store.clone())
        .lifecycle(store.clone())
        .build();
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, key) = echo_provider(PermissionDecision::Ask, Arc::clone(&calls));
    assert_eq!(manager.load_provider(&provider).await.unwrap(), 1);
    let mut rule = core_agent_tool::ToolPermissionRule::for_tool(&key, PermissionDecision::Allow);
    rule.priority = 10;
    store.upsert_permission(&rule).await.unwrap();

    let mut request = ToolRequest::new(&key, serde_json::json!({"text":"hello"}));
    request.metadata.insert("trace_id".into(), "trace-1".into());
    request
        .metadata
        .insert("custom".into(), "not-persisted".into());
    let request_id = request.id;
    let result = manager.execute(request).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(manager.list().await.unwrap().len(), 1);
    assert_eq!(
        manager
            .find_by_capability(&ToolCapability::new("utility").unwrap(), true)
            .await
            .unwrap()
            .len(),
        1
    );
    let audit = store.find_execution(request_id).await.unwrap().unwrap();
    assert_eq!(audit.status, ToolLifecycleStatus::Success);
    assert_eq!(audit.metadata.len(), 1);
    assert_eq!(audit.metadata.get("trace_id").unwrap(), "trace-1");
}

#[tokio::test]
async fn default_ask_and_explicit_deny_never_invoke_tool() {
    for decision in [PermissionDecision::Ask, PermissionDecision::Deny] {
        let calls = Arc::new(AtomicUsize::new(0));
        let (provider, key) = echo_provider(PermissionDecision::Ask, Arc::clone(&calls));
        let manager = ToolManager::builder()
            .permission(Arc::new(FixedToolPermission(decision)))
            .build();
        manager.load_provider(&provider).await.unwrap();
        let error = manager
            .execute(ToolRequest::new(&key, serde_json::json!({"text":"x"})))
            .await
            .unwrap_err();
        assert!(matches!(
            (decision, error),
            (PermissionDecision::Ask, ToolError::ApprovalRequired(_))
                | (PermissionDecision::Deny, ToolError::PermissionDenied(_))
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

struct RemoveTextInterceptor;

#[async_trait]
impl ToolInterceptor for RemoveTextInterceptor {
    async fn intercept_request(&self, request: &mut ToolRequest) -> ToolRuntimeResult<()> {
        request.parameters = serde_json::json!({});
        Ok(())
    }
}

#[tokio::test]
async fn interceptor_mutations_are_revalidated_before_permission_and_execution() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, key) = echo_provider(PermissionDecision::Allow, Arc::clone(&calls));
    let manager = ToolManager::builder()
        .interceptor(Arc::new(RemoveTextInterceptor))
        .build();
    manager.load_provider(&provider).await.unwrap();
    let error = manager
        .execute(ToolRequest::new(&key, serde_json::json!({"text":"x"})))
        .await
        .unwrap_err();
    assert!(matches!(error, ToolError::Validation(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn timeout_and_tool_failure_return_uniform_terminal_results() {
    let mut slow_definition =
        ToolDefinition::new("builtin", "slow", "1", serde_json::json!({"type":"object"}));
    slow_definition.default_permission = PermissionDecision::Allow;
    slow_definition.timeout_ms = 10;
    let slow_key = slow_definition.key.clone();
    let slow = FunctionTool::new(slow_key.clone(), |_request, _context| async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(RawToolOutput::text("late"))
    });

    let mut fail_definition =
        ToolDefinition::new("builtin", "fail", "1", serde_json::json!({"type":"object"}));
    fail_definition.default_permission = PermissionDecision::Allow;
    let fail_key = fail_definition.key.clone();
    let fail = FunctionTool::new(fail_key.clone(), move |_request, _context| {
        let key = fail_key.clone();
        async move { Err(ToolError::execution(key, "expected failure", false)) }
    });
    let provider = StaticToolProvider::new(
        ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin),
        vec![
            ToolRegistration::new(slow_definition, Arc::new(slow)),
            ToolRegistration::new(fail_definition, Arc::new(fail)),
        ],
    );
    let manager = ToolManager::builder().build();
    manager.load_provider(&provider).await.unwrap();

    let timeout = manager
        .execute(ToolRequest::new(&slow_key, serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(timeout.status, ToolLifecycleStatus::Failed);
    assert_eq!(timeout.error.unwrap().kind, "TIMEOUT");
    let failure = manager
        .execute(ToolRequest::new("builtin/fail@1", serde_json::json!({})))
        .await
        .unwrap();
    assert_eq!(failure.status, ToolLifecycleStatus::Failed);
    assert_eq!(failure.error.unwrap().kind, "EXECUTION");
}

#[tokio::test]
async fn active_execution_can_be_cancelled_and_observes_one_cancelled_terminal() {
    let started = Arc::new(Notify::new());
    let mut definition =
        ToolDefinition::new("builtin", "wait", "1", serde_json::json!({"type":"object"}));
    definition.default_permission = PermissionDecision::Allow;
    let key = definition.key.clone();
    let tool = FunctionTool::new(key.clone(), {
        let started = Arc::clone(&started);
        move |_request, context| {
            let started = Arc::clone(&started);
            async move {
                started.notify_one();
                context.cancellation.cancelled().await;
                Err(ToolError::Cancelled(context.request_id.to_string()))
            }
        }
    });
    let events = Arc::new(CollectingObserver::default());
    let manager = Arc::new(ToolManager::builder().observer(events.clone()).build());
    manager
        .load_provider(&StaticToolProvider::new(
            ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin),
            vec![ToolRegistration::new(definition, Arc::new(tool))],
        ))
        .await
        .unwrap();
    let request = ToolRequest::new(&key, serde_json::json!({}));
    let request_id = request.id;
    let executing = {
        let manager = Arc::clone(&manager);
        tokio::spawn(async move { manager.execute(request).await.unwrap() })
    };
    started.notified().await;
    assert!(manager.cancel(request_id).unwrap());
    let result = executing.await.unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Cancelled);
    assert!(!manager.cancel(request_id).unwrap());
    let terminal = events
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| {
            matches!(
                event.stage,
                ToolStage::Success | ToolStage::Failed | ToolStage::Cancelled
            )
        })
        .count();
    assert_eq!(terminal, 1);
}

#[derive(Default)]
struct CollectingObserver {
    events: Mutex<Vec<ToolObservation>>,
}

impl ToolObserver for CollectingObserver {
    fn on_observation(&self, observation: &ToolObservation) {
        self.events.lock().unwrap().push(observation.clone());
    }
}

struct PanicObserver;

impl ToolObserver for PanicObserver {
    fn on_observation(&self, _observation: &ToolObservation) {
        panic!("observer panic must be isolated");
    }
}

#[tokio::test]
async fn observer_panic_never_changes_tool_result() {
    let (provider, key) = echo_provider(PermissionDecision::Allow, Arc::new(AtomicUsize::new(0)));
    let manager = ToolManager::builder()
        .observer(Arc::new(PanicObserver))
        .build();
    manager.load_provider(&provider).await.unwrap();
    let result = manager
        .execute(ToolRequest::new(&key, serde_json::json!({"text":"ok"})))
        .await
        .unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
}

struct FailFinalAudit;

#[async_trait]
impl ToolLifecycle for FailFinalAudit {
    async fn transition(&self, record: &ToolExecutionRecord) -> ToolRuntimeResult<()> {
        if record.status.is_terminal() {
            Err(ToolError::Persistence("audit offline".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn final_audit_failure_does_not_hide_successful_side_effect() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, key) = echo_provider(PermissionDecision::Allow, Arc::clone(&calls));
    let manager = ToolManager::builder()
        .lifecycle(Arc::new(FailFinalAudit))
        .build();
    manager.load_provider(&provider).await.unwrap();
    let result = manager
        .execute(ToolRequest::new(&key, serde_json::json!({"text":"ok"})))
        .await
        .unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        result.metadata.get("core_agent.execution_audit"),
        Some(&"FAILED".to_owned())
    );
}

struct FailInitialAudit;

#[async_trait]
impl ToolLifecycle for FailInitialAudit {
    async fn transition(&self, _record: &ToolExecutionRecord) -> ToolRuntimeResult<()> {
        Err(ToolError::Persistence("audit offline".into()))
    }
}

#[tokio::test]
async fn initial_audit_failure_prevents_side_effect() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, key) = echo_provider(PermissionDecision::Allow, Arc::clone(&calls));
    let manager = ToolManager::builder()
        .lifecycle(Arc::new(FailInitialAudit))
        .build();
    manager.load_provider(&provider).await.unwrap();
    let error = manager
        .execute(ToolRequest::new(&key, serde_json::json!({"text":"no"})))
        .await
        .unwrap_err();
    assert!(matches!(error, ToolError::Persistence(_)));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn durable_lifecycle_rejects_duplicate_request_id_without_reexecution() {
    let store = Arc::new(SqliteToolStore::new(":memory:").unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let (provider, key) = echo_provider(PermissionDecision::Allow, Arc::clone(&calls));
    let manager = ToolManager::builder()
        .catalog(store.clone())
        .permission(store.clone())
        .lifecycle(store.clone())
        .build();
    manager.load_provider(&provider).await.unwrap();
    let request = ToolRequest::new(&key, serde_json::json!({"text":"once"}));
    let request_id = request.id;
    assert_eq!(
        manager.execute(request.clone()).await.unwrap().status,
        ToolLifecycleStatus::Success
    );
    assert!(matches!(
        manager.execute(request).await.unwrap_err(),
        ToolError::Persistence(_)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        store
            .find_execution(request_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        ToolLifecycleStatus::Success
    );
}

#[tokio::test]
async fn file_database_recovers_catalog_permissions_and_execution_audit() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("tool.db");
    let path_string = path.to_string_lossy().into_owned();
    let request_id;
    {
        let store = Arc::new(SqliteToolStore::new(&path_string).unwrap());
        let manager = ToolManager::builder()
            .catalog(store.clone())
            .permission(store.clone())
            .lifecycle(store.clone())
            .build();
        let (provider, key) =
            echo_provider(PermissionDecision::Allow, Arc::new(AtomicUsize::new(0)));
        manager.load_provider(&provider).await.unwrap();
        let request = ToolRequest::new(&key, serde_json::json!({"text":"persist"}));
        request_id = request.id;
        manager.execute(request).await.unwrap();
    }
    let reopened = SqliteToolStore::new(&path_string).unwrap();
    assert_eq!(reopened.list_tools().await.unwrap().len(), 1);
    assert_eq!(
        reopened
            .find_execution(request_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        ToolLifecycleStatus::Success
    );

    let connection = rusqlite::Connection::open(&path).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(tool_execution)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!columns
        .iter()
        .any(|column| column == "parameters" || column == "content"));
}
