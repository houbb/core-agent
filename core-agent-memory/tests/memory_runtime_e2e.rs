use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use core_agent_memory::{
    normalize_tag, DefaultMemoryIndexer, DefaultMemoryLifecycle, InMemoryMemoryStore, Memory,
    MemoryContent, MemoryError, MemoryEvent, MemoryEventKind, MemoryImportance, MemoryIndexEntry,
    MemoryIndexer, MemoryInterceptor, MemoryLifecycle, MemoryManager, MemoryObservation,
    MemoryObserver, MemoryPolicyDefinition, MemoryQuery, MemorySourceKind, MemoryState,
    MemoryStore, MemoryType, MemoryUpdate, SqliteMemoryStore,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn event(
    namespace: &str,
    title: &str,
    body: &str,
    kind: MemoryEventKind,
    source: MemorySourceKind,
) -> MemoryEvent {
    let mut event = MemoryEvent::new(namespace, source, MemoryContent::new(title, body));
    event.kind = kind;
    event.actor = "tester".into();
    event
}

async fn stored(manager: &MemoryManager, event: MemoryEvent) -> Memory {
    manager.remember(event).await.unwrap().memory.unwrap()
}

#[derive(Default)]
struct RecordingObserver {
    values: Mutex<Vec<MemoryObservation>>,
}

impl MemoryObserver for RecordingObserver {
    fn on_observation(&self, observation: &MemoryObservation) {
        self.values.lock().unwrap().push(observation.clone());
    }
}

#[test]
fn tag_and_content_validation_are_normalized_and_secret_safe() {
    assert_eq!(normalize_tag(" Spring ").unwrap(), "spring");
    let mut event = event(
        "team-a",
        "unsafe",
        "body",
        MemoryEventKind::Knowledge,
        MemorySourceKind::User,
    );
    event.content.data = serde_json::json!({"nested": {"api_token": "secret"}});
    assert!(matches!(event.validate(), Err(MemoryError::Validation(_))));
}

#[tokio::test]
async fn classifier_skips_conversation_noise_but_keeps_useful_outcomes() {
    let manager = MemoryManager::builder().build();
    let skipped = manager
        .remember(event(
            "team-a",
            "chat",
            "temporary conversation",
            MemoryEventKind::Observation,
            MemorySourceKind::Conversation,
        ))
        .await
        .unwrap();
    assert!(skipped.memory.is_none());

    let remembered = stored(
        &manager,
        event(
            "team-a",
            "Fixed null pointer",
            "Added an explicit null guard",
            MemoryEventKind::Outcome,
            MemorySourceKind::Execution,
        ),
    )
    .await;
    assert_eq!(remembered.state, MemoryState::Indexed);
    assert_eq!(remembered.memory_type, MemoryType::Experience);
    assert_eq!(manager.list("team-a").await.unwrap().len(), 1);
}

#[tokio::test]
async fn remember_is_idempotent_and_recall_is_ranked_and_namespace_isolated() {
    let observer = Arc::new(RecordingObserver::default());
    let manager = MemoryManager::builder().observer(observer.clone()).build();
    let mut high = event(
        "team-a",
        "Spring null pointer fix",
        "Use an Optional boundary",
        MemoryEventKind::Knowledge,
        MemorySourceKind::Execution,
    );
    high.suggested_importance = Some(MemoryImportance::High);
    high.tags.insert("spring".into());
    let duplicate = high.clone();
    let first = stored(&manager, high).await;
    let second = stored(&manager, duplicate).await;
    assert_eq!(first.id, second.id);

    let mut low = event(
        "team-a",
        "Spring observation",
        "A framework log was observed",
        MemoryEventKind::Observation,
        MemorySourceKind::Tool,
    );
    low.tags.insert("spring".into());
    stored(&manager, low).await;
    stored(
        &manager,
        event(
            "team-b",
            "Spring private memory",
            "must not cross namespace",
            MemoryEventKind::Knowledge,
            MemorySourceKind::User,
        ),
    )
    .await;

    let mut query = MemoryQuery::new("team-a");
    query.text = Some("spring".into());
    query.tags.insert("spring".into());
    query.actor = "recaller".into();
    let hits = manager.recall(query).await.unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].memory.id, first.id);
    assert!(hits.iter().all(|hit| hit.memory.namespace == "team-a"));
    assert!(hits
        .iter()
        .all(|hit| hit.memory.state == MemoryState::Recalled));
    assert!(hits.iter().all(|hit| hit.memory.recall_count == 1));
    assert!(observer
        .values
        .lock()
        .unwrap()
        .iter()
        .any(|value| value.actor == "recaller"));
}

