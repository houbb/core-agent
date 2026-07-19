//! Attachment 实体
//!
//! 统一处理图片、文件、日志、Diff、Terminal 输出、PDF 等。
//! Message 引用 Attachment，而不是 Blob。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::metadata::Metadata;

/// Attachment 唯一标识
pub type AttachmentId = Uuid;

/// Attachment 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentType {
    /// 图片
    Image,
    /// 文件
    File,
    /// 日志
    Log,
    /// 代码差异
    Diff,
    /// 终端输出
    Terminal,
    /// PDF 文档
    Pdf,
    /// 其他
    Other,
}

impl AttachmentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttachmentType::Image => "IMAGE",
            AttachmentType::File => "FILE",
            AttachmentType::Log => "LOG",
            AttachmentType::Diff => "DIFF",
            AttachmentType::Terminal => "TERMINAL",
            AttachmentType::Pdf => "PDF",
            AttachmentType::Other => "OTHER",
        }
    }
}

/// Attachment 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    /// 唯一标识
    pub id: AttachmentId,
    /// 关联的 Message ID
    pub message_id: Option<super::message::MessageId>,
    /// 关联的 Session ID
    pub session_id: Option<super::session::SessionId>,
    /// 附件类型
    pub attachment_type: AttachmentType,
    /// 文件名或标识名
    pub name: String,
    /// MIME 类型
    pub mime_type: Option<String>,
    /// 文件大小（字节）
    pub size_bytes: Option<u64>,
    /// 存储路径或 URL
    pub storage_path: Option<String>,
    /// 内容（小文件可直接存储）
    pub content: Option<Vec<u8>>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 扩展元数据
    pub metadata: Metadata,
}

impl Attachment {
    /// 创建新附件
    pub fn new(attachment_type: AttachmentType, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            message_id: None,
            session_id: None,
            attachment_type,
            name: name.into(),
            mime_type: None,
            size_bytes: None,
            storage_path: None,
            content: None,
            created_at: Utc::now(),
            metadata: Metadata::default(),
        }
    }

    /// 关联到 Message
    pub fn attach_to_message(&mut self, message_id: super::message::MessageId) {
        self.message_id = Some(message_id);
    }

    /// 关联到 Session
    pub fn attach_to_session(&mut self, session_id: super::session::SessionId) {
        self.session_id = Some(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_attachment() {
        let att = Attachment::new(AttachmentType::File, "report.pdf");

        assert_eq!(att.attachment_type, AttachmentType::File);
        assert_eq!(att.name, "report.pdf");
        assert!(att.message_id.is_none());
    }

    #[test]
    fn test_attach_to_message() {
        let mut att = Attachment::new(AttachmentType::Image, "screenshot.png");
        let msg_id = Uuid::new_v4();
        att.attach_to_message(msg_id);

        assert_eq!(att.message_id, Some(msg_id));
    }
}
