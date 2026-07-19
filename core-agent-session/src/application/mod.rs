//! Application 层 — 用例编排
//!
//! 协调 domain / infrastructure / event 层，实现 Session Runtime 的全部用例。

use std::sync::Arc;

use crate::domain::{
    attachment::{Attachment, AttachmentType},
    conversation::{Conversation, ConversationId, ConversationType},
    manifest::Manifest,
    message::{Message, MessageId, MessageRole, MessageStatus},
    session::{Session, SessionId, SessionState},
};
use crate::dto::{
    AppendMessageRequest, CreateConversationRequest, CreateSessionRequest, UpdateMessageRequest,
    UpdateSessionRequest,
};
use crate::error::{SessionError, SessionResult};
use crate::event::{EventBus, SessionEvent};
use crate::infrastructure::{NoopSessionLifecycle, SessionLifecycle, SessionStore};

/// SessionApplicationService — 核心用例编排器
///
/// 所有 Session Runtime 的业务逻辑入口。
pub struct SessionApplicationService<S: SessionStore> {
    store: Arc<S>,
    event_bus: Arc<EventBus>,
    lifecycle: Arc<dyn SessionLifecycle>,
}

impl<S: SessionStore> SessionApplicationService<S> {
    /// 创建服务实例
    pub fn new(store: Arc<S>, event_bus: Arc<EventBus>) -> Self {
        Self::with_lifecycle(store, event_bus, Arc::new(NoopSessionLifecycle))
    }

    /// 使用自定义生命周期钩子创建服务实例。
    pub fn with_lifecycle(
        store: Arc<S>,
        event_bus: Arc<EventBus>,
        lifecycle: Arc<dyn SessionLifecycle>,
    ) -> Self {
        Self {
            store,
            event_bus,
            lifecycle,
        }
    }

    // ── Session 用例 ──

