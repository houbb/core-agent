use std::collections::BTreeSet;
use std::sync::Arc;

use core_agent_document::{
    Document, DocumentChunk, DocumentError, DocumentSourceKind, DocumentStatus, DocumentType,
    DocumentStore, EmbeddingStatus, InMemoryDocumentStore, SqliteDocumentStore,
};
use rusqlite::Connection;
use tempfile::tempdir;

fn doc(
    name: &str,
    content: &str,
    doc_type: DocumentType,
    source: DocumentSourceKind,
) -> Document {
    Document::new(name, content, doc_type, source, "tester")
}

#[tokio::test]
async fn process_markdown_pipeline_works() {
    let manager = core_agent_document::DocumentManager::builder().build();
    let md = "# Title\n\n## Section 1\nContent here.\n\n## Section 2\nMore content.";
    let document = manager
        .process_document("test.md", md, DocumentType::Markdown, DocumentSourceKind::Manual, 1024, "tester")
        .await
        .unwrap();
    assert_eq!(document.status, DocumentStatus::Embedding);
    assert!(document.chunk_count > 0);
    let chunks = manager.get_chunks(document.id).await.unwrap();
    assert_eq!(chunks.len() as u32, document.chunk_count);
}

#[tokio::test]
async fn document_lifecycle_create_list_delete() {
    let manager = core_agent_document::DocumentManager::builder().build();
    let doc = doc("lifecycle.md", "# Lifecycle", DocumentType::Markdown, DocumentSourceKind::Manual);
    let processed = manager
        .process_document(&doc.name, &doc.content, doc.doc_type, doc.source, 1024, "tester")
        .await
        .unwrap();
    let listed = manager.list_documents("default").await.unwrap();
    assert!(!listed.is_empty());
    manager.delete_document(processed.id, "cleaner").await.unwrap();
    assert!(manager.get_document(processed.id).await.unwrap().is_none());
}

#[tokio::test]
async fn sqlite_has_audit_columns_and_no_foreign_keys() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("document.db");
    let store = Arc::new(SqliteDocumentStore::new(&path).unwrap());
    let manager = core_agent_document::DocumentManager::builder()
        .store(store.clone())
        .build();
    let md = "# SQLite Test\nContent";
    let document = manager
        .process_document("sqlite.md", md, DocumentType::Markdown, DocumentSourceKind::Manual, 1024, "tester")
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened: Arc<dyn DocumentStore> = Arc::new(SqliteDocumentStore::new(&path).unwrap());
    let recovered = reopened.find_document(document.id).await.unwrap().unwrap();
    assert_eq!(recovered.name, "sqlite.md");

    let connection = Connection::open(&path).unwrap();
    for table in ["document", "document_chunk"] {
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
async fn delete_cascades_chunks() {
    let manager = core_agent_document::DocumentManager::builder().build();
    let doc = manager
        .process_document("cascade.md", "# A\n## B\nC", DocumentType::Markdown, DocumentSourceKind::Manual, 1024, "tester")
        .await
        .unwrap();
    assert!(!manager.get_chunks(doc.id).await.unwrap().is_empty());
    manager.delete_document(doc.id, "cleaner").await.unwrap();
    assert!(manager.get_chunks(doc.id).await.unwrap().is_empty());
}

#[tokio::test]
async fn txt_parser_produces_ast() {
    let manager = core_agent_document::DocumentManager::builder().build();
    let doc = manager
        .process_document("readme.txt", "Hello World\nLine 2", DocumentType::Txt, DocumentSourceKind::Manual, 1024, "tester")
        .await
        .unwrap();
    assert_eq!(doc.status, DocumentStatus::Embedding);
    assert!(doc.chunk_count > 0);
}