#[tokio::test]
async fn update_archive_snapshot_restore_and_forget_form_one_safe_lifecycle() {
    let manager = MemoryManager::builder().build();
    let memory = stored(
        &manager,
        event(
            "team-a",
            "Original rule",
            "Prefer explicit errors",
            MemoryEventKind::Knowledge,
            MemorySourceKind::User,
        ),
    )
    .await;
    let snapshot = manager
        .save_snapshot(memory.id, "before edit", "tester")
        .await
        .unwrap();
    let mut update = MemoryUpdate::new(memory.version, "editor");
    update.content = Some(MemoryContent::new("Changed rule", "Prefer typed errors"));
    update.tags = Some(BTreeSet::from(["rust".into()]));
    let updated = manager.update(memory.id, update).await.unwrap();
    assert_eq!(updated.state, MemoryState::Updated);
    assert_eq!(updated.content.title, "Changed rule");

    let restored = manager
        .restore_snapshot(snapshot.id, updated.version, "restorer")
        .await
        .unwrap();
    assert_eq!(restored.content.title, "Original rule");
    let archived = manager
        .archive(restored.id, restored.version, "archiver")
        .await
        .unwrap();
    assert_eq!(archived.state, MemoryState::Archived);
    assert!(manager
        .recall(MemoryQuery::new("team-a"))
        .await
        .unwrap()
        .is_empty());

    let forgotten = manager
        .forget(archived.id, archived.version, "privacy")
        .await
        .unwrap();
    assert_eq!(forgotten.state, MemoryState::Forgotten);
    assert_eq!(forgotten.content, MemoryContent::forgotten());
    assert!(manager
        .list_snapshots(forgotten.id)
        .await
        .unwrap()
        .is_empty());
    let mut archived_query = MemoryQuery::new("team-a");
    archived_query.include_archived = true;
    assert!(manager.recall(archived_query).await.unwrap().is_empty());
    assert!(matches!(
        manager
            .restore_snapshot(snapshot.id, forgotten.version, "restorer")
            .await,
        Err(MemoryError::NotFound(_))
    ));
}

#[tokio::test]
async fn stale_updates_conflict_without_losing_the_first_change() {
    let manager = MemoryManager::builder().build();
    let memory = stored(
        &manager,
        event(
            "team-a",
            "Fact",
            "first",
            MemoryEventKind::Fact,
            MemorySourceKind::User,
        ),
    )
    .await;
    let mut first = MemoryUpdate::new(memory.version, "first-writer");
    first.content = Some(MemoryContent::new("Fact", "winner"));
    manager.update(memory.id, first).await.unwrap();
    let mut stale = MemoryUpdate::new(memory.version, "stale-writer");
    stale.content = Some(MemoryContent::new("Fact", "loser"));
    assert!(matches!(
        manager.update(memory.id, stale).await,
        Err(MemoryError::Conflict(_))
    ));
    assert_eq!(
        manager.find(memory.id).await.unwrap().unwrap().content.body,
        "winner"
    );
}

#[tokio::test]
async fn policy_denies_sensitive_events_and_applies_versioned_retention() {
    let manager = MemoryManager::builder().build();
    let mut sensitive = event(
        "team-a",
        "Credential",
        "opaque sensitive value",
        MemoryEventKind::Fact,
        MemorySourceKind::User,
    );
    sensitive.sensitive = true;
    assert!(matches!(
        manager.remember(sensitive).await,
        Err(MemoryError::PolicyDenied(_))
    ));

    let mut policy = MemoryPolicyDefinition::new("short-retention", "Short Retention");
    policy.temporary_retention_days = 1;
    let policy = manager.register_policy(policy, "admin").await.unwrap();
    let mut temporary = event(
        "team-a",
        "Temporary observation",
        "retain for one day",
        MemoryEventKind::Observation,
        MemorySourceKind::Tool,
    );
    temporary.policy_id = Some(policy.id);
    temporary.suggested_importance = Some(MemoryImportance::Temporary);
    let occurred_at = temporary.occurred_at;
    let memory = stored(&manager, temporary).await;
    assert_eq!(memory.expires_at, Some(occurred_at + Duration::days(1)));
}

struct RedirectNamespace;

impl MemoryInterceptor for RedirectNamespace {
    fn before_remember(&self, event: &mut MemoryEvent) -> core_agent_memory::MemoryResult<()> {
        event.namespace = "other-team".into();
        Ok(())
    }
}

struct PanickingObserver;

impl MemoryObserver for PanickingObserver {
    fn on_observation(&self, _observation: &MemoryObservation) {
        panic!("observer failure")
    }
}

struct RedirectingLifecycle;

