use std::collections::BTreeSet;
use std::sync::Arc;

use core_agent_knowledge::{
    KnowledgeItem, KnowledgeKind, KnowledgeSourceKind, KnowledgeStatus, KnowledgeStore, SqliteKnowledgeStore,
};
use rusqlite::Connection;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn knowledge_crud_works() {
    let manager = core_agent_knowledge::KnowledgeManager::builder().build();
    let item = KnowledgeItem::new(
        KnowledgeKind::Document,
        "Architecture",
        "System architecture document",
        KnowledgeSourceKind::Manual,
        "architect",
        "tester",
    );
    let created = manager.create_knowledge(&item, "tester").await.unwrap();
    assert_eq!(created.status, KnowledgeStatus::Created);

    let published = manager.publish_knowledge(created.id, "publisher").await.unwrap();
    assert_eq!(published.status, KnowledgeStatus::Published);

    let found = manager.get_knowledge(created.id).await.unwrap().unwrap();
    assert_eq!(found.title, "Architecture");
}

#[tokio::test]
async fn knowledge_search_works() {
    let manager = core_agent_knowledge::KnowledgeManager::builder().build();
    let item = KnowledgeItem::new(
        KnowledgeKind::Business,
        "Order Flow",
        "How orders are processed",
        KnowledgeSourceKind::Manual,
        "business",
        "tester",
    );
    manager.create_knowledge(&item, "tester").await.unwrap();
    let results = manager.search_knowledge("order", "default", 10).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].title.contains("Order"));
}

#[tokio::test]
async fn sqlite_persistence_audit_columns() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("knowledge.db");
    let store = Arc::new(SqliteKnowledgeStore::new(&path).unwrap());
    let manager = core_agent_knowledge::KnowledgeManager::builder()
        .store(store.clone())
        .build();
    let item = KnowledgeItem::new(
        KnowledgeKind::Document,
        "SQLite Test",
        "Persistent content",
        KnowledgeSourceKind::Manual,
        "owner",
        "tester",
    );
    manager.create_knowledge(&item, "tester").await.unwrap();
    drop(manager);
    drop(store);

    let reopened: Arc<dyn KnowledgeStore> = Arc::new(SqliteKnowledgeStore::new(&path).unwrap());
    let items = reopened.list_items().await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "SQLite Test");

    let connection = Connection::open(&path).unwrap();
    for table in ["knowledge_item", "knowledge_category"] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        for audit in ["id", "create_time", "update_time", "create_user", "update_user"] {
            assert!(columns.contains(audit), "{table} is missing {audit}");
        }
        let foreign_keys: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0, "{table} has foreign keys");
    }
}

#[tokio::test]
async fn import_from_document_works() {
    let manager = core_agent_knowledge::KnowledgeManager::builder().build();
    let item = manager
        .import_from_document(Uuid::new_v4(), "Doc Import", "Imported from doc", "tester")
        .await
        .unwrap();
    assert_eq!(item.status, KnowledgeStatus::Published);
    assert!(item.document_id.is_some());
}

#[tokio::test]
async fn categories_work() {
    let manager = core_agent_knowledge::KnowledgeManager::builder().build();
    let cat = manager.create_category("Architecture", None, "tester").await.unwrap();
    assert_eq!(cat.name, "Architecture");
    let cats = manager.get_knowledge_tree().await.unwrap();
    assert!(!cats.is_empty());
}