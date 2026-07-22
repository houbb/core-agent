use std::sync::Arc;

use core_agent_learning::{
    InMemoryLearningStore, LearningManager, LearningQuery, LearningSource, LearningType,
};
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn learning_e2e_full_lifecycle() {
    let store = Arc::new(InMemoryLearningStore::default());
    let manager = LearningManager::new(store);

    let agent_id = Uuid::new_v4();

    // Create a learning record
    let record = manager
        .create_record(
            agent_id,
            LearningSource::Evaluation,
            LearningType::Skill,
            "redis-slowlog",
            "Always check slowlog first for Redis issues",
            serde_json::json!({"observation": "redis slow query"}),
            serde_json::json!({"skill": "redis-diagnosis"}),
            "system",
        )
        .await
        .unwrap();
    assert_eq!(record.status.as_str(), "CANDIDATE");

    // Approve
    let approved = manager.approve(record.id, "reviewer").await.unwrap();
    assert_eq!(approved.status.as_str(), "APPROVED");

    // Apply
    let applied = manager.apply(record.id, "system").await.unwrap();
    assert_eq!(applied.status.as_str(), "APPLIED");

    // List
    let records = manager
        .list(&LearningQuery {
            agent_id: Some(agent_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(records.len(), 1);

    // Snapshot
    let snap = manager.snapshot(agent_id).await.unwrap();
    assert_eq!(snap.total_records, 1);
    assert_eq!(snap.applied_count, 1);
}

#[tokio::test]
async fn learning_e2e_multiple_records() {
    let store = Arc::new(InMemoryLearningStore::default());
    let manager = LearningManager::new(store);

    let agent_id = Uuid::new_v4();

    // Create multiple records of different types
    for i in 0..3 {
        manager
            .create_record(
                agent_id,
                LearningSource::Evaluation,
                LearningType::Skill,
                &format!("skill-{i}"),
                "desc",
                Value::Null,
                Value::Null,
                "system",
            )
            .await
            .unwrap();
    }

    manager
        .create_record(
            agent_id,
            LearningSource::UserFeedback,
            LearningType::Prompt,
            "prompt-opt",
            "Optimize system prompt",
            Value::Null,
            Value::Null,
            "system",
        )
        .await
        .unwrap();

    let snap = manager.snapshot(agent_id).await.unwrap();
    assert_eq!(snap.total_records, 4);
    assert_eq!(*snap.by_type.get("SKILL").unwrap(), 3);
    assert_eq!(*snap.by_type.get("PROMPT").unwrap(), 1);
}