impl MemoryLifecycle for RedirectingLifecycle {
    fn transition(
        &self,
        memory: &mut Memory,
        next: MemoryState,
        actor: &str,
        reason: &str,
    ) -> core_agent_memory::MemoryResult<()> {
        DefaultMemoryLifecycle.transition(memory, next, actor, reason)?;
        memory.namespace = "other-team".into();
        Ok(())
    }
}

struct PrefixedIndexer;

impl MemoryIndexer for PrefixedIndexer {
    fn index(&self, memory: &Memory) -> core_agent_memory::MemoryResult<MemoryIndexEntry> {
        let mut index = DefaultMemoryIndexer.index(memory)?;
        index.normalized_text = format!("custom:{}", index.normalized_text);
        index.validate_for(memory)?;
        Ok(index)
    }
}

#[tokio::test]
async fn extensions_cannot_redirect_identity_and_observer_panics_are_isolated() {
    let redirected = MemoryManager::builder()
        .interceptor(Arc::new(RedirectNamespace))
        .build();
    assert!(matches!(
        redirected
            .remember(event(
                "team-a",
                "Fact",
                "body",
                MemoryEventKind::Fact,
                MemorySourceKind::User,
            ))
            .await,
        Err(MemoryError::Validation(_))
    ));

    let redirected = MemoryManager::builder()
        .lifecycle(Arc::new(RedirectingLifecycle))
        .build();
    assert!(matches!(
        redirected
            .remember(event(
                "team-a",
                "Fact",
                "body",
                MemoryEventKind::Fact,
                MemorySourceKind::User,
            ))
            .await,
        Err(MemoryError::Validation(_))
    ));

    let manager = MemoryManager::builder()
        .observer(Arc::new(PanickingObserver))
        .build();
    assert!(stored(
        &manager,
        event(
            "team-a",
            "Fact",
            "observer cannot hide success",
            MemoryEventKind::Fact,
            MemorySourceKind::User,
        ),
    )
    .await
    .state
    .is_recallable());
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_index_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.db");
    let store = Arc::new(SqliteMemoryStore::new(&path).unwrap());
    let manager = MemoryManager::builder()
        .store(store.clone())
        .indexer(Arc::new(PrefixedIndexer))
        .build();
    let mut value = event(
        "team-a",
        "SQLite knowledge",
        "survives reopen",
        MemoryEventKind::Knowledge,
        MemorySourceKind::Workspace,
    );
    value.tags.insert("sqlite".into());
    let memory = stored(&manager, value).await;
    manager
        .save_snapshot(memory.id, "durable", "tester")
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened = Arc::new(SqliteMemoryStore::new(&path).unwrap());
    let recovered = reopened.find_memory(memory.id).await.unwrap().unwrap();
    assert_eq!(recovered.content.title, "SQLite knowledge");
    let connection = Connection::open(&path).unwrap();
    let tables = [
        "memory",
        "memory_index",
        "memory_snapshot",
        "memory_policy",
        "memory_tag",
    ];
    for table in tables {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        for audit in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.contains(audit), "{table} is missing {audit}");
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
            "UPDATE memory_index SET memory_type = 'FACT' WHERE memory_id = ?1",
            [memory.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        reopened.find_memory(memory.id).await,
        Err(MemoryError::Validation(_))
    ));
}

#[tokio::test]
async fn sqlite_forget_atomically_purges_index_tags_and_snapshots() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("forget.db");
    let store = Arc::new(SqliteMemoryStore::new(&path).unwrap());
    let manager = MemoryManager::new(store);
    let mut value = event(
        "team-a",
        "Forget me",
        "private knowledge",
        MemoryEventKind::Knowledge,
        MemorySourceKind::User,
    );
    value.tags.insert("private".into());
    let memory = stored(&manager, value).await;
    manager
        .save_snapshot(memory.id, "private", "tester")
        .await
        .unwrap();
    manager
        .forget(memory.id, memory.version, "privacy")
        .await
        .unwrap();

    let connection = Connection::open(path).unwrap();
    for table in ["memory_index", "memory_tag", "memory_snapshot"] {
        let count: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE memory_id = ?1"),
                [memory.id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "{table} retained forgotten content");
    }
}

#[tokio::test]
async fn expired_memory_is_not_recalled() {
    let store = Arc::new(InMemoryMemoryStore::default());
    let manager = MemoryManager::new(store);
    let mut old = event(
        "team-a",
        "Expired",
        "old temporary memory",
        MemoryEventKind::Observation,
        MemorySourceKind::Tool,
    );
    old.suggested_importance = Some(MemoryImportance::Temporary);
    old.occurred_at = Utc::now() - Duration::days(8);
    stored(&manager, old).await;
    assert!(manager
        .recall(MemoryQuery::new("team-a"))
        .await
        .unwrap()
        .is_empty());
}