    /// 创建 Session
    pub async fn create_session(&self, req: CreateSessionRequest) -> SessionResult<Session> {
        let title = req.title.trim();
        if title.is_empty() {
            return Err(SessionError::InvalidArgument(
                "Session title must not be empty".into(),
            ));
        }

        let mut session = Session::new(title);
        session.description = req.description;
        session.owner = req.owner;
        session.workspace_id = req.workspace_id;

        // 就绪
        self.lifecycle
            .before_transition(&session, SessionState::Ready)
            .await?;
        session.transition_to(SessionState::Ready)?;

        // 创建默认 MAIN Conversation
        let conv = Conversation::new_main(session.id);

        // 创建 Manifest，并立即纳入默认 MAIN Conversation。
        let mut manifest = Manifest::from_session(&session);
        manifest.update_stats(1, 0, None);
        manifest.last_conversation_id = Some(conv.id);

        // SQLite 实现会在一个事务中写入三个对象。
        self.store
            .create_session_bundle(&session, &manifest, &conv)
            .await?;
        self.lifecycle
            .after_transition(&session, SessionState::Created)
            .await;

        // 发布事件
        self.event_bus.publish(SessionEvent::SessionCreated {
            session: session.clone(),
            timestamp: chrono::Utc::now(),
        });
        self.event_bus.publish(SessionEvent::SessionStateChanged {
            session_id: session.id,
            old_state: SessionState::Created,
            new_state: SessionState::Ready,
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
    pub async fn list_sessions(
        &self,
        offset: u64,
        limit: u64,
    ) -> SessionResult<(Vec<Session>, u64)> {
        self.store.list_sessions(offset, limit).await
    }

    /// 更新 Session
    pub async fn update_session(
        &self,
        id: &SessionId,
        req: UpdateSessionRequest,
    ) -> SessionResult<Session> {
        let mut session = self.get_session(id).await?;
        if matches!(session.state, SessionState::Deleted) {
            return Err(SessionError::InvalidArgument(format!(
                "Deleted session {} cannot be updated",
                session.id
            )));
        }
        let mut changed = false;

        if let Some(title) = &req.title {
            let title = title.trim();
            if title.is_empty() {
                return Err(SessionError::InvalidArgument(
                    "Session title must not be empty".into(),
                ));
            }
            session.update_title(title);
            changed = true;
        }
        if let Some(desc) = &req.description {
            session.update_description(desc);
            changed = true;
        }
        if let Some(owner) = req.owner {
            let owner = owner.trim();
            if owner.is_empty() {
                return Err(SessionError::InvalidArgument(
                    "Session owner must not be empty when supplied".into(),
                ));
            }
            session.owner = Some(owner.to_string());
            changed = true;
        }
        if let Some(ws_id) = req.workspace_id {
            let ws_id = ws_id.trim();
            if ws_id.is_empty() {
                return Err(SessionError::InvalidArgument(
                    "Workspace id must not be empty when supplied".into(),
                ));
            }
            session.workspace_id = Some(ws_id.to_string());
            changed = true;
        }

        if !changed {
            return Ok(session);
        }
        session.updated_at = chrono::Utc::now();

        self.store.update_session(&session).await?;

        self.sync_manifest_with_session(&session).await?;

        self.event_bus.publish(SessionEvent::SessionUpdated {
            session: session.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    /// 启动 Session。
    pub async fn start_session(&self, id: &SessionId) -> SessionResult<Session> {
        self.transition_session(id, SessionState::Running).await
    }

    /// 暂停 Session。
    pub async fn pause_session(&self, id: &SessionId) -> SessionResult<Session> {
        self.transition_session(id, SessionState::Paused).await
    }

    /// 归档 Session。
    pub async fn archive_session(&self, id: &SessionId) -> SessionResult<Session> {
        self.transition_session(id, SessionState::Archived).await
    }

    /// 删除 Session（软删除）
    pub async fn delete_session(&self, id: &SessionId) -> SessionResult<()> {
        let mut session = self.get_session(id).await?;
        let previous = session.state;
        self.lifecycle
            .before_transition(&session, SessionState::Deleted)
            .await?;
        session.transition_to(SessionState::Deleted)?;
        self.store.delete_session(id).await?;
        self.sync_manifest_with_session(&session).await?;
        self.lifecycle.after_transition(&session, previous).await;

        self.event_bus.publish(SessionEvent::SessionStateChanged {
            session_id: session.id,
            old_state: previous,
            new_state: SessionState::Deleted,
            timestamp: chrono::Utc::now(),
        });

        self.event_bus.publish(SessionEvent::SessionDeleted {
            session_id: session.id,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    /// 恢复 Session（重新激活）
    pub async fn resume_session(&self, id: &SessionId) -> SessionResult<Session> {
        self.transition_session(id, SessionState::Running).await
    }

    // ── Conversation 用例 ──

    /// 创建 Conversation
    pub async fn create_conversation(
        &self,
        req: CreateConversationRequest,
    ) -> SessionResult<Conversation> {
        let session_id = SessionId::parse_str(&req.session_id)
            .map_err(|_| SessionError::InvalidArgument("Invalid session_id".into()))?;

        // 验证 Session 存在且尚未结束。
        let session = self.get_session(&session_id).await?;
        Self::ensure_session_mutable(&session)?;

        let conv_type = match req.conversation_type.to_ascii_uppercase().as_str() {
            "MAIN" => ConversationType::Main,
            "PLAN" => ConversationType::Plan,
            "REVIEW" => ConversationType::Review,
            "SYSTEM" => ConversationType::System,
            "DEBUG" => ConversationType::Debug,
            _ => {
                return Err(SessionError::InvalidArgument(format!(
                    "Unsupported conversation type: {}",
                    req.conversation_type
                )))
            }
        };

        if matches!(conv_type, ConversationType::Main)
            && self
                .store
                .list_conversations(&session_id)
                .await?
                .iter()
                .any(|conversation| {
                    matches!(conversation.conversation_type, ConversationType::Main)
                })
        {
            return Err(SessionError::AlreadyExists(format!(
                "MAIN conversation for session {}",
                session_id
            )));
        }

        if req
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(SessionError::InvalidArgument(
                "Conversation name must not be empty when supplied".into(),
            ));
        }

        let conv = Conversation::new(
            session_id,
            conv_type,
            req.name.map(|name| name.trim().to_string()),
        );
        self.store.create_conversation(&conv).await?;

        self.refresh_manifest(&session_id, Some(conv.id)).await?;

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
    pub async fn get_conversation(&self, id: &ConversationId) -> SessionResult<Conversation> {
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

        // 验证 Conversation 及其 Session 可写。
        let conversation = self.get_conversation(&conversation_id).await?;
        let session = self.get_session(&conversation.session_id).await?;
        Self::ensure_session_mutable(&session)?;

        let role = match req.role.to_ascii_uppercase().as_str() {
            "USER" => MessageRole::User,
            "SYSTEM" => MessageRole::System,
            "ASSISTANT" => MessageRole::Assistant,
            "TOOL" => MessageRole::Tool,
            "AGENT" => MessageRole::Agent,
            _ => {
                return Err(SessionError::InvalidArgument(format!(
                    "Unsupported message role: {}",
                    req.role
                )))
            }
        };

        let message = Message::new(conversation_id, role, req.content);
        self.store.append_message(&message).await?;
        self.refresh_manifest(&conversation.session_id, Some(conversation_id))
            .await?;

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
        let mut message = self
            .store
            .get_message(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Message {}", id)))?;
        let Some(content) = req.content else {
            return Ok(message);
        };
        if matches!(message.status, MessageStatus::Done | MessageStatus::Failed) {
            return Err(SessionError::InvalidArgument(format!(
                "Message {} content cannot change in status {:?}",
                message.id, message.status
            )));
        }
        let conversation = self.get_conversation(&message.conversation_id).await?;
        let session = self.get_session(&conversation.session_id).await?;
        Self::ensure_session_mutable(&session)?;

        message.update_content(content);

        self.store.update_message(&message).await?;
        self.refresh_manifest(&conversation.session_id, Some(conversation.id))
            .await?;

        self.event_bus.publish(SessionEvent::MessageUpdated {
            message: message.clone(),
            timestamp: chrono::Utc::now(),
        });

        Ok(message)
    }

    /// 更新流式 Message 状态。
    pub async fn update_message_status(
        &self,
        id: &MessageId,
        status: MessageStatus,
    ) -> SessionResult<Message> {
        let mut message = self
            .store
            .get_message(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Message {}", id)))?;
        let conversation = self.get_conversation(&message.conversation_id).await?;
        let session = self.get_session(&conversation.session_id).await?;
        Self::ensure_session_mutable(&session)?;
        message.transition_to(status)?;
        self.store.update_message(&message).await?;
        self.refresh_manifest(&conversation.session_id, Some(conversation.id))
            .await?;

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
        self.store
            .list_messages(conversation_id, offset, limit)
            .await
    }

    /// 删除 Message
    pub async fn delete_message(&self, id: &MessageId) -> SessionResult<()> {
        let message = self
            .store
            .get_message(id)
            .await?
            .ok_or_else(|| SessionError::NotFound(format!("Message {}", id)))?;
        let conversation = self.get_conversation(&message.conversation_id).await?;
        let session = self.get_session(&conversation.session_id).await?;
        Self::ensure_session_mutable(&session)?;

        self.store.delete_message(id).await?;
        self.refresh_manifest(&conversation.session_id, Some(conversation.id))
            .await?;

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
    pub async fn list_manifests(
        &self,
        offset: u64,
        limit: u64,
    ) -> SessionResult<(Vec<Manifest>, u64)> {
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
        let name = name.trim();
        if name.is_empty() {
            return Err(SessionError::InvalidArgument(
                "Attachment name must not be empty".into(),
            ));
        }
        let mut att = Attachment::new(attachment_type, name);

        if message_id.is_none() && session_id.is_none() {
            return Err(SessionError::InvalidArgument(
                "Attachment must belong to a message or session".into(),
            ));
        }

        if let Some(mid) = message_id {
            let message = self
                .store
                .get_message(&mid)
                .await?
                .ok_or_else(|| SessionError::NotFound(format!("Message {}", mid)))?;
            let conversation = self.get_conversation(&message.conversation_id).await?;
            let parent_session = self.get_session(&conversation.session_id).await?;
            Self::ensure_session_mutable(&parent_session)?;
            if let Some(sid) = session_id {
                if conversation.session_id != sid {
                    return Err(SessionError::InvalidArgument(
                        "Attachment message does not belong to the supplied session".into(),
                    ));
                }
            }
            att.attach_to_message(mid);
        }
        if let Some(sid) = session_id {
            let session = self.get_session(&sid).await?;
            Self::ensure_session_mutable(&session)?;
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

    async fn transition_session(
        &self,
        id: &SessionId,
        target: SessionState,
    ) -> SessionResult<Session> {
        let mut session = self.get_session(id).await?;
        let previous = session.state;
        self.lifecycle.before_transition(&session, target).await?;
        session.transition_to(target)?;
        self.store.update_session(&session).await?;
        self.sync_manifest_with_session(&session).await?;
        self.lifecycle.after_transition(&session, previous).await;

        self.event_bus.publish(SessionEvent::SessionStateChanged {
            session_id: session.id,
            old_state: previous,
            new_state: target,
            timestamp: chrono::Utc::now(),
        });

        Ok(session)
    }

    async fn sync_manifest_with_session(&self, session: &Session) -> SessionResult<()> {
        let mut manifest = self
            .store
            .get_manifest(&session.id)
            .await?
            .unwrap_or_else(|| Manifest::from_session(session));
        manifest.sync_session(session);
        self.store.upsert_manifest(&manifest).await?;
        self.event_bus.publish(SessionEvent::ManifestUpdated {
            manifest,
            timestamp: chrono::Utc::now(),
        });
        Ok(())
    }

    async fn refresh_manifest(
        &self,
        session_id: &SessionId,
        last_conversation_id: Option<ConversationId>,
    ) -> SessionResult<()> {
        let mut session = self.get_session(session_id).await?;
        session.touch();
        self.store.update_session(&session).await?;

        let conversation_count = self.store.list_conversations(session_id).await?.len();
        let message_count = self.store.count_messages_for_session(session_id).await?;
        let mut manifest = self
            .store
            .get_manifest(session_id)
            .await?
            .unwrap_or_else(|| Manifest::from_session(&session));
        let token_count = manifest.token_count;
        manifest.sync_session(&session);
        manifest.update_stats(
            u32::try_from(conversation_count).unwrap_or(u32::MAX),
            u32::try_from(message_count).unwrap_or(u32::MAX),
            token_count,
        );
        manifest.last_conversation_id = last_conversation_id;
        self.store.upsert_manifest(&manifest).await?;
        self.event_bus.publish(SessionEvent::ManifestUpdated {
            manifest,
            timestamp: chrono::Utc::now(),
        });
        Ok(())
    }

    fn ensure_session_mutable(session: &Session) -> SessionResult<()> {
        if matches!(
            session.state,
            SessionState::Archived | SessionState::Deleted
        ) {
            return Err(SessionError::InvalidArgument(format!(
                "Session {} is not mutable in state {:?}",
                session.id, session.state
            )));
        }
        Ok(())
    }
}
