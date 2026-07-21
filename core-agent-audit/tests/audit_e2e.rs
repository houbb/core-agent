use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use tempfile::TempDir;
use uuid::Uuid;

use core_agent_audit::{
    AuditEvent, AuditEventType, AuditManager, AuditQuery, AuditSeverity, AuditResult, SqliteAuditStore,
};

struct CountingObserver {
    count: AtomicUsize,
}

impl core_agent_audit::AuditObserver for CountingObserver {
    fn on_audit(&self, _event: &AuditEvent) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn audit_append_only_in_memory() {
    let manager = AuditManager::builder().build();
    let tenant = Uuid::new_v4();

    let event = AuditEvent::new(tenant, "rca-agent", AuditEventType::ToolCall, "log.query", "production/logs")
        .with_severity(AuditSeverity::Info)
        .with_result("success");
    manager.record(&event, "system").await.unwrap();

    let events = manager.list(&AuditQuery {
        tenant_id: Some(tenant),
        ..Default::default()
    }).await.unwrap();
    assert_eq!(events.len(), 1);

    // Duplicate event ID is rejected (append-only enforcement)
    let dup = event.clone();
    let result = manager.record(&dup, "system").await;
    assert!(result.is_err(), "duplicate event should be rejected");

    // Query by actor
    let by_actor = manager.list(&AuditQuery {
        actor: Some("rca-agent".into()),
        limit: 100,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(by_actor.len(), 1);

    // Query by event type
    let by_type = manager.list(&AuditQuery {
        event_type: Some(AuditEventType::ToolCall),
        limit: 100,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(by_type.len(), 1);

    let by_missing = manager.list(&AuditQuery {
        event_type: Some(AuditEventType::System),
        limit: 100,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(by_missing.len(), 0);
}

#[tokio::test]
async fn audit_snapshot_aggregates_correctly() {
    let manager = AuditManager::builder().build();
    let tenant = Uuid::new_v4();

    for i in 0..5 {
        let event = AuditEvent::new(tenant, "agent", AuditEventType::ToolCall, "tool", "resource")
            .with_severity(if i % 2 == 0 { AuditSeverity::Info } else { AuditSeverity::Warning });
        manager.record(&event, "system").await.unwrap();
    }
    let event = AuditEvent::new(tenant, "agent", AuditEventType::System, "startup", "system")
        .with_severity(AuditSeverity::Info);
    manager.record(&event, "system").await.unwrap();

    let snap = manager.snapshot(tenant).await.unwrap();
    assert_eq!(snap.total_events, 6);
    assert_eq!(*snap.by_event_type.get("TOOL_CALL").unwrap(), 5);
    assert_eq!(*snap.by_event_type.get("SYSTEM").unwrap(), 1);
}

#[tokio::test]
async fn audit_observer_fires_on_record() {
    let observer = Arc::new(CountingObserver { count: AtomicUsize::new(0) });
    let manager = AuditManager::builder().observer(observer.clone()).build();
    let tenant = Uuid::new_v4();

    let event = AuditEvent::new(tenant, "agent", AuditEventType::System, "test", "test");
    manager.record(&event, "system").await.unwrap();
    assert_eq!(observer.count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn audit_sqlite_persistence_and_tamper_detection() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("audit.db");

    let store = Arc::new(SqliteAuditStore::new(&db_path).unwrap());
    let manager = AuditManager::new(store);
    let tenant = Uuid::new_v4();

    let event = AuditEvent::new(tenant, "rca-agent", AuditEventType::ToolCall, "log.query", "production/logs")
        .with_payload(serde_json::json!({"query": "SELECT * FROM logs"}));
    manager.record(&event, "system").await.unwrap();

    let found = manager.find(event.id).await.unwrap().unwrap();
    assert_eq!(found.id, event.id);
    assert_eq!(found.action, "log.query");

    // Snapshot
    let snap = manager.snapshot(tenant).await.unwrap();
    assert_eq!(snap.total_events, 1);

    // Tamper: modify the content column directly
    let tampered = serde_json::to_string(&AuditEvent::new(
        tenant, "tampered", AuditEventType::System, "tampered", "tampered"
    )).unwrap();
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute(
        "UPDATE audit_event SET content = ?1 WHERE id = ?2",
        rusqlite::params![tampered, event.id.to_string()],
    ).unwrap();

    // Reading tampered data should fail cross-validation
    let store2 = Arc::new(SqliteAuditStore::new(&db_path).unwrap());
    let manager2 = AuditManager::new(store2);
    let result = manager2.find(event.id).await;
    assert!(result.is_err(), "tampered data should be detected");
}

#[tokio::test]
async fn audit_works_without_tenant_filter() {
    let manager = AuditManager::builder().build();
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    for i in 0..3 {
        let e = AuditEvent::new(tenant_a, "agent", AuditEventType::System, "a", &format!("res{i}"));
        manager.record(&e, "system").await.unwrap();
    }
    for i in 0..2 {
        let e = AuditEvent::new(tenant_b, "agent", AuditEventType::System, "b", &format!("res{i}"));
        manager.record(&e, "system").await.unwrap();
    }

    let all = manager.list(&AuditQuery {
        limit: 100,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(all.len(), 5);

    let a_only = manager.list(&AuditQuery {
        tenant_id: Some(tenant_a),
        limit: 100,
        ..Default::default()
    }).await.unwrap();
    assert_eq!(a_only.len(), 3);
}