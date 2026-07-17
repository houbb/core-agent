//! DTO 层 — 输入输出数据传输对象
//!
//! 隔离领域模型与外部 API，保证领域模型可以独立演进。

use serde::{Deserialize, Serialize};

// ── Session DTOs ──

/// 创建 Session 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    /// 会话标题
    pub title: String,
    /// 会话描述
    pub description: Option<String>,
    /// 所有者
    pub owner: Option<String>,
    /// 工作空间 ID
    pub workspace_id: Option<String>,
}

/// 更新 Session 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionRequest {
    /// 会话标题
    pub title: Option<String>,
    /// 会话描述
    pub description: Option<String>,
    /// 所有者
    pub owner: Option<String>,
    /// 工作空间 ID
    pub workspace_id: Option<String>,
}

/// Session 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    /// 唯一标识
    pub id: String,
    /// 会话标题
    pub title: String,
    /// 会话描述
    pub description: Option<String>,
    /// 当前状态
    pub state: String,
    /// 创建时间 (ISO 8601)
    pub created_at: String,
    /// 更新时间 (ISO 8601)
    pub updated_at: String,
    /// 最后活跃时间 (ISO 8601)
    pub last_active_at: String,
    /// 所有者
    pub owner: Option<String>,
    /// 工作空间 ID
    pub workspace_id: Option<String>,
}

// ── Conversation DTOs ──

/// 创建 Conversation 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateConversationRequest {
    /// 所属 Session ID
    pub session_id: String,
    /// Conversation 类型（MVP 默认 MAIN）
    #[serde(default = "default_conversation_type")]
    pub conversation_type: String,
    /// Conversation 名称
    pub name: Option<String>,
}

fn default_conversation_type() -> String {
    "MAIN".to_string()
}

/// Conversation 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResponse {
    /// 唯一标识
    pub id: String,
    /// 所属 Session ID
    pub session_id: String,
    /// Conversation 类型
    pub conversation_type: String,
    /// Conversation 名称
    pub name: Option<String>,
    /// 创建时间 (ISO 8601)
    pub created_at: String,
}

// ── Message DTOs ──

/// 追加 Message 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendMessageRequest {
    /// 所属 Conversation ID
    pub conversation_id: String,
    /// 消息角色
    pub role: String,
    /// 消息内容
    pub content: String,
}

/// 更新 Message 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMessageRequest {
    /// 消息内容
    pub content: Option<String>,
}

/// Message 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    /// 唯一标识
    pub id: String,
    /// 所属 Conversation ID
    pub conversation_id: String,
    /// 消息角色
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 消息状态
    pub status: String,
    /// 创建时间 (ISO 8601)
    pub created_at: String,
}

// ── Manifest DTOs ──

/// Manifest 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestResponse {
    /// Session ID
    pub session_id: String,
    /// Session 名称
    pub name: String,
    /// 当前模型
    pub model: Option<String>,
    /// Workspace 路径
    pub workspace_path: Option<String>,
    /// 标签
    pub tags: Vec<String>,
    /// 当前状态
    pub state: String,
    /// 最后活跃时间 (ISO 8601)
    pub last_active_at: String,
    /// Conversation 数量
    pub conversation_count: u32,
    /// 消息总数
    pub message_count: u32,
    /// Token 总数
    pub token_count: Option<u64>,
    /// 创建时间 (ISO 8601)
    pub created_at: String,
    /// 更新时间 (ISO 8601)
    pub updated_at: String,
}

// ── 列表响应 ──

/// 分页列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    /// 数据列表
    pub items: Vec<T>,
    /// 总数
    pub total: u64,
    /// 偏移量
    pub offset: u64,
    /// 限制数
    pub limit: u64,
}

// ── 转换实现 ──

impl From<&crate::domain::session::Session> for SessionResponse {
    fn from(s: &crate::domain::session::Session) -> Self {
        Self {
            id: s.id.to_string(),
            title: s.title.clone(),
            description: s.description.clone(),
            state: format!("{:?}", s.state).to_uppercase(),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            last_active_at: s.last_active_at.to_rfc3339(),
            owner: s.owner.clone(),
            workspace_id: s.workspace_id.clone(),
        }
    }
}

impl From<&crate::domain::conversation::Conversation> for ConversationResponse {
    fn from(c: &crate::domain::conversation::Conversation) -> Self {
        Self {
            id: c.id.to_string(),
            session_id: c.session_id.to_string(),
            conversation_type: c.conversation_type.as_str().to_string(),
            name: c.name.clone(),
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

impl From<&crate::domain::message::Message> for MessageResponse {
    fn from(m: &crate::domain::message::Message) -> Self {
        Self {
            id: m.id.to_string(),
            conversation_id: m.conversation_id.to_string(),
            role: m.role.as_str().to_string(),
            content: m.content.clone(),
            status: m.status.as_str().to_string(),
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

impl From<&crate::domain::manifest::Manifest> for ManifestResponse {
    fn from(m: &crate::domain::manifest::Manifest) -> Self {
        Self {
            session_id: m.session_id.to_string(),
            name: m.name.clone(),
            model: m.model.clone(),
            workspace_path: m.workspace_path.clone(),
            tags: m.tags.clone(),
            state: format!("{:?}", m.state).to_uppercase(),
            last_active_at: m.last_active_at.to_rfc3339(),
            conversation_count: m.conversation_count,
            message_count: m.message_count,
            token_count: m.token_count,
            created_at: m.created_at.to_rfc3339(),
            updated_at: m.updated_at.to_rfc3339(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::Session;

    #[test]
    fn test_session_to_response() {
        let session = Session::new("Test Session");
        let resp = SessionResponse::from(&session);

        assert_eq!(resp.title, "Test Session");
        assert_eq!(resp.state, "CREATED");
        assert!(!resp.id.is_empty());
    }

    #[test]
    fn test_create_session_request_serialization() {
        let req = CreateSessionRequest {
            title: "Java 重构".to_string(),
            description: Some("重构用户模块".to_string()),
            owner: None,
            workspace_id: Some("ws-001".to_string()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let restored: CreateSessionRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.title, "Java 重构");
        assert_eq!(restored.workspace_id, Some("ws-001".to_string()));
    }
}