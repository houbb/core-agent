//! ContextApplicationService — 核心用例编排器
//!
//! 协调 domain / infrastructure / persistence 层，
//! 实现 Context Runtime 的全部用例。

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;

use crate::application::composer::DefaultComposer;
use crate::application::pipeline::ContextPipeline;
use crate::application::reducer::SummaryReducer;
use crate::domain::context::Context;
use crate::dto::BuildContextRequest;
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{
    ContextSnapshotMeta, ContextSnapshotStore, ProviderContext, ReducerConfig,
};
use crate::persistence::providers::{
    ConversationProvider, EnvironmentProvider, SystemProvider, UserProvider,
};
use core_agent_session::SessionStore;
use core_agent_session::{ConversationType, SessionState};

const DEFAULT_MAX_MESSAGES: usize = 20;
const DEFAULT_MAX_TOKENS: u64 = 128_000;

/// ContextApplicationService — Context Runtime 的核心用例编排器
pub struct ContextApplicationService<S: SessionStore> {
    session_store: Arc<S>,
    snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
    pipeline: ContextPipeline,
}

impl<S: SessionStore + 'static> ContextApplicationService<S> {
    /// 创建服务实例
    pub fn new(
        session_store: Arc<S>,
        snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
    ) -> Self {
        // 构建默认 Pipeline
        let pipeline = ContextPipeline::builder()
            .add_provider(SystemProvider::new())
            .add_provider(ConversationProvider::new())
            .add_provider(EnvironmentProvider::new())
            .add_provider(UserProvider::new())
            .add_reducer(SummaryReducer::new(ReducerConfig::default()))
            .add_composer(DefaultComposer::new())
            .build();

        Self {
            session_store,
            snapshot_store,
            pipeline,
        }
    }

    /// 使用自定义 Pipeline 创建服务实例
    pub fn with_pipeline(
        session_store: Arc<S>,
        snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
        pipeline: ContextPipeline,
    ) -> Self {
        Self {
            session_store,
            snapshot_store,
            pipeline,
        }
    }

    // ── Context 用例 ──

    /// 构建 Context
    pub async fn build_context(&self, req: BuildContextRequest) -> ContextResult<Context> {
        let session_id = Uuid::parse_str(&req.session_id)
            .map_err(|_| ContextError::InvalidArgument("Invalid session_id".into()))?;

        let conversation_id = req
            .conversation_id
            .as_ref()
            .map(|s| {
                Uuid::parse_str(s)
                    .map_err(|_| ContextError::InvalidArgument("Invalid conversation_id".into()))
            })
            .transpose()?;

        let session = self
            .session_store
            .get_session(&session_id)
            .await?
            .ok_or_else(|| ContextError::NotFound(format!("Session {}", session_id)))?;
        if session.state == SessionState::Deleted {
            return Err(ContextError::InvalidArgument(format!(
                "Session {} is deleted",
                session_id
            )));
        }

        let conversation_id = match conversation_id {
            Some(id) => {
                let conversation = self
                    .session_store
                    .get_conversation(&id)
                    .await?
                    .ok_or_else(|| ContextError::NotFound(format!("Conversation {}", id)))?;
                if conversation.session_id != session_id {
                    return Err(ContextError::InvalidArgument(format!(
                        "Conversation {} does not belong to Session {}",
                        id, session_id
                    )));
                }
                id
            }
            None => self
                .session_store
                .list_conversations(&session_id)
                .await?
                .into_iter()
                .find(|conversation| conversation.conversation_type == ConversationType::Main)
                .map(|conversation| conversation.id)
                .ok_or_else(|| {
                    ContextError::NotFound(format!(
                        "No MAIN conversation found for Session {}",
                        session_id
                    ))
                })?,
        };

        let max_messages = req.max_messages.unwrap_or(DEFAULT_MAX_MESSAGES);
        let reducer_config = ReducerConfig {
            max_total_tokens: req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            keep_recent_messages: max_messages,
            enable_summary: false,
            ..ReducerConfig::default()
        };

        // 构建 ProviderContext
        let mut extensions = HashMap::new();
        if let Some(user_input) = &req.user_input {
            extensions.insert(
                "user_input".to_string(),
                serde_json::Value::String(user_input.clone()),
            );
        }

        let provider_ctx = ProviderContext {
            session_id,
            conversation_id: Some(conversation_id),
            session_store: self.session_store.clone() as Arc<dyn SessionStore>,
            system_prompt: req.system_prompt.clone(),
            working_directory: req.working_directory.clone(),
            max_messages: Some(max_messages),
            extensions,
        };

        // 执行 Pipeline
        let context = self
            .pipeline
            .execute_with_config(
                session_id,
                Some(conversation_id),
                &provider_ctx,
                &reducer_config,
            )
            .await?;

        // 如果配置了 snapshot_store，额外保存
        if let Some(store) = &self.snapshot_store {
            store.save_snapshot(&context).await?;
        }

        Ok(context)
    }

    // ── Snapshot 用例 ──

    /// 加载历史快照
    pub async fn load_snapshot(&self, id: &Uuid) -> ContextResult<Context> {
        let store = self
            .snapshot_store
            .as_ref()
            .ok_or_else(|| ContextError::Internal("No snapshot store configured".into()))?;

        store
            .load_snapshot(id)
            .await?
            .ok_or_else(|| ContextError::NotFound(format!("Snapshot {}", id)))
    }

    /// 列出快照
    pub async fn list_snapshots(
        &self,
        session_id: &Uuid,
        offset: u64,
        limit: u64,
    ) -> ContextResult<(Vec<ContextSnapshotMeta>, u64)> {
        let store = self
            .snapshot_store
            .as_ref()
            .ok_or_else(|| ContextError::Internal("No snapshot store configured".into()))?;

        store.list_snapshots(session_id, offset, limit).await
    }

    /// 清理过期快照
    pub async fn prune_snapshots(
        &self,
        session_id: &Uuid,
        keep_recent: usize,
    ) -> ContextResult<usize> {
        let store = self
            .snapshot_store
            .as_ref()
            .ok_or_else(|| ContextError::Internal("No snapshot store configured".into()))?;

        store.prune_snapshots(session_id, keep_recent).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_session::{
        Conversation, Message, MessageRole, Session, SessionState, SqliteSessionStore,
    };

    #[tokio::test]
    async fn test_build_context_with_session_data() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());

        // 创建 Session + Conversation + Messages
        let mut session = Session::new("Test Session");
        session.transition_to(SessionState::Ready).unwrap();
        store.create_session(&session).await.unwrap();

        let conv = Conversation::new_main(session.id);
        store.create_conversation(&conv).await.unwrap();

        for i in 1..=3 {
            let msg = Message::new(conv.id, MessageRole::User, format!("Message {}", i));
            store.append_message(&msg).await.unwrap();
        }

        let service = ContextApplicationService::new(store, None);

        let req = BuildContextRequest {
            session_id: session.id.to_string(),
            conversation_id: Some(conv.id.to_string()),
            system_prompt: Some("You are helpful.".into()),
            user_input: Some("Hello agent".into()),
            max_messages: Some(10),
            max_tokens: Some(128000),
            working_directory: None,
        };

        let context = service.build_context(req).await.unwrap();
        assert_eq!(context.session_id, session.id);
        assert!(context.total_tokens > 0);
        assert!(context.system.prompt.is_some());
        assert_eq!(context.conversation.messages.len(), 3);
    }

    #[tokio::test]
    async fn test_build_context_invalid_session() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let service = ContextApplicationService::new(store, None);

        let req = BuildContextRequest {
            session_id: "not-a-uuid".into(),
            conversation_id: None,
            system_prompt: None,
            user_input: None,
            max_messages: None,
            max_tokens: None,
            working_directory: None,
        };

        let result = service.build_context(req).await;
        assert!(result.is_err());
    }
}
