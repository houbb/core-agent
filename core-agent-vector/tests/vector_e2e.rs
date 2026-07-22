use std::collections::BTreeSet;
use std::sync::Arc;

use core_agent_vector::{
    InMemoryVectorStore, SearchResult, SqliteVectorStore, VectorManager, VectorQuery, VectorRecord,
    VectorStore,
};
use rusqlite::Connection;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn insert_and_search_by_vector() {
    let manager = VectorManager::builder().build();
    manager
        .index_chunk("database connection pool", "doc1", None, None, "tester")
        .await
        .unwrap();
    manager
        .index_chunk("order service timeout", "doc2", None, None, "tester")
        .await
        .unwrap();

    let results = manager.search_similar("connection", 5).await.unwrap();
    assert!(!results.is_empty());
    assert!(results[0].score > 0.0);
}

#[tokio::test]
async fn keyword_search_via_fts() {
    let store = Arc::new(SqliteVectorStore::new(":memory:").unwrap());
    let manager = VectorManager::builder().store(store).build();
    manager
        .index_chunk("payment gateway timeout error", "doc", None, None, "tester")
        .await
        .unwrap();
    manager
        .index_chunk("database connection pool", "doc", None, None, "tester")
        .await
        .unwrap();

    let results = manager
        .search(&VectorQuery::new(Some("timeout".into()), None))
        .await
        .unwrap();
    assert!(!results.is_empty());
    assert!(results
        .iter()
        .any(|r| r.record.content.contains("timeout")));
}

#[tokio::test]
async fn hybrid_search_metadata_filter() {
    let manager = VectorManager::builder().build();
    manager
        .index_chunk("payment service", "pay", None, None, "tester")
        .await
        .unwrap();
    manager
        .index_chunk("order service", "order", None, None, "tester")
        .await
        .unwrap();

    let results = manager.search_similar("service", 5).await.unwrap();
    assert_eq!(results.len(), 2);
}

#[tokio::test]
async fn sqlite_persistence_audit_columns() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("vector.db");
    let store = Arc::new(SqliteVectorStore::new(&path).unwrap());
    let manager = VectorManager::builder().store(store.clone()).build();
    manager
        .index_chunk("persistent content", "test", None, None, "tester")
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened = Arc::new(SqliteVectorStore::new(&path).unwrap());
    let found = reopened.find_by_id(
        reopened
            .search_by_keyword("persistent", 1)
            .await
            .unwrap()
            .first()
            .unwrap()
            .record
            .id,
    )
    .await
    .unwrap();
    assert!(found.is_some());

    let connection = Connection::open(&path).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(vector_record)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<BTreeSet<_>, _>>()
        .unwrap();
    for audit in ["id", "create_time", "update_time", "create_user", "update_user"] {
        assert!(columns.contains(audit), "missing {audit}");
    }
    let foreign_keys: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('vector_record')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(foreign_keys, 0);
}

#[tokio::test]
async fn delete_removes_vector() {
    let manager = VectorManager::builder().build();
    let record = manager
        .index_chunk("delete me", "doc", None, None, "tester")
        .await
        .unwrap();
    let store: Arc<dyn VectorStore> = manager.store_ref();
    store.delete(record.id, "cleaner").await.unwrap();
    assert!(store.find_by_id(record.id).await.unwrap().is_none());
}