//! Session Runtime — Domain Layer
//!
//! 核心实体与值对象定义。整个 Agent Runtime 的第一层。

pub mod session;
pub mod conversation;
pub mod message;
pub mod attachment;
pub mod metadata;
pub mod manifest;

pub use session::{Session, SessionId, SessionState};
pub use conversation::{Conversation, ConversationId, ConversationType};
pub use message::{Message, MessageId, MessageRole, MessageStatus};
pub use attachment::{Attachment, AttachmentId, AttachmentType};
pub use metadata::Metadata;
pub use manifest::{Manifest, ManifestId};