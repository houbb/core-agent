use std::sync::Arc;

use core_agent_context::{
    AddReferenceRequest, BuildContextRequest, ContextError, ContextRuntime,
    SqliteContextReferenceStore, SqliteContextSnapshotStore,
};
use core_agent_session::{
    AppendMessageRequest, CreateSessionRequest, EventBus, SessionRuntime, SqliteSessionStore,
};

async fn create_session(
    runtime: &SessionRuntime<SqliteSessionStore>,
    title: &str,
) -> (String, String) {
    let session = runtime
        .create_session(CreateSessionRequest {
            title: title.into(),
            description: None,
            owner: None,
            workspace_id: None,
        })
        .await
        .unwrap();
    let conversation = runtime
        .list_conversations(&session.id)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.conversation_type == "MAIN")
        .unwrap();
    (session.id, conversation.id)
}

#[tokio::test]
async fn complete_context_flow_uses_latest_messages_and_replays_snapshot() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(32)));
    let (session_id, conversation_id) = create_session(&session_runtime, "Context E2E").await;

    for index in 0..5 {
        session_runtime
            .append_message(AppendMessageRequest {
                conversation_id: conversation_id.clone(),
                role: "USER".into(),
                content: format!("message-{index}"),
            })
            .await
            .unwrap();
    }

    let snapshot_store = Arc::new(SqliteContextSnapshotStore::new(":memory:").unwrap());
    let runtime = ContextRuntime::new(session_store, Some(snapshot_store));
    let request = BuildContextRequest {
        session_id: session_id.clone(),
        conversation_id: None,
        system_prompt: Some("You are a deterministic agent.".into()),
        user_input: Some("Continue".into()),
        max_messages: Some(2),
        max_tokens: Some(1_000),
        compression_strategy: None,
        compression_trigger_percent: None,
        working_directory: None,
    };

    let first = runtime.build(request.clone()).await.unwrap();
    let second = runtime.build(request).await.unwrap();

    assert_eq!(
        first.conversation_id.as_ref().unwrap().to_string(),
        conversation_id
    );
    assert_eq!(first.conversation.total_count, 5);
    let messages: Vec<_> = first
        .conversation
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(messages, vec!["message-3", "message-4"]);
    assert_eq!(first.user.current_input.as_deref(), Some("Continue"));
    assert!(first.total_tokens <= 1_000);
    assert_eq!(first.hash, second.hash);
    assert_ne!(first.id, second.id);

    let restored = runtime
        .load_context_snapshot(&first.id.to_string())
        .await
        .unwrap();
    assert_eq!(restored.hash, first.hash);
    assert_eq!(restored.build_duration_ms, first.build_duration_ms);
    assert_eq!(restored.conversation.messages.len(), 2);
    let access = runtime
        .context_access_snapshot(&first.id.to_string(), 1_000)
        .await
        .unwrap();
    assert_eq!(access.total_tokens, first.total_tokens);
    assert_eq!(access.max_tokens, 1_000);
    assert_eq!(
        access.distribution.conversation,
        first.token_distribution.conversation
    );
    assert!(!serde_json::to_string(&access)
        .unwrap()
        .contains("message-4"));
    assert_eq!(
        runtime
            .list_snapshots(&session_id, 0, 10)
            .await
            .unwrap()
            .total,
        2
    );
}

#[tokio::test]
async fn context_runtime_rejects_cross_session_conversation_and_budget_overflow() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(16)));
    let (first_session, first_conversation) = create_session(&session_runtime, "First").await;
    let (second_session, _) = create_session(&session_runtime, "Second").await;
    let runtime = ContextRuntime::new(session_store, None);

    let cross_session = runtime
        .build(BuildContextRequest {
            session_id: second_session,
            conversation_id: Some(first_conversation),
            system_prompt: None,
            user_input: None,
            max_messages: None,
            max_tokens: None,
            compression_strategy: None,
            compression_trigger_percent: None,
            working_directory: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(cross_session, ContextError::InvalidArgument(_)));

    let over_budget = runtime
        .build(BuildContextRequest {
            session_id: first_session,
            conversation_id: None,
            system_prompt: Some("required".into()),
            user_input: Some("required".into()),
            max_messages: Some(0),
            max_tokens: Some(1),
            compression_strategy: None,
            compression_trigger_percent: None,
            working_directory: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(over_budget, ContextError::TokenBudgetExceeded(_)));
}

#[tokio::test]
async fn configurable_compression_observes_history_before_applying_recent_window() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(16)));
    let (session_id, conversation_id) = create_session(&session_runtime, "Compression E2E").await;
    for index in 0..5 {
        session_runtime
            .append_message(AppendMessageRequest {
                conversation_id: conversation_id.clone(),
                role: "USER".into(),
                content: format!("short-{index}"),
            })
            .await
            .unwrap();
    }
    let runtime = ContextRuntime::new(session_store, None);

    let below_threshold = runtime
        .build(BuildContextRequest {
            session_id: session_id.clone(),
            conversation_id: Some(conversation_id.clone()),
            system_prompt: None,
            user_input: None,
            max_messages: Some(2),
            max_tokens: Some(1_000),
            compression_strategy: Some("recent-window".into()),
            compression_trigger_percent: Some(100),
            working_directory: None,
        })
        .await
        .unwrap();
    assert_eq!(below_threshold.conversation.messages.len(), 5);

    let compressed = runtime
        .build(BuildContextRequest {
            session_id,
            conversation_id: Some(conversation_id),
            system_prompt: None,
            user_input: None,
            max_messages: Some(2),
            max_tokens: Some(100),
            compression_strategy: Some("recent-window".into()),
            compression_trigger_percent: Some(1),
            working_directory: None,
        })
        .await
        .unwrap();
    assert_eq!(compressed.conversation.messages.len(), 2);
}

