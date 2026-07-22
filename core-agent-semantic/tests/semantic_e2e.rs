use std::collections::BTreeSet;
use std::sync::Arc;

use core_agent_document::DocumentAST;
use core_agent_semantic::{
    Entity, EntityType, GraphStore, SqliteGraphStore,
};
use rusqlite::Connection;
use tempfile::tempdir;
use uuid::Uuid;

#[tokio::test]
async fn extract_and_query_entities() {
    let manager = core_agent_semantic::SemanticManager::builder().build();
    let mut ast = DocumentAST::new();
    ast.sections.push(core_agent_document::DocumentSection::new(
        "OrderService", 1, "OrderService depends on PaymentService",
    ));
    ast.sections.push(core_agent_document::DocumentSection::new(
        "PaymentService", 1, "PaymentService uses Database",
    ));

    let (entities, relations) = manager
        .extract_from_document(Uuid::new_v4(), &ast, "tester")
        .await
        .unwrap();
    assert!(!entities.is_empty());
    assert!(!relations.is_empty());

    // BFS query
    let order = manager.search_entities("OrderService").await.unwrap();
    assert!(!order.is_empty());
    let related = manager.find_related(order[0].id, 3).await.unwrap();
    assert!(!related.is_empty());
}

#[tokio::test]
async fn entity_relations_are_stored() {
    let manager = core_agent_semantic::SemanticManager::builder().build();
    let mut ast = DocumentAST::new();
    ast.sections.push(core_agent_document::DocumentSection::new(
        "PaymentService", 1, "PaymentService uses Database",
    ));
    let (entities, _) = manager
        .extract_from_document(Uuid::new_v4(), &ast, "tester")
        .await
        .unwrap();
    assert!(!entities.is_empty());
    let found = manager.get_entity(entities[0].id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn sqlite_persistence_audit_columns() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("semantic.db");
    let store = Arc::new(SqliteGraphStore::new(&path).unwrap());
    let manager = core_agent_semantic::SemanticManager::builder()
        .store(store.clone())
        .build();
    let mut ast = DocumentAST::new();
    ast.sections.push(core_agent_document::DocumentSection::new(
        "TestService", 1, "TestService uses Database",
    ));
    manager
        .extract_from_document(Uuid::new_v4(), &ast, "tester")
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened: Arc<dyn GraphStore> = Arc::new(SqliteGraphStore::new(&path).unwrap());
    let entities = reopened.list_entities().await.unwrap();
    assert!(!entities.is_empty());

    let connection = Connection::open(&path).unwrap();
    for table in ["semantic_entity", "semantic_relation"] {
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
async fn graph_summary_works() {
    let manager = core_agent_semantic::SemanticManager::builder().build();
    let (entities, rels) = manager.get_graph_summary().await.unwrap();
    assert_eq!(entities, 0);
    assert_eq!(rels, 0);
}