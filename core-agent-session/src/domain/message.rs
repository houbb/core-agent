//! Message 实体
//!
//! Message 只负责保存内容。不保存推理，不保存 Tool。
//! Tool 以后由 Tool Runtime 单独管理。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::metadata::Metadata;

/// Message 唯一标识
pub type MessageId = Uuid;

/// Message 角色
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    /// 系统消息
    System,
    /// 用户消息
    User,
    /// 助手消息
    Assistant,
    /// 工具消息
    Tool,
    /// Agent 消息（以后 Multi-Agent 使用）
    Agent,
}

impl MessageRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageRole::System => "SYSTEM",
            MessageRole::User => "USER",
            MessageRole::Assistant => "ASSISTANT",
            MessageRole::Tool => "TOOL",
            MessageRole::Agent => "AGENT",
        }
    }
}

/// Message 状态
///
/// 支持 Streaming 场景：PENDING → STREAMING → DONE
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageStatus {
    /// 待发送
    Pending,
    /// 流式传输中
    Streaming,
    /// 已完成
    Done,
    /// 发送失败
    Failed,
}

impl MessageStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageStatus::Pending => "PENDING",
            MessageStatus::Streaming => "STREAMING",
            MessageStatus::Done => "DONE",
            MessageStatus::Failed => "FAILED",
        }
    }
}

/// Message 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// 唯一标识
    pub id: MessageId,
    /// 所属 Conversation ID
    pub conversation_id: super::conversation::ConversationId,
    /// 消息角色
    pub role: MessageRole,
    /// 消息内容
    pub content: String,
    /// 消息状态
    pub status: MessageStatus,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 扩展元数据
    pub metadata: Metadata,
}

impl Message {
    /// 创建新消息
    pub fn new(
        conversation_id: super::conversation::ConversationId,
        role: MessageRole,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            role,
            content: content.into(),
            status: MessageStatus::Pending,
            created_at: Utc::now(),
            metadata: Metadata::default(),
        }
    }

    /// 更新消息内容
    pub fn update_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
    }

    /// 更新消息状态
    pub fn update_status(&mut self, status: MessageStatus) {
        self.status = status;
    }

    /// 标记为完成
    pub fn mark_done(&mut self) {
        self.status = MessageStatus::Done;
    }

    /// 标记为失败
    pub fn mark_failed(&mut self) {
        self.status = MessageStatus::Failed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_new_message() {
        let conv_id = Uuid::new_v4();
        let msg = Message::new(conv_id, MessageRole::User, "Hello");

        assert_eq!(msg.conversation_id, conv_id);
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert_eq!(msg.status, MessageStatus::Pending);
    }

    #[test]
    fn test_message_status_flow() {
        let conv_id = Uuid::new_v4();
        let mut msg = Message::new(conv_id, MessageRole::Assistant, "Thinking...");

        msg.update_status(MessageStatus::Streaming);
        assert_eq!(msg.status, MessageStatus::Streaming);

        msg.mark_done();
        assert_eq!(msg.status, MessageStatus::Done);
    }

    #[test]
    fn test_message_role_as_str() {
        assert_eq!(MessageRole::System.as_str(), "SYSTEM");
        assert_eq!(MessageRole::User.as_str(), "USER");
        assert_eq!(MessageRole::Assistant.as_str(), "ASSISTANT");
    }
}