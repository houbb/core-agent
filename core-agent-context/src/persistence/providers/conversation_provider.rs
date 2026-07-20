//! ConversationProvider — 对话上下文提供者
//!
//! 从 SessionStore 读取消息历史，转换为 ContextSegment。

use async_trait::async_trait;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::context_reference::{ReferenceLocator, ReferenceType};
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
        ContextSource::Conversation
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
                    .find(|c| {
                        matches!(
                            c.conversation_type,
                            core_agent_session::ConversationType::Main
                        )
                    })
                    .map(|c| c.id)
                    .ok_or_else(|| {
                        ContextError::NotFound(format!(
                            "No MAIN conversation found for session {}",
                            ctx.session_id
                        ))
                    })?
            }
        };

        let conversation = ctx
            .session_store
            .get_conversation(&conversation_id)
            .await
            .map_err(ContextError::from)?
            .ok_or_else(|| ContextError::NotFound(format!("Conversation {}", conversation_id)))?;
        if conversation.session_id != ctx.session_id {
            return Err(ContextError::InvalidArgument(format!(
                "Conversation {} does not belong to Session {}",
                conversation_id, ctx.session_id
            )));
        }

        // 先读取总数，再从尾部取最新 N 条，最终保持时间正序。
        let limit = u64::try_from(ctx.max_messages.unwrap_or(20))
            .map_err(|_| ContextError::InvalidArgument("max_messages is too large".into()))?;
        let (_, total) = ctx
            .session_store
            .list_messages(&conversation_id, 0, 0)
            .await
            .map_err(ContextError::from)?;
        let metadata_segment = ContextSegment::new(
            ContextSource::Conversation,
            ContextSlot::Conversation,
            serde_json::Value::Null,
            0,
            ContextSlot::Conversation.default_priority(),
        )
        .required()
        .with_meta("conversation_meta", "true")
        .with_meta("conversation_total", total.to_string())
        .with_meta("message_index", "-1");
        if limit == 0 {
            return Ok(vec![metadata_segment]);
        }
        let offset = total.saturating_sub(limit);
        let (messages, _) = ctx
            .session_store
            .list_messages(&conversation_id, offset, limit)
            .await
            .map_err(ContextError::from)?;

        let mut segments = Vec::with_capacity(messages.len() + 1);
        segments.push(metadata_segment);
        let priority = ContextSlot::Conversation.default_priority();

        // Slot 优先级保持固定，消息先后由 message_index 单独表达。
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
                ContextSource::Conversation,
                ContextSlot::Conversation,
                content,
                token_count,
                priority,
            )
            .with_meta("message_id", msg.id.to_string())
            .with_meta(
                "message_index",
                offset
                    .saturating_add(u64::try_from(i).unwrap_or(u64::MAX))
                    .to_string(),
            )
            .with_meta("conversation_total", total.to_string());

            segments.push(segment);
        }

        // 解析消息引用：从 ctx.references 中查找 Message 类型引用并获取对应消息
        for reference in &ctx.references {
            if reference.reference_type != ReferenceType::Message {
                continue;
            }
            if let ReferenceLocator::Message { message_id, .. } = &reference.locator {
                // 从 SessionStore 获取指定消息
                let msg = ctx
                    .session_store
                    .get_message(message_id)
                    .await
                    .map_err(ContextError::from)?;
                if let Some(msg) = msg {
                    let token_count = TokenCounter::estimate(&msg.content);
                    let content = serde_json::json!({
                        "id": msg.id.to_string(),
                        "role": msg.role.as_str(),
                        "content": msg.content,
                        "status": msg.status.as_str(),
                        "created_at": msg.created_at.to_rfc3339(),
                    });
                    let ref_segment = ContextSegment::new(
                        ContextSource::Reference,
                        ContextSlot::Reference,
                        content,
                        token_count,
                        ContextSlot::Reference.default_priority(),
                    )
                    .required()
                    .with_meta("message_id", msg.id.to_string())
                    .with_meta("reference_id", reference.id.to_string())
                    .with_meta("reference_type", "message");
                    segments.push(ref_segment);
                }
            }
        }

        Ok(segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_session::{
        Conversation, Message, MessageRole, Session, SessionState, SessionStore, SqliteSessionStore,
    };
    use std::sync::Arc;

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

        let ctx = ProviderContext::new(session_id, store).with_conversation(conv.id);

        let provider = ConversationProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 6);
        assert_eq!(segments[0].slot, ContextSlot::Conversation);
        assert_eq!(segments[0].metadata.get("conversation_total").unwrap(), "5");
    }

    #[tokio::test]
    async fn test_conversation_provider_collects_latest_messages() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session = Session::new("Test");
        store.create_session(&session).await.unwrap();
        let conversation = Conversation::new_main(session.id);
        store.create_conversation(&conversation).await.unwrap();
        for index in 0..5 {
            store
                .append_message(&Message::new(
                    conversation.id,
                    MessageRole::User,
                    format!("message-{index}"),
                ))
                .await
                .unwrap();
        }

        let mut context =
            ProviderContext::new(session.id, store).with_conversation(conversation.id);
        context.max_messages = Some(2);
        let segments = ConversationProvider::new().collect(&context).await.unwrap();

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[1].content["content"], "message-3");
        assert_eq!(segments[2].content["content"], "message-4");
    }
}
