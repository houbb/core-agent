use std::sync::Arc;

use core_agent_rag::RagManager;
use core_agent_vector::{VectorManager, VectorQuery};

#[tokio::test]
async fn rag_pipeline_integration() {
    let vector_manager = Arc::new(VectorManager::builder().build());
    vector_manager
        .index_chunk("payment gateway timeout error", "payment-design.md", None, None, "tester")
        .await
        .unwrap();
    vector_manager
        .index_chunk("database connection pool exhausted", "database-setup.md", None, None, "tester")
        .await
        .unwrap();

    let rag = RagManager::new(vector_manager);
    let answer = rag.ask("payment", "default", "tester").await.unwrap();
    assert!(!answer.answer.is_empty());
    assert!(answer.sources.len() <= 5);
}

#[tokio::test]
async fn rag_config_limits_results() {
    let vector_manager = Arc::new(VectorManager::builder().build());
    for i in 0..10 {
        vector_manager
            .index_chunk(&format!("test content {i}"), "doc", None, None, "tester")
            .await
            .unwrap();
    }

    let rag = RagManager::new(vector_manager);
    let answer = rag.ask("test", "default", "tester").await.unwrap();
    assert!(answer.sources.len() <= 5);
}