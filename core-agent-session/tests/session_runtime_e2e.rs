use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent_session::{
    AppendMessageRequest, AttachmentType, CreateConversationRequest, CreateSessionRequest,
    EventBus, MessageStatus, Session, SessionEvent, SessionLifecycle, SessionResult,
    SessionRuntime, SessionState, SqliteSessionStore, UpdateMessageRequest, UpdateSessionRequest,
};

#[derive(Default)]
struct RecordingLifecycle {
    before: AtomicUsize,
    after: AtomicUsize,
}

#[async_trait]
impl SessionLifecycle for RecordingLifecycle {
    async fn before_transition(
        &self,
        _session: &Session,
        _target: SessionState,
    ) -> SessionResult<()> {
        self.before.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn after_transition(&self, _session: &Session, _previous: SessionState) {
        self.after.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn complete_session_lifecycle_keeps_related_data_consistent() {
    let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let event_bus = Arc::new(EventBus::default());
    let mut events = event_bus.subscribe();
    let runtime = SessionRuntime::new(store, event_bus);

    let session = runtime
        .create_session(CreateSessionRequest {
            title: "Agent Workspace".into(),
            description: Some("P0 end-to-end test".into()),
            owner: Some("tester".into()),
            workspace_id: Some("workspace-1".into()),
        })
        .await
        .unwrap();
    assert_eq!(session.state, "READY");

    let conversations = runtime.list_conversations(&session.id).await.unwrap();
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].conversation_type, "MAIN");
    let conversation_id = conversations[0].id.clone();

    let manifest = runtime.get_manifest(&session.id).await.unwrap();
    assert_eq!(manifest.conversation_count, 1);
    assert_eq!(manifest.message_count, 0);

    assert!(runtime.archive_session(&session.id).await.is_err());
    assert_eq!(
        runtime.start_session(&session.id).await.unwrap().state,
        "RUNNING"
    );
    assert_eq!(
        runtime.pause_session(&session.id).await.unwrap().state,
        "PAUSED"
    );
    assert_eq!(
        runtime.resume_session(&session.id).await.unwrap().state,
        "RUNNING"
    );

    let message = runtime
        .append_message(AppendMessageRequest {
            conversation_id: conversation_id.clone(),
            role: "ASSISTANT".into(),
            content: "partial".into(),
        })
        .await
        .unwrap();
    assert_eq!(message.status, "PENDING");

    let message = runtime
        .update_message_status(&message.id, MessageStatus::Streaming)
        .await
        .unwrap();
    assert_eq!(message.status, "STREAMING");
    let message = runtime
        .update_message(
            &message.id,
            UpdateMessageRequest {
                content: Some("complete response".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(message.status, "STREAMING");
    assert_eq!(message.content, "complete response");
    let message = runtime
        .update_message_status(&message.id, MessageStatus::Done)
        .await
        .unwrap();
    assert_eq!(message.status, "DONE");
    assert!(runtime
        .update_message(
            &message.id,
            UpdateMessageRequest {
                content: Some("invalid late edit".into()),
            },
        )
        .await
        .is_err());

    let manifest = runtime.get_manifest(&session.id).await.unwrap();
    assert_eq!(manifest.message_count, 1);

    let attachment_id = runtime
        .add_attachment(
            AttachmentType::File,
            "evidence.txt".into(),
            Some(message.id.clone()),
            Some(session.id.clone()),
        )
        .await
        .unwrap();
    let attachment = runtime.get_attachment(&attachment_id).await.unwrap();
    assert_eq!(attachment.name, "evidence.txt");

    runtime.delete_message(&message.id).await.unwrap();
    assert_eq!(
        runtime
            .get_manifest(&session.id)
            .await
            .unwrap()
            .message_count,
        0
    );

    assert_eq!(
        runtime.archive_session(&session.id).await.unwrap().state,
        "ARCHIVED"
    );
    assert!(runtime
        .append_message(AppendMessageRequest {
            conversation_id,
            role: "USER".into(),
            content: "too late".into(),
        })
        .await
        .is_err());

    runtime.delete_session(&session.id).await.unwrap();
    assert_eq!(
        runtime.get_session(&session.id).await.unwrap().state,
        "DELETED"
    );
    assert_eq!(runtime.list_sessions(0, 10).await.unwrap().total, 0);
    assert_eq!(runtime.list_manifests(0, 10).await.unwrap().total, 0);

    let mut transitions = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let SessionEvent::SessionStateChanged {
            old_state,
            new_state,
            ..
        } = event.as_ref()
        {
            transitions.push((*old_state, *new_state));
        }
    }
    assert!(transitions.contains(&(
        core_agent_session::SessionState::Created,
        core_agent_session::SessionState::Ready,
    )));
    assert!(transitions.contains(&(
        core_agent_session::SessionState::Archived,
        core_agent_session::SessionState::Deleted,
    )));
}

#[tokio::test]
async fn file_database_restores_session_manifest_and_main_conversation() {
    let database =
        std::env::temp_dir().join(format!("core-agent-session-{}.db", uuid::Uuid::new_v4()));

    let session_id = {
        let store = Arc::new(SqliteSessionStore::new(database.to_str().unwrap()).unwrap());
        let runtime = SessionRuntime::new(store, Arc::new(EventBus::default()));
        runtime
            .create_session(CreateSessionRequest {
                title: "Persistent workspace".into(),
                description: None,
                owner: None,
                workspace_id: None,
            })
            .await
            .unwrap()
            .id
    };

    {
        let store = Arc::new(SqliteSessionStore::new(database.to_str().unwrap()).unwrap());
        let runtime = SessionRuntime::new(store, Arc::new(EventBus::default()));
        assert_eq!(
            runtime.get_session(&session_id).await.unwrap().state,
            "READY"
        );
        assert_eq!(
            runtime
                .get_manifest(&session_id)
                .await
                .unwrap()
                .conversation_count,
            1
        );
        assert_eq!(
            runtime.list_conversations(&session_id).await.unwrap().len(),
            1
        );
    }

    std::fs::remove_file(database).unwrap();
}

#[tokio::test]
async fn custom_lifecycle_wraps_state_changes() {
    let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let event_bus = Arc::new(EventBus::default());
    let lifecycle = Arc::new(RecordingLifecycle::default());
    let runtime = SessionRuntime::with_lifecycle(store, event_bus, lifecycle.clone());

    let session = runtime
        .create_session(CreateSessionRequest {
            title: "Hooked".into(),
            description: None,
            owner: None,
            workspace_id: None,
        })
        .await
        .unwrap();
    runtime.start_session(&session.id).await.unwrap();

    assert_eq!(lifecycle.before.load(Ordering::SeqCst), 2);
    assert_eq!(lifecycle.after.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn runtime_rejects_invalid_or_inconsistent_related_data() {
    let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let runtime = SessionRuntime::new(store, Arc::new(EventBus::default()));

    assert!(runtime
        .create_session(CreateSessionRequest {
            title: "   ".into(),
            description: None,
            owner: None,
            workspace_id: None,
        })
        .await
        .is_err());

    let session = runtime
        .create_session(CreateSessionRequest {
            title: "Validated".into(),
            description: None,
            owner: None,
            workspace_id: None,
        })
        .await
        .unwrap();
    let updated = runtime
        .update_session(
            &session.id,
            UpdateSessionRequest {
                title: None,
                description: None,
                owner: Some("new-owner".into()),
                workspace_id: None,
            },
        )
        .await
        .unwrap();
    assert_ne!(updated.updated_at, session.updated_at);
    let mut conversations = runtime.list_conversations(&session.id).await.unwrap();
    let conversation = conversations.remove(0);

    assert!(runtime
        .create_conversation(CreateConversationRequest {
            session_id: session.id.clone(),
            conversation_type: "MAIN".into(),
            name: None,
        })
        .await
        .is_err());
    assert!(runtime
        .append_message(AppendMessageRequest {
            conversation_id: conversation.id,
            role: "UNKNOWN".into(),
            content: "invalid".into(),
        })
        .await
        .is_err());
    assert!(runtime
        .add_attachment(AttachmentType::File, "orphan.txt".into(), None, None)
        .await
        .is_err());
}
