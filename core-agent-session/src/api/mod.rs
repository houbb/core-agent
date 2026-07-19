//! API 层 — 公开 API 接口
//!
//! Session Runtime 的对外门面。所有外部调用者（CLI、Web、Desktop）都通过此层访问。

use std::sync::Arc;

use crate::application::SessionApplicationService;
use crate::domain::{
    attachment::{AttachmentId, AttachmentType},
    conversation::ConversationId,
    message::{MessageId, MessageStatus},
    session::SessionId,
};
use crate::dto::{
    AppendMessageRequest, ConversationResponse, CreateConversationRequest, CreateSessionRequest,
    ListResponse, ManifestResponse, MessageResponse, SessionResponse, UpdateMessageRequest,
    UpdateSessionRequest,
};
use crate::error::SessionResult;
use crate::event::EventBus;
use crate::infrastructure::{SessionLifecycle, SessionStore};

/// SessionRuntime — Session Runtime 公开 API
///
/// 这是整个 Session Runtime 的入口点。
///
/// # Example
///
/// ```ignore
/// use core_agent_session::SessionRuntime;
///
/// let runtime = SessionRuntime::new(store, event_bus);
///
/// // 创建 Workspace
/// let session = runtime.create_session(CreateSessionRequest {
///     title: "Java 重构".into(),
///     description: None,
///     owner: None,
///     workspace_id: None,
/// }).await?;
/// ```
pub struct SessionRuntime<S: SessionStore> {
    app: SessionApplicationService<S>,
    event_bus: Arc<EventBus>,
}

impl<S: SessionStore> SessionRuntime<S> {
    /// 创建 Session Runtime 实例
    pub fn new(store: Arc<S>, event_bus: Arc<EventBus>) -> Self {
        let app = SessionApplicationService::new(store, event_bus.clone());
        Self { app, event_bus }
    }

    /// 使用自定义生命周期钩子创建 Runtime。
    pub fn with_lifecycle(
        store: Arc<S>,
        event_bus: Arc<EventBus>,
        lifecycle: Arc<dyn SessionLifecycle>,
    ) -> Self {
        let app = SessionApplicationService::with_lifecycle(store, event_bus.clone(), lifecycle);
        Self { app, event_bus }
    }

    /// 获取事件总线引用（供外部订阅）
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    // ── Session API ──

    /// 创建 Session（对外叫 Workspace）
    pub async fn create_session(
        &self,
        req: CreateSessionRequest,
    ) -> SessionResult<SessionResponse> {
        let session = self.app.create_session(req).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 获取 Session
    pub async fn get_session(&self, id: &str) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.get_session(&sid).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 列出 Sessions
    pub async fn list_sessions(
        &self,
        offset: u64,
        limit: u64,
    ) -> SessionResult<ListResponse<SessionResponse>> {
        let (sessions, total) = self.app.list_sessions(offset, limit).await?;
        Ok(ListResponse {
            items: sessions.iter().map(SessionResponse::from).collect(),
            total,
            offset,
            limit,
        })
    }

    /// 更新 Session
    pub async fn update_session(
        &self,
        id: &str,
        req: UpdateSessionRequest,
    ) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.update_session(&sid, req).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 归档 Session
    pub async fn archive_session(&self, id: &str) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.archive_session(&sid).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 启动 Session。
    pub async fn start_session(&self, id: &str) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.start_session(&sid).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 暂停 Session。
    pub async fn pause_session(&self, id: &str) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.pause_session(&sid).await?;
        Ok(SessionResponse::from(&session))
    }

