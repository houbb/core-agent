//! Infrastructure 层 — 扩展点定义
//!
//! Session Runtime 的扩展点，定义稳定的 trait 接口。
//! 企业版只需要新增实现，不需要修改核心代码。

pub mod lifecycle;
pub mod observer;
pub mod serializer;

use async_trait::async_trait;

use crate::domain::{
    attachment::{Attachment, AttachmentId},
    conversation::{Conversation, ConversationId},
    manifest::Manifest,
    message::{Message, MessageId},
    session::{Session, SessionId},
};
use crate::error::SessionResult;

pub use lifecycle::{NoopSessionLifecycle, SessionLifecycle};
pub use observer::SessionObserver;
pub use serializer::{JsonSessionSerializer, SessionSerializer};

/// SessionStore — 持久化存储接口
///
/// 实现：SQLite、PostgreSQL、云端存储
#[async_trait]
pub trait SessionStore: Send + Sync {
    // ── Session ──
    async fn create_session(&self, session: &Session) -> SessionResult<()>;
    async fn get_session(&self, id: &SessionId) -> SessionResult<Option<Session>>;
    async fn list_sessions(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Session>, u64)>;
    async fn update_session(&self, session: &Session) -> SessionResult<()>;
    async fn delete_session(&self, id: &SessionId) -> SessionResult<()>;

    // ── Conversation ──
    async fn create_conversation(&self, conversation: &Conversation) -> SessionResult<()>;
    async fn get_conversation(&self, id: &ConversationId) -> SessionResult<Option<Conversation>>;
    async fn list_conversations(&self, session_id: &SessionId) -> SessionResult<Vec<Conversation>>;

    // ── Message ──
    async fn append_message(&self, message: &Message) -> SessionResult<()>;
    async fn get_message(&self, id: &MessageId) -> SessionResult<Option<Message>>;
    async fn update_message(&self, message: &Message) -> SessionResult<()>;
    async fn list_messages(
        &self,
        conversation_id: &ConversationId,
        offset: u64,
        limit: u64,
    ) -> SessionResult<(Vec<Message>, u64)>;
    async fn delete_message(&self, id: &MessageId) -> SessionResult<()>;

    // ── Manifest ──
    async fn upsert_manifest(&self, manifest: &Manifest) -> SessionResult<()>;
    async fn get_manifest(&self, session_id: &SessionId) -> SessionResult<Option<Manifest>>;
    async fn list_manifests(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Manifest>, u64)>;

    // ── Attachment ──
    async fn create_attachment(&self, attachment: &Attachment) -> SessionResult<()>;
    async fn get_attachment(&self, id: &AttachmentId) -> SessionResult<Option<Attachment>>;

    /// 原子创建 Session、Manifest 和默认 Conversation。
    ///
    /// 默认实现保持第三方 Store 源码兼容；支持事务的 Store 应覆盖此方法。
    async fn create_session_bundle(
        &self,
        session: &Session,
        manifest: &Manifest,
        conversation: &Conversation,
    ) -> SessionResult<()> {
        self.create_session(session).await?;
        self.upsert_manifest(manifest).await?;
        self.create_conversation(conversation).await
    }

    /// 统计 Session 下全部 Message，供 Manifest 同步使用。
    async fn count_messages_for_session(&self, session_id: &SessionId) -> SessionResult<u64> {
        let conversations = self.list_conversations(session_id).await?;
        let mut total = 0u64;
        for conversation in conversations {
            let (_, count) = self.list_messages(&conversation.id, 0, 1).await?;
            total = total.saturating_add(count);
        }
        Ok(total)
    }
}
