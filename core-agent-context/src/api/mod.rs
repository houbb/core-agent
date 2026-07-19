//! API 层 — 公开 API 接口
//!
//! Context Runtime 的对外门面。所有外部调用者（后续的 Model Runtime、CLI、Web）
//! 都通过此层访问 Context Runtime。

use std::sync::Arc;
use uuid::Uuid;

use crate::application::ContextApplicationService;
use crate::application::ContextPipeline;
use crate::domain::Context;
use crate::dto::{BuildContextRequest, ContextResponse, ContextSnapshotResponse, ListResponse};
use crate::error::ContextResult;
use crate::infrastructure::ContextSnapshotStore;
use core_agent_session::SessionStore;

/// ContextRuntime — Context Runtime 公开 API
///
/// # Example
///
/// ```ignore
/// use core_agent_context::ContextRuntime;
/// use core_agent_session::{SqliteSessionStore, EventBus};
///
/// let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
/// let runtime = ContextRuntime::new(session_store, None);
///
/// let context = runtime.build_context(BuildContextRequest {
///     session_id: "...".into(),
///     conversation_id: None,
///     system_prompt: Some("You are helpful.".into()),
///     user_input: Some("Hello".into()),
///     max_messages: Some(20),
///     max_tokens: Some(128000),
///     working_directory: None,
/// }).await?;
/// ```
pub struct ContextRuntime<S: SessionStore> {
    app: ContextApplicationService<S>,
}

impl<S: SessionStore + 'static> ContextRuntime<S> {
    /// 创建 Context Runtime 实例
    ///
    /// - `session_store` 用于读取 Session/Conversation/Message 数据
    /// - `snapshot_store` 可选，用于持久化 Context 快照
    pub fn new(
        session_store: Arc<S>,
        snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
    ) -> Self {
        let app = ContextApplicationService::new(session_store, snapshot_store);
        Self { app }
    }

    /// 使用自定义 Pipeline 创建 Runtime。
    pub fn with_pipeline(
        session_store: Arc<S>,
        snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
        pipeline: ContextPipeline,
    ) -> Self {
        Self {
            app: ContextApplicationService::with_pipeline(session_store, snapshot_store, pipeline),
        }
    }

    // ── Context API ──

    /// 构建 Context
    ///
    /// 这是 Context Runtime 的核心 API。
    /// 执行完整的 Pipeline：Collect → Reduce → Compose → Snapshot。
    pub async fn build_context(&self, req: BuildContextRequest) -> ContextResult<ContextResponse> {
        let context = self.app.build_context(req).await?;
        Ok(ContextResponse::from(&context))
    }

    /// 构建并返回完整领域 Context，供 Model Runtime 等框架消费者直接使用。
    pub async fn build(&self, req: BuildContextRequest) -> ContextResult<Context> {
        self.app.build_context(req).await
    }

    // ── Snapshot API ──

    /// 加载历史快照
    pub async fn load_snapshot(&self, id: &str) -> ContextResult<ContextSnapshotResponse> {
        let uid = Uuid::parse_str(id).map_err(|_| {
            crate::error::ContextError::InvalidArgument("Invalid snapshot id".into())
        })?;
        let snapshot = self.app.load_snapshot(&uid).await?;
        Ok(ContextSnapshotResponse {
            id: snapshot.id.to_string(),
            session_id: snapshot.session_id.to_string(),
            conversation_id: snapshot.conversation_id.map(|id| id.to_string()),
            created_at: snapshot.built_at.to_rfc3339(),
            token_count: snapshot.total_tokens,
            hash: snapshot.hash,
        })
    }

    /// 加载完整 Context 快照，用于 Replay、Debug 和 Audit。
    pub async fn load_context_snapshot(&self, id: &str) -> ContextResult<Context> {
        let uid = Uuid::parse_str(id).map_err(|_| {
            crate::error::ContextError::InvalidArgument("Invalid snapshot id".into())
        })?;
        self.app.load_snapshot(&uid).await
    }

    /// 列出某 Session 的所有快照
    pub async fn list_snapshots(
        &self,
        session_id: &str,
        offset: u64,
        limit: u64,
    ) -> ContextResult<ListResponse<ContextSnapshotResponse>> {
        let sid = Uuid::parse_str(session_id).map_err(|_| {
            crate::error::ContextError::InvalidArgument("Invalid session id".into())
        })?;
        let (snapshots, total) = self.app.list_snapshots(&sid, offset, limit).await?;
        Ok(ListResponse {
            items: snapshots
                .iter()
                .map(ContextSnapshotResponse::from)
                .collect(),
            total,
            offset,
            limit,
        })
    }

    /// 清理过期快照
    pub async fn prune_snapshots(
        &self,
        session_id: &str,
        keep_recent: usize,
    ) -> ContextResult<usize> {
        let sid = Uuid::parse_str(session_id).map_err(|_| {
            crate::error::ContextError::InvalidArgument("Invalid session id".into())
        })?;
        self.app.prune_snapshots(&sid, keep_recent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_session::{
        Conversation, Message, MessageRole, Session, SessionState, SqliteSessionStore,
    };

    #[tokio::test]
    async fn test_context_runtime_build() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());

        let mut session = Session::new("Test");
        session.transition_to(SessionState::Ready).unwrap();
        store.create_session(&session).await.unwrap();

        let conv = Conversation::new_main(session.id);
        store.create_conversation(&conv).await.unwrap();

        for i in 1..=3 {
            let msg = Message::new(conv.id, MessageRole::User, format!("msg{}", i));
            store.append_message(&msg).await.unwrap();
        }

        let runtime = ContextRuntime::new(store, None);
        let resp = runtime
            .build_context(BuildContextRequest {
                session_id: session.id.to_string(),
                conversation_id: Some(conv.id.to_string()),
                system_prompt: Some("You are helpful.".into()),
                user_input: Some("Hello!".into()),
                max_messages: Some(20),
                max_tokens: None,
                working_directory: None,
            })
            .await
            .unwrap();

        assert_eq!(resp.session_id, session.id.to_string());
        assert!(resp.total_tokens > 0);
        assert!(resp.user.has_input);
        assert_eq!(resp.context.conversation.messages.len(), 3);
        assert!(!resp.hash.is_empty());
    }
}