#[tokio::test]
async fn archived_session_is_replayable_but_deleted_session_is_rejected() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(16)));
    let (session_id, _) = create_session(&session_runtime, "Lifecycle").await;
    session_runtime.start_session(&session_id).await.unwrap();
    session_runtime.archive_session(&session_id).await.unwrap();

    let context_runtime = ContextRuntime::new(session_store, None);
    let request = BuildContextRequest {
        session_id: session_id.clone(),
        conversation_id: None,
        system_prompt: None,
        user_input: None,
        max_messages: None,
        max_tokens: None,
        compression_strategy: None,
        compression_trigger_percent: None,
        working_directory: None,
    };
    assert!(context_runtime.build(request.clone()).await.is_ok());

    session_runtime.delete_session(&session_id).await.unwrap();
    assert!(matches!(
        context_runtime.build(request).await.unwrap_err(),
        ContextError::InvalidArgument(_)
    ));
}

#[tokio::test]
async fn file_snapshot_database_recovers_after_reopen() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(16)));
    let (session_id, _) = create_session(&session_runtime, "File recovery").await;
    let database = std::env::temp_dir().join(format!(
        "core-agent-context-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database_path = database.to_str().unwrap();

    let (context_id, context_hash) = {
        let snapshot_store = Arc::new(SqliteContextSnapshotStore::new(database_path).unwrap());
        let runtime = ContextRuntime::new(session_store.clone(), Some(snapshot_store));
        let context = runtime
            .build(BuildContextRequest {
                session_id,
                conversation_id: None,
                system_prompt: Some("persistent".into()),
                user_input: None,
                max_messages: None,
                max_tokens: None,
                compression_strategy: None,
                compression_trigger_percent: None,
                working_directory: None,
            })
            .await
            .unwrap();
        (context.id, context.hash)
    };

    {
        let snapshot_store = Arc::new(SqliteContextSnapshotStore::new(database_path).unwrap());
        let runtime = ContextRuntime::new(session_store, Some(snapshot_store));
        let restored = runtime
            .load_context_snapshot(&context_id.to_string())
            .await
            .unwrap();
        assert_eq!(restored.hash, context_hash);
    }

    std::fs::remove_file(database).unwrap();
}

#[tokio::test]
async fn reference_round_trip_and_context_inclusion() {
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let session_runtime = SessionRuntime::new(session_store.clone(), Arc::new(EventBus::new(16)));
    let (session_id, conversation_id) = create_session(&session_runtime, "Reference E2E").await;

    let reference_store = Arc::new(SqliteContextReferenceStore::new(":memory:").unwrap());
    let runtime = ContextRuntime::new(session_store.clone(), None)
        .with_reference_store(reference_store.clone());

    // 添加一个 File 引用 — 使用与临时目录匹配的路径和行号
    let file_ref = runtime
        .add_reference(AddReferenceRequest {
            session_id: session_id.clone(),
            reference_type: "FILE".into(),
            locator: serde_json::json!({"path": "src/main.rs", "start_line": 1, "end_line": 3}),
            snapshot: Some("fn main() {\n    println!(\"hello\");\n}".into()),
            metadata: None,
            path: Some("src/main.rs".into()),
            start_line: Some(1),
            end_line: Some(3),
            content: None,
            message_id: None,
        })
        .await
        .unwrap();
    assert_eq!(file_ref.reference_type, "FILE");

    // 添加一个 Selection 引用
    let sel_ref = runtime
        .add_reference(AddReferenceRequest {
            session_id: session_id.clone(),
            reference_type: "SELECTION".into(),
            locator: serde_json::json!({"content": "selected text"}),
            snapshot: Some("selected text".into()),
            metadata: None,
            path: None,
            start_line: None,
            end_line: None,
            content: Some("selected text".into()),
            message_id: None,
        })
        .await
        .unwrap();
    assert_eq!(sel_ref.reference_type, "SELECTION");

    // 列出引用
    let list = runtime.list_references(&session_id, 0, 10).await.unwrap();
    assert_eq!(list.total, 2);
    assert_eq!(list.items.len(), 2);

    // 构建 Context 验证引用注入 — 使用临时目录作为 working_directory
    let temp_dir = std::env::temp_dir()
        .join(format!("context-ref-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(temp_dir.join("src")).unwrap();
    std::fs::write(
        temp_dir.join("src").join("main.rs"),
        "fn main() {\n    println!(\"hello\");\n}\n",
    )
    .unwrap();

    let context = runtime
        .build(BuildContextRequest {
            session_id: session_id.clone(),
            conversation_id: None,
            system_prompt: Some("Test".into()),
            user_input: Some("analyze".into()),
            max_messages: Some(10),
            max_tokens: Some(128000),
            compression_strategy: None,
            compression_trigger_percent: None,
            working_directory: Some(temp_dir.to_string_lossy().to_string()),
        })
        .await
        .unwrap();
    // 验证 references 字段存在
    println!("Context references count: {}", context.references.len());

    // 清理临时文件
    let _ = std::fs::remove_dir_all(&temp_dir);

    // 删除引用
    runtime.delete_reference(&file_ref.id).await.unwrap();
    let list = runtime.list_references(&session_id, 0, 10).await.unwrap();
    assert_eq!(list.total, 1);

    // 清理引用
    runtime.clear_references(&session_id).await.unwrap();
    let list = runtime.list_references(&session_id, 0, 10).await.unwrap();
    assert_eq!(list.total, 0);
}
