//! Session Runtime — Domain Layer
//!
//! 核心实体与值对象定义。整个 Agent Runtime 的第一层。

pub mod attachment;
pub mod conversation;
pub mod manifest;
pub mod message;
pub mod metadata;
pub mod session;

pub use attachment::{Attachment, AttachmentId, AttachmentType};
pub use conversation::{Conversation, ConversationId, ConversationType};
pub use manifest::{Manifest, ManifestId};
pub use message::{Message, MessageId, MessageRole, MessageStatus, MessageStatusError};
pub use metadata::Metadata;
pub use session::{Session, SessionId, SessionState};
