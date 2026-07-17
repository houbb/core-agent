//! ConversationProvider — 对话上下文提供者
//!
//! 从 SessionStore 读取消息历史，转换为 ContextSegment。

use async_trait::async_trait;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{ContextProvider, ProviderContext};

/// ConversationProvider
///
/// 从 SessionStore 读取指定 Conversation 的消息历史。
/// 每条消息生成一个 ContextSegment。
pub struct ConversationProvider {
    /// 是否启用
    enabled: bool,
}

impl ConversationProvider {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    pub fn disabled() -> Self {
        Self { enabled: false }
    }
}

impl Default for ConversationProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for ConversationProvider {
    fn name(&self) -> &str {
        "conversation-provider"
    }

    fn source(&self) -> ContextSource {
        ContextSource::System
    }

    fn slot(&self) -> ContextSlot {
        ContextSlot::Conversation
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
        // 确定要读取的 Conversation ID
        let conversation_id = match ctx.conversation_id {
            Some(cid) => cid,
            None => {
                // 未指定时，找到 MAIN conversation
                let conversations = ctx
                    .session_store
                    .list_conversations(&ctx.session_id)
                    .await
                    .map_err(ContextError::from)?;

                conversations
                    .iter()
                    .find(|c| matches!(c.conversation_type, core_agent_session::ConversationType::Main))
                    .map(|c| c.id)
                    .ok_or_else(|| {
                        ContextError::NotFound(format!(
                            "No MAIN conversation found for session {}",
                            ctx.session_id
                        ))
                    })?
            }
        };

        // 读取消息
        let limit = ctx.max_messages.unwrap_or(100).min(1000) as u64;
        let (messages, _total) = ctx
            .session_store
            .list_messages(&conversation_id, 0, limit)
            .await
            .map_err(ContextError::from)?;

        let mut segments = Vec::with_capacity(messages.len());
        let base_priority = ContextSlot::Conversation.default_priority();

        // 按时间顺序，后面的消息优先级稍高
        for (i, msg) in messages.iter().enumerate() {
            let token_count = TokenCounter::estimate(&msg.content);
            let content = serde_json::json!({
                "id": msg.id.to_string(),
                "role": msg.role.as_str(),
                "content": msg.content,
                "status": msg.status.as_str(),
                "created_at": msg.created_at.to_rfc3339(),
            });

            let segment = ContextSegment::new(
                ContextSource::System,
                ContextSlot::Conversation,
                content,
                token_count,
                base_priority + (i as i32), // 后面的消息权重稍高
            )
            .with_meta("message_id", msg.id.to_string())
            .with_meta("message_index", i.to_string());

            segments.push(segment);
        }

        Ok(segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use core_agent_session::{
        Conversation, Message, MessageRole, Session, SessionState, SessionStore,
        SqliteSessionStore,
    };

    #[tokio::test]
    async fn test_conversation_provider_collect() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();

        // 创建 Session + Conversation + Messages
        let mut session = Session::new("Test");
        session.id = session_id;
        session.transition_to(SessionState::Ready).unwrap();
        store.create_session(&session).await.unwrap();

        let conv = Conversation::new_main(session_id);
        store.create_conversation(&conv).await.unwrap();

        for i in 1..=5 {
            let msg = Message::new(conv.id, MessageRole::User, format!("Message {}", i));
            store.append_message(&msg).await.unwrap();
        }

        let ctx = ProviderContext::new(session_id, store)
            .with_conversation(conv.id);

        let provider = ConversationProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 5);
        assert_eq!(segments[0].slot, ContextSlot::Conversation);
    }
}