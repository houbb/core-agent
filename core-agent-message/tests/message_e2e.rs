use std::sync::Arc;

use core_agent_message::{
    MessageManager, MessagePriority, MessageType, SqliteMessageStore, MessageStore,
};
use serde_json::json;
use uuid::Uuid;

fn create_manager() -> MessageManager {
    MessageManager::builder().build()
}

#[tokio::test]
async fn test_send_and_receive() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to = Uuid::new_v4();

    let msg = manager
        .send(
            from,
            to,
            MessageType::Request,
            "ANALYSIS_REQUEST",
            json!({"goal": "analyze logs"}),
            MessagePriority::Normal,
            "system",
        )
        .await
        .unwrap();
    assert_eq!(msg.from_agent_id, from);
    assert_eq!(msg.to_agent_id, to);
    assert_eq!(msg.message_type, MessageType::Request);

    let received = manager.receive(to, 10).await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, msg.id);
}

#[tokio::test]
async fn test_broadcast() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];

    let results = manager
        .broadcast(
            from,
            &to,
            MessageType::Broadcast,
            "SYSTEM_UPDATE",
            json!({"message": "deploy started"}),
            "system",
        )
        .await
        .unwrap();
    assert_eq!(results.len(), 3);
    for (i, msg) in results.iter().enumerate() {
        assert_eq!(msg.to_agent_id, to[i]);
    }
}

#[tokio::test]
async fn test_reply_to() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to = Uuid::new_v4();

    let original = manager
        .send(
            from, to, MessageType::Request, "GET_STATUS",
            json!({}), MessagePriority::Normal, "system",
        )
        .await
        .unwrap();

    let reply = manager
        .reply_to(&original, json!({"status": "ok"}), "worker")
        .await
        .unwrap();
    assert_eq!(reply.correlation_id, Some(original.id));
    assert_eq!(reply.from_agent_id, to);
    assert_eq!(reply.to_agent_id, from);
    assert_eq!(reply.message_type, MessageType::Response);
}

#[tokio::test]
async fn test_mark_read() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to = Uuid::new_v4();

    let msg = manager
        .send(from, to, MessageType::Request, "TEST", json!({}), MessagePriority::Normal, "system")
        .await
        .unwrap();

    let result = manager.mark_read(msg.id, "system").await.unwrap();
    assert!(result);

    // Receive again, this message should not be in inbox (marked Read)
    let inbox = manager.list_inbox(to, 10).await.unwrap();
    assert!(inbox.iter().all(|m| m.id != msg.id));
}

#[tokio::test]
async fn test_inbox_filtering() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to1 = Uuid::new_v4();
    let to2 = Uuid::new_v4();

    manager
        .send(from, to1, MessageType::Request, "A", json!({}), MessagePriority::Normal, "system")
        .await
        .unwrap();
    manager
        .send(from, to1, MessageType::Request, "B", json!({}), MessagePriority::Normal, "system")
        .await
        .unwrap();

    let inbox = manager.list_inbox(to1, 10).await.unwrap();
    assert_eq!(inbox.len(), 2);

    // to2 should have empty inbox
    let empty = manager.list_inbox(to2, 10).await.unwrap();
    assert!(empty.is_empty());
}

#[tokio::test]
async fn test_list_by_correlation() {
    let manager = create_manager();
    let from = Uuid::new_v4();
    let to = Uuid::new_v4();

    let original = manager
        .send(from, to, MessageType::Request, "QUERY", json!({"q": 1}), MessagePriority::Normal, "system")
        .await
        .unwrap();
    manager
        .reply_to(&original, json!({"a": 2}), "worker")
        .await
        .unwrap();

    let chain = manager.list_by_correlation(original.id).await.unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].correlation_id, Some(original.id));
}

#[tokio::test]
async fn test_sqlite_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_message.db");
    let store = Arc::new(SqliteMessageStore::new(&db_path).unwrap());
    let manager = MessageManager::builder().store(store).build();

    let msg = manager
        .send(
            Uuid::new_v4(), Uuid::new_v4(),
            MessageType::Broadcast, "PERSIST_TEST",
            json!({"data": true}), MessagePriority::High, "system",
        )
        .await
        .unwrap();

    let store2 = Arc::new(SqliteMessageStore::new(&db_path).unwrap());
    let found = store2.find(msg.id).await.unwrap().unwrap();
    assert_eq!(found.intent, "PERSIST_TEST");
    assert_eq!(found.priority, MessagePriority::High);
}
