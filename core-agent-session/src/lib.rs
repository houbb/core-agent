//! core-agent-session — Session Runtime
//!
//! Agent 生命周期管理器。负责 Session 从出生到结束的整个生命周期。
//!
//! # Architecture
//!
//! ```text
//! api/          — 公开 API（SessionRuntime）
//! application/  — 用例编排（SessionApplicationService）
//! domain/       — 核心实体（Session/Conversation/Message/Attachment/Manifest/Metadata）
//! infrastructure/ — 扩展点 trait（SessionStore）
//! persistence/  — SQLite 实现
//! dto/          — 输入输出 DTO
//! event/        — 事件系统（EventBus）
//! error/        — 统一错误类型
//! ```
//!
//! # Quick Start
//!
//! ```ignore
//! use core_agent_session::{
//!     SessionRuntime,
//!     SqliteSessionStore,
//!     EventBus,
//!     CreateSessionRequest,
//! };
//!
//! let store = SqliteSessionStore::new(":memory:").unwrap();
//! let event_bus = EventBus::default();
//! let runtime = SessionRuntime::new(store, event_bus);
//!
//! let session = runtime.create_session(CreateSessionRequest {
//!     title: "My Agent".into(),
//!     description: None,
//!     owner: None,
//!     workspace_id: None,
//! }).await?;
//! ```

pub mod api;
pub mod application;
pub mod domain;
pub mod dto;
pub mod error;
pub mod event;
pub mod infrastructure;
pub mod persistence;

// 重导出常用类型
pub use api::SessionRuntime;
pub use dto::{
    AppendMessageRequest, ConversationResponse, CreateConversationRequest,
    CreateSessionRequest, ListResponse, ManifestResponse, MessageResponse,
    SessionResponse, UpdateMessageRequest, UpdateSessionRequest,
};
pub use domain::{
    attachment::{Attachment, AttachmentId, AttachmentType},
    conversation::{Conversation, ConversationId, ConversationType},
    manifest::{Manifest, ManifestId},
    message::{Message, MessageId, MessageRole, MessageStatus},
    session::{Session, SessionId, SessionState},
    Metadata,
};
pub use error::{SessionError, SessionResult};
pub use event::{EventBus, SessionEvent};
pub use infrastructure::SessionStore;
pub use persistence::SqliteSessionStore;