    /// 删除 Session
    pub async fn delete_session(&self, id: &str) -> SessionResult<()> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        self.app.delete_session(&sid).await
    }

    /// 恢复 Session
    pub async fn resume_session(&self, id: &str) -> SessionResult<SessionResponse> {
        let sid = SessionId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let session = self.app.resume_session(&sid).await?;
        Ok(SessionResponse::from(&session))
    }

    // ── Conversation API ──

    /// 创建 Conversation
    pub async fn create_conversation(
        &self,
        req: CreateConversationRequest,
    ) -> SessionResult<ConversationResponse> {
        let conv = self.app.create_conversation(req).await?;
        Ok(ConversationResponse::from(&conv))
    }

    /// 列出 Conversations
    pub async fn list_conversations(
        &self,
        session_id: &str,
    ) -> SessionResult<Vec<ConversationResponse>> {
        let sid = SessionId::parse_str(session_id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let convs = self.app.list_conversations(&sid).await?;
        Ok(convs.iter().map(ConversationResponse::from).collect())
    }

    /// 获取 Conversation
    pub async fn get_conversation(&self, id: &str) -> SessionResult<ConversationResponse> {
        let cid = ConversationId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid conversation id".into())
        })?;
        let conv = self.app.get_conversation(&cid).await?;
        Ok(ConversationResponse::from(&conv))
    }

    // ── Message API ──

    /// 追加 Message
    pub async fn append_message(
        &self,
        req: AppendMessageRequest,
    ) -> SessionResult<MessageResponse> {
        let msg = self.app.append_message(req).await?;
        Ok(MessageResponse::from(&msg))
    }

    /// 更新 Message
    pub async fn update_message(
        &self,
        id: &str,
        req: UpdateMessageRequest,
    ) -> SessionResult<MessageResponse> {
        let mid = MessageId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid message id".into())
        })?;
        let msg = self.app.update_message(&mid, req).await?;
        Ok(MessageResponse::from(&msg))
    }

    /// 更新流式 Message 状态。
    pub async fn update_message_status(
        &self,
        id: &str,
        status: MessageStatus,
    ) -> SessionResult<MessageResponse> {
        let mid = MessageId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid message id".into())
        })?;
        let msg = self.app.update_message_status(&mid, status).await?;
        Ok(MessageResponse::from(&msg))
    }

    /// 列出 Messages
    pub async fn list_messages(
        &self,
        conversation_id: &str,
        offset: u64,
        limit: u64,
    ) -> SessionResult<ListResponse<MessageResponse>> {
        let cid = ConversationId::parse_str(conversation_id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid conversation id".into())
        })?;
        let (messages, total) = self.app.list_messages(&cid, offset, limit).await?;
        Ok(ListResponse {
            items: messages.iter().map(MessageResponse::from).collect(),
            total,
            offset,
            limit,
        })
    }

    /// 删除 Message
    pub async fn delete_message(&self, id: &str) -> SessionResult<()> {
        let mid = MessageId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid message id".into())
        })?;
        self.app.delete_message(&mid).await
    }

    // ── Manifest API ──

    /// 获取 Manifest
    pub async fn get_manifest(&self, session_id: &str) -> SessionResult<ManifestResponse> {
        let sid = SessionId::parse_str(session_id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid session id".into())
        })?;
        let manifest = self.app.get_manifest(&sid).await?;
        Ok(ManifestResponse::from(&manifest))
    }

    /// 列出 Manifests
    pub async fn list_manifests(
        &self,
        offset: u64,
        limit: u64,
    ) -> SessionResult<ListResponse<ManifestResponse>> {
        let (manifests, total) = self.app.list_manifests(offset, limit).await?;
        Ok(ListResponse {
            items: manifests.iter().map(ManifestResponse::from).collect(),
            total,
            offset,
            limit,
        })
    }

    // ── Attachment API ──

    /// 添加 Attachment
    pub async fn add_attachment(
        &self,
        attachment_type: AttachmentType,
        name: String,
        message_id: Option<String>,
        session_id: Option<String>,
    ) -> SessionResult<String> {
        let mid = message_id
            .map(|s| MessageId::parse_str(&s))
            .transpose()
            .map_err(|_| {
                crate::error::SessionError::InvalidArgument("Invalid message id".into())
            })?;
        let sid = session_id
            .map(|s| SessionId::parse_str(&s))
            .transpose()
            .map_err(|_| {
                crate::error::SessionError::InvalidArgument("Invalid session id".into())
            })?;

        let att = self
            .app
            .add_attachment(attachment_type, name, mid, sid)
            .await?;
        Ok(att.id.to_string())
    }

    /// 获取 Attachment
    pub async fn get_attachment(
        &self,
        id: &str,
    ) -> SessionResult<crate::domain::attachment::Attachment> {
        let aid = AttachmentId::parse_str(id).map_err(|_| {
            crate::error::SessionError::InvalidArgument("Invalid attachment id".into())
        })?;
        self.app.get_attachment(&aid).await
    }
}
