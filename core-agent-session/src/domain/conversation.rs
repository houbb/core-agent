//! Conversation 实体
//!
//! Conversation 属于 Session。一个 Session 可以有多个 Conversation。
//! 以后一个 Session 可能同时有 MAIN / PLAN / REVIEW / REFLECTION 等多个 Conversation。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Conversation 唯一标识
pub type ConversationId = Uuid;

/// Conversation 类型
///
/// MVP 只使用 MAIN，以后直接扩展 PLAN / REVIEW / SYSTEM / DEBUG。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationType {
    /// 主对话
    Main,
    /// 规划对话
    Plan,
    /// 审查对话
    Review,
    /// 系统对话
    System,
    /// 调试对话
    Debug,
}

impl ConversationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConversationType::Main => "MAIN",
            ConversationType::Plan => "PLAN",
            ConversationType::Review => "REVIEW",
            ConversationType::System => "SYSTEM",
            ConversationType::Debug => "DEBUG",
        }
    }
}

/// Conversation 实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// 唯一标识
    pub id: ConversationId,
    /// 所属 Session ID
    pub session_id: super::session::SessionId,
    /// Conversation 类型
    pub conversation_type: ConversationType,
    /// Conversation 名称
    pub name: Option<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl Conversation {
    /// 创建主对话
    pub fn new_main(session_id: super::session::SessionId) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            conversation_type: ConversationType::Main,
            name: None,
            created_at: Utc::now(),
        }
    }

    /// 创建指定类型的对话
    pub fn new(
        session_id: super::session::SessionId,
        conversation_type: ConversationType,
        name: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            conversation_type,
            name,
            created_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_new_main_conversation() {
        let session_id = Uuid::new_v4();
        let conv = Conversation::new_main(session_id);

        assert_eq!(conv.session_id, session_id);
        assert_eq!(conv.conversation_type, ConversationType::Main);
        assert!(!conv.id.is_nil());
    }

    #[test]
    fn test_conversation_type_as_str() {
        assert_eq!(ConversationType::Main.as_str(), "MAIN");
        assert_eq!(ConversationType::Plan.as_str(), "PLAN");
        assert_eq!(ConversationType::Review.as_str(), "REVIEW");
    }
}