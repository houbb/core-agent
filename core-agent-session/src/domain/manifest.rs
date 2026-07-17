//! Manifest 实体 — Session 概要快照
//!
//! Manifest 不保存聊天内容，而是保存整个 Session 的概要信息。
//! 这样左侧 Session 列表无需加载全部 Message，桌面端启动更快。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::session::{SessionId, SessionState};

/// Manifest 唯一标识
pub type ManifestId = Uuid;

/// Session Manifest（会话清单）
///
/// 保存 Session 的概要信息，用于快速列表展示和切换。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// 唯一标识（与 Session ID 相同）
    pub id: ManifestId,
    /// 对应的 Session ID
    pub session_id: SessionId,
    /// Session 名称
    pub name: String,
    /// 当前使用的模型
    pub model: Option<String>,
    /// Workspace 路径
    pub workspace_path: Option<String>,
    /// 标签列表
    pub tags: Vec<String>,
    /// 当前状态
    pub state: SessionState,
    /// 最后活跃时间
    pub last_active_at: DateTime<Utc>,
    /// Conversation 数量
    pub conversation_count: u32,
    /// 消息总数
    pub message_count: u32,
    /// Token 总数（估算）
    pub token_count: Option<u64>,
    /// 最近打开的 Conversation ID
    pub last_conversation_id: Option<super::conversation::ConversationId>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
}

impl Manifest {
    /// 从 Session 创建 Manifest
    pub fn from_session(session: &super::session::Session) -> Self {
        let now = Utc::now();
        Self {
            id: session.id,
            session_id: session.id,
            name: session.title.clone(),
            model: session.metadata.get("model"),
            workspace_path: session.metadata.get("workspace_path"),
            tags: session
                .metadata
                .get::<Vec<String>>("tags")
                .unwrap_or_default(),
            state: session.state,
            last_active_at: session.last_active_at,
            conversation_count: 0,
            message_count: 0,
            token_count: None,
            last_conversation_id: None,
            created_at: session.created_at,
            updated_at: now,
        }
    }

    /// 更新统计信息
    pub fn update_stats(&mut self, conversation_count: u32, message_count: u32, token_count: Option<u64>) {
        self.conversation_count = conversation_count;
        self.message_count = message_count;
        self.token_count = token_count;
        self.updated_at = Utc::now();
    }

    /// 更新最近活跃
    pub fn touch(&mut self, last_conversation_id: Option<super::conversation::ConversationId>) {
        self.last_active_at = Utc::now();
        self.last_conversation_id = last_conversation_id;
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::Session;

    #[test]
    fn test_manifest_from_session() {
        let session = Session::new("Java 重构");
        let manifest = Manifest::from_session(&session);

        assert_eq!(manifest.session_id, session.id);
        assert_eq!(manifest.name, "Java 重构");
        assert_eq!(manifest.state, session.state);
        assert_eq!(manifest.conversation_count, 0);
        assert_eq!(manifest.message_count, 0);
    }

    #[test]
    fn test_manifest_update_stats() {
        let session = Session::new("Test");
        let mut manifest = Manifest::from_session(&session);

        manifest.update_stats(3, 150, Some(50000));

        assert_eq!(manifest.conversation_count, 3);
        assert_eq!(manifest.message_count, 150);
        assert_eq!(manifest.token_count, Some(50000));
    }
}