//! API 层 — 公开 API 接口
//!
//! Context Runtime 的对外门面。所有外部调用者（后续的 Model Runtime、CLI、Web）
//! 都通过此层访问 Context Runtime。

use std::sync::Arc;
use uuid::Uuid;

use crate::application::ContextApplicationService;
use crate::application::ContextPipeline;
use crate::domain::Context;
use crate::dto::{
    AddReferenceRequest, BuildContextRequest, ContextAccessSnapshot, ContextResponse,
    ContextSnapshotResponse, ListResponse, ReferenceResponse,
};
use crate::error::ContextResult;
use crate::infrastructure::ContextSnapshotStore;
use crate::persistence::SqliteContextReferenceStore;
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
///     compression_strategy: None,
///     compression_trigger_percent: None,
///     working_directory: None,
/// }).await?;
/// ```
pub struct ContextRuntime<S: SessionStore> {
    app: ContextApplicationService<S>,
    reference_store: Option<Arc<SqliteContextReferenceStore>>,
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
        Self { app, reference_store: None }
    }

    /// 使用自定义 Pipeline 创建 Runtime。
    pub fn with_pipeline(
        session_store: Arc<S>,
        snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
        pipeline: ContextPipeline,
    ) -> Self {
        Self {
            app: ContextApplicationService::with_pipeline(session_store, snapshot_store, pipeline),
            reference_store: None,
        }
    }

    /// 设置 ReferenceStore
    pub fn with_reference_store(mut self, store: Arc<SqliteContextReferenceStore>) -> Self {
        self.reference_store = Some(store);
        self
    }

    /// 构建 Context（带 references 注入）
    ///
    /// 这是 Context Runtime 的核心 API。
    /// 执行完整的 Pipeline：Collect → Reduce → Compose → Snapshot。
    pub async fn build_context(&self, req: BuildContextRequest) -> ContextResult<ContextResponse> {
        let context = self.build_with_references(req).await?;
        Ok(ContextResponse::from(&context))
    }

    /// 构建并返回完整领域 Context，供 Model Runtime 等框架消费者直接使用。
    pub async fn build(&self, req: BuildContextRequest) -> ContextResult<Context> {
        self.build_with_references(req).await
    }

    /// 内部：构建 Context，注入 references
    async fn build_with_references(&self, req: BuildContextRequest) -> ContextResult<Context> {
        if let Some(ref_store) = &self.reference_store {
            let app = ContextApplicationService::with_stores(
                self.app.session_store(),
                self.app.snapshot_store(),
                Some(ref_store.clone()),
            );
            app.build_context(req).await
        } else {
            self.app.build_context(req).await
        }
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

    /// Loads a persisted Context as content-free occupancy metadata.
    pub async fn context_access_snapshot(
        &self,
        id: &str,
        max_tokens: u64,
    ) -> ContextResult<ContextAccessSnapshot> {
        let context = self.load_context_snapshot(id).await?;
        Ok(ContextAccessSnapshot::from_context(&context, max_tokens))
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

    // ── Reference API ──

    /// 添加引用
    pub async fn add_reference(&self, req: AddReferenceRequest) -> ContextResult<ReferenceResponse> {
        let store = self.reference_store.as_ref().ok_or_else(|| {
            crate::error::ContextError::Internal("No reference store configured".into())
        })?;

        let reference_type = match req.reference_type.to_uppercase().as_str() {
            "FILE" => crate::domain::context_reference::ReferenceType::File,
            "SELECTION" => crate::domain::context_reference::ReferenceType::Selection,
            "MESSAGE" => crate::domain::context_reference::ReferenceType::Message,
            _ => return Err(crate::error::ContextError::InvalidArgument(
                format!("Unknown reference type: {}", req.reference_type)
            )),
        };

        let locator = match reference_type {
            crate::domain::context_reference::ReferenceType::File => {
                let path = req.path.clone().ok_or_else(|| {
                    crate::error::ContextError::InvalidArgument("path is required for File reference".into())
                })?;
                crate::domain::context_reference::ReferenceLocator::File {
                    path,
                    start_line: req.start_line,
                    end_line: req.end_line,
                }
            }
            crate::domain::context_reference::ReferenceType::Selection => {
                let content = req.content.clone().ok_or_else(|| {
                    crate::error::ContextError::InvalidArgument("content is required for Selection reference".into())
                })?;
                crate::domain::context_reference::ReferenceLocator::Selection {
                    content,
                    source_path: req.path.clone(),
                    start_line: req.start_line,
                    end_line: req.end_line,
                }
            }
            crate::domain::context_reference::ReferenceType::Message => {
                return Err(crate::error::ContextError::InvalidArgument(
                    "Message references must be created via the application layer".into()
                ));
            }
        };

        let mut metadata = req.metadata.unwrap_or_default();
        metadata.insert("session_id".to_string(), req.session_id.clone());

        let reference = crate::domain::context_reference::ContextReference {
            id: uuid::Uuid::new_v4(),
            reference_type,
            locator,
            snapshot: req.snapshot,
            metadata,
            created_at: chrono::Utc::now(),
        };

        store.save_reference(&reference).await?;
        Ok(ReferenceResponse {
            id: reference.id.to_string(),
            reference_type: reference.reference_type.as_str().to_string(),
            locator: serde_json::to_value(&reference.locator)
                .map_err(|e| crate::error::ContextError::Serialization(e.to_string()))?,
            snapshot: reference.snapshot,
            created_at: reference.created_at.to_rfc3339(),
        })
    }

    /// 列出引用
    pub async fn list_references(
        &self,
        session_id: &str,
        offset: u64,
        limit: u64,
    ) -> ContextResult<ListResponse<ReferenceResponse>> {
        // 使用 reference_store 直接查询
        let store = self.reference_store.as_ref().ok_or_else(|| {
            crate::error::ContextError::Internal("No reference store configured".into())
        })?;
        let (refs, total) = store.list_references(session_id, offset, limit).await?;
        Ok(ListResponse {
            items: refs.iter().map(|r| ReferenceResponse {
                id: r.id.to_string(),
                reference_type: r.reference_type.as_str().to_string(),
                locator: serde_json::to_value(&r.locator)
                    .unwrap_or(serde_json::Value::Null),
                snapshot: r.snapshot.clone(),
                created_at: r.created_at.to_rfc3339(),
            }).collect(),
            total,
            offset,
            limit,
        })
    }

    /// 删除引用
    pub async fn delete_reference(&self, id: &str) -> ContextResult<()> {
        let store = self.reference_store.as_ref().ok_or_else(|| {
            crate::error::ContextError::Internal("No reference store configured".into())
        })?;
        store.delete_reference(id).await
    }

    /// 清理 Session 引用
    pub async fn clear_references(&self, session_id: &str) -> ContextResult<usize> {
        let store = self.reference_store.as_ref().ok_or_else(|| {
            crate::error::ContextError::Internal("No reference store configured".into())
        })?;
        store.clear_references(session_id).await
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
                compression_strategy: None,
                compression_trigger_percent: None,
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
