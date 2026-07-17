//! Application 层 — 用例编排
//!
//! 协调 domain / infrastructure / event 层，实现 Session Runtime 的全部用例。

use std::sync::Arc;

use crate::domain::{
    attachment::{Attachment, AttachmentType},
    conversation::{Conversation, ConversationId, ConversationType},
    manifest::Manifest,
    message::{Message, MessageId, MessageRole},
    session::{Session, SessionId, SessionState},
};
use crate::dto::{
    AppendMessageRequest, CreateConversationRequest, CreateSessionRequest,
    UpdateMessageRequest, UpdateSessionRequest,
};
use crate::error::{SessionError, SessionResult};
use crate::event::{EventBus, SessionEvent};
use crate::infrastructure::SessionStore;

/// SessionApplicationService — 核心用例编排器
///
/// 所有 Session Runtime 的业务逻辑入口。
pub struct SessionApplicationService<S: SessionStore> {
    store: Arc<S>,
    event_bus: Arc<EventBus>,
}

impl<S: SessionStore> SessionApplicationService<S> {
    /// 创建服务实例
    pub fn new(store: Arc<S>, event_bus: Arc<EventBus>) -> Self {
        Self { store, event_bus }
    }

    // ── Session 用例 ──

    /// 创建 Session
    pub async fn create_session(&self, req: CreateSessionRequest) -> SessionResult<Session> {
        let mut session = Session::new(&req.title);
        session.description = req.description;
        session.owner = req.owner;
        session.workspace_id = req.workspace_id;

        // 就绪
        session.transition_to(SessionState::Ready)?;

        self.store.create_session(&session).await?;

        // 创建 Manifest
        let manifest = Manifest::from_session(&session);
        self.store.upsert_manifest(&manifest).await?;

        // 创建默认 MAIN Conversation
        let conv = Conversation::new_main(session.id);
        self.store.create_conversation(&conv).await?;

        // 发布事件
        self.event_bus.publish(SessionEvent::SessionCreated {
            session: session.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(SessionEvent::ConversationCreated {
            conversation: conv.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(SessionEvent::ManifestUpdated {
            manifest,
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    /// 获取 Session
    pub async fn get_session(&self, id: &SessionId) -> SessionResult<Session> {
        self.store
            .get_session(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Session {}", id)))
    }

    /// 列出 Sessions
    pub async fn list_sessions(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Session>, u64)> {
        self.store.list_sessions(offset, limit).await
    }

    /// 更新 Session
    pub async fn update_session(
        &self,
        id: &SessionId,
        req: UpdateSessionRequest,
    ) -> SessionResult<Session> {
        let mut session = self.get_session(id).await?;

        if let Some(title) = &req.title {
            session.update_title(title);
        }
        if let Some(desc) = &req.description {
            session.update_description(desc);
        }
        if let Some(owner) = req.owner {
            session.owner = Some(owner);
        }
        if let Some(ws_id) = req.workspace_id {
            session.workspace_id = Some(ws_id);
        }

        self.store.update_session(&session).await?;

        // 同步 Manifest
        let manifest = Manifest::from_session(&session);
        self.store.upsert_manifest(&manifest).await?;

        self.event_bus.publish(SessionEvent::SessionUpdated {
            session: session.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    /// 归档 Session
    pub async fn archive_session(&self, id: &SessionId) -> SessionResult<Session> {
        let mut session = self.get_session(id).await?;
        session.transition_to(SessionState::Archived)?;
        self.store.update_session(&session).await?;

        // 同步 Manifest
        let manifest = Manifest::from_session(&session);
        self.store.upsert_manifest(&manifest).await?;

        self.event_bus.publish(SessionEvent::SessionStateChanged {
            session_id: session.id,
            old_state: SessionState::Running,
            new_state: SessionState::Archived,
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    /// 删除 Session（软删除）
    pub async fn delete_session(&self, id: &SessionId) -> SessionResult<()> {
        let session = self.get_session(id).await?;
        self.store.delete_session(id).await?;

        self.event_bus.publish(SessionEvent::SessionDeleted {
            session_id: session.id,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    /// 恢复 Session（重新激活）
    pub async fn resume_session(&self, id: &SessionId) -> SessionResult<Session> {
        let mut session = self.get_session(id).await?;
        session.transition_to(SessionState::Running)?;
        session.touch();
        self.store.update_session(&session).await?;

        let manifest = Manifest::from_session(&session);
        self.store.upsert_manifest(&manifest).await?;

        self.event_bus.publish(SessionEvent::SessionStateChanged {
            session_id: session.id,
            old_state: SessionState::Paused,
            new_state: SessionState::Running,
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    // ── Conversation 用例 ──

    /// 创建 Conversation
    pub async fn create_conversation(
        &self,
        req: CreateConversationRequest,
    ) -> SessionResult<Conversation> {
        let session_id = SessionId::parse_str(&req.session_id)
            .map_err(|_| SessionError::InvalidArgument("Invalid session_id".into()))?;

        // 验证 Session 存在
        self.get_session(&session_id).await?;

        let conv_type = match req.conversation_type.as_str() {
            "PLAN" => ConversationType::Plan,
            "REVIEW" => ConversationType::Review,
            "SYSTEM" => ConversationType::System,
            "DEBUG" => ConversationType::Debug,
            _ => ConversationType::Main,
        };

        let conv = Conversation::new(session_id, conv_type, req.name);
        self.store.create_conversation(&conv).await?;

        // 更新 Manifest 统计
        if let Ok(Some(mut manifest)) = self.store.get_manifest(&session_id).await {
            let conversations = self.store.list_conversations(&session_id).await.unwrap_or_default();
            manifest.update_stats(
                conversations.len() as u32,
                manifest.message_count,
                manifest.token_count,
            );
            manifest.touch(Some(conv.id));
            self.store.upsert_manifest(&manifest).await?;
        }

        self.event_bus.publish(SessionEvent::ConversationCreated {
            conversation: conv.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(conv)
    }

    /// 列出 Conversations
    pub async fn list_conversations(
        &self,
        session_id: &SessionId,
    ) -> SessionResult<Vec<Conversation>> {
        // 验证 Session 存在
        self.get_session(session_id).await?;
        self.store.list_conversations(session_id).await
    }

    /// 获取 Conversation
    pub async fn get_conversation(
        &self,
        id: &ConversationId,
    ) -> SessionResult<Conversation> {
        self.store
            .get_conversation(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Conversation {}", id)))
    }

    // ── Message 用例 ──

    /// 追加 Message
    pub async fn append_message(&self, req: AppendMessageRequest) -> SessionResult<Message> {
        let conversation_id = ConversationId::parse_str(&req.conversation_id)
            .map_err(|_| SessionError::InvalidArgument("Invalid conversation_id".into()))?;

        // 验证 Conversation 存在
        self.get_conversation(&conversation_id).await?;

        let role = match req.role.as_str() {
            "SYSTEM" => MessageRole::System,
            "ASSISTANT" => MessageRole::Assistant,
            "TOOL" => MessageRole::Tool,
            "AGENT" => MessageRole::Agent,
            _ => MessageRole::User,
        };

        let message = Message::new(conversation_id, role, req.content);
        self.store.append_message(&message).await?;

        self.event_bus.publish(SessionEvent::MessageAdded {
            message: message.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(message)
    }

    /// 更新 Message
    pub async fn update_message(
        &self,
        id: &MessageId,
        req: UpdateMessageRequest,
    ) -> SessionResult<Message> {
        // 需要先从 store 获取 message（通过遍历？设计上应该增加 get_message）
        // MVP 简化：先通过 list 方式查找
        let _message = self
            .store
            .get_message(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Message {}", id)))?;

        let mut message = _message;
        if let Some(content) = &req.content {
            message.update_content(content);
        }
        message.mark_done();

        self.store.update_message(&message).await?;

        self.event_bus.publish(SessionEvent::MessageUpdated {
            message: message.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(message)
    }

    /// 列出 Messages
    pub async fn list_messages(
        &self,
        conversation_id: &ConversationId,
        offset: u64,
        limit: u64,
    ) -> SessionResult<(Vec<Message>, u64)> {
        // 验证 Conversation 存在
        self.get_conversation(conversation_id).await?;
        self.store.list_messages(conversation_id, offset, limit).await
    }

    /// 删除 Message
    pub async fn delete_message(&self, id: &MessageId) -> SessionResult<()> {
        let message = self
            .store
            .get_message(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Message {}", id)))?;

        self.store.delete_message(id).await?;

        self.event_bus.publish(SessionEvent::MessageDeleted {
            message_id: message.id,
            conversation_id: message.conversation_id,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    // ── Manifest 用例 ──

    /// 获取 Manifest
    pub async fn get_manifest(&self, session_id: &SessionId) -> SessionResult<Manifest> {
        self.store
            .get_manifest(session_id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Manifest for session {}", session_id)))
    }

    /// 列出 Manifests（左侧 Session 列表用）
    pub async fn list_manifests(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Manifest>, u64)> {
        self.store.list_manifests(offset, limit).await
    }

    // ── Attachment 用例 ──

    /// 添加 Attachment
    pub async fn add_attachment(
        &self,
        attachment_type: AttachmentType,
        name: String,
        message_id: Option<MessageId>,
        session_id: Option<SessionId>,
    ) -> SessionResult<Attachment> {
        let mut att = Attachment::new(attachment_type, name);

        if let Some(mid) = message_id {
            att.attach_to_message(mid);
        }
        if let Some(sid) = session_id {
            att.attach_to_session(sid);
        }

        self.store.create_attachment(&att).await?;
        Ok(att)
    }

    /// 获取 Attachment
    pub async fn get_attachment(
        &self,
        id: &crate::domain::attachment::AttachmentId,
    ) -> SessionResult<Attachment> {
        self.store
            .get_attachment(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Attachment {}", id)))
    }
}
