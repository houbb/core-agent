//! Session 实体 — Agent 生命周期管理器
//!
//! Session 不是聊天。它是 Agent 从出生到结束的整个生命周期。
//! 以后 Chat / Coding / Workflow / Multi-Agent 全部依赖 Session。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::metadata::Metadata;

/// Session 唯一标识
pub type SessionId = Uuid;

/// Session 生命周期状态
///
/// 状态流转：
/// ```text
/// CREATED → READY → RUNNING → PAUSED → ARCHIVED → DELETED
/// ```
///
/// 不允许跳跃（如 RUNNING → DELETED），保证生命周期可追溯。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionState {
    /// 刚创建，尚未就绪
    Created,
    /// 已就绪，可以开始运行
    Ready,
    /// 正在运行中
    Running,
    /// 已暂停（用户关闭电脑、中断等）
    Paused,
    /// 已归档
    Archived,
    /// 已删除（软删除）
    Deleted,
}

impl SessionState {
    /// 检查是否可以转换到目标状态
    pub fn can_transition_to(&self, target: &SessionState) -> bool {
        matches!(
            (self, target),
            (SessionState::Created, SessionState::Ready)
                | (SessionState::Ready, SessionState::Running)
                | (SessionState::Running, SessionState::Paused)
                | (SessionState::Running, SessionState::Archived)
                | (SessionState::Paused, SessionState::Running)
                | (SessionState::Paused, SessionState::Archived)
                | (SessionState::Archived, SessionState::Deleted)
        )
    }

    /// 是否为终态（不可再转换）
    pub fn is_terminal(&self) -> bool {
        matches!(self, SessionState::Deleted)
    }

    /// 是否为活跃状态
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            SessionState::Ready | SessionState::Running | SessionState::Paused
        )
    }
}

/// Session 实体
///
/// Session 是 Agent 生命周期的载体。它包含 Conversation 列表，
/// 但不在 Session 实体内部直接持有 Message。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// 唯一标识
    pub id: SessionId,
    /// 会话标题
    pub title: String,
    /// 会话描述
    pub description: Option<String>,
    /// 当前状态
    pub state: SessionState,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 最后活跃时间
    pub last_active_at: DateTime<Utc>,
    /// 所有者
    pub owner: Option<String>,
    /// 工作空间 ID
    pub workspace_id: Option<String>,
    /// 扩展元数据
    pub metadata: Metadata,
}

impl Session {
    /// 创建新 Session
    pub fn new(title: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            state: SessionState::Created,
            created_at: now,
            updated_at: now,
            last_active_at: now,
            owner: None,
            workspace_id: None,
            metadata: Metadata::default(),
        }
    }

    /// 状态转换
    ///
    /// 返回 Ok(()) 表示转换成功，Err 表示非法转换。
    pub fn transition_to(&mut self, target: SessionState) -> Result<(), SessionStateError> {
        if !self.state.can_transition_to(&target) {
            return Err(SessionStateError {
                current: self.state,
                target,
            });
        }
        self.state = target;
        self.updated_at = Utc::now();
        if target.is_active() {
            self.last_active_at = Utc::now();
        }
        Ok(())
    }

    /// 更新标题
    pub fn update_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        self.updated_at = Utc::now();
    }

    /// 更新描述
    pub fn update_description(&mut self, description: impl Into<String>) {
        self.description = Some(description.into());
        self.updated_at = Utc::now();
    }

    /// 标记为活跃
    pub fn touch(&mut self) {
        let now = Utc::now();
        self.last_active_at = now;
        self.updated_at = now;
    }
}

/// 非法状态转换错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStateError {
    pub current: SessionState,
    pub target: SessionState,
}

impl std::fmt::Display for SessionStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Illegal state transition: {:?} → {:?}",
            self.current, self.target
        )
    }
}

impl std::error::Error for SessionStateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_is_created() {
        let session = Session::new("Test Session");
        assert_eq!(session.state, SessionState::Created);
        assert!(!session.id.is_nil());
    }

    #[test]
    fn test_valid_state_transitions() {
        let mut session = Session::new("Test");

        // Created → Ready
        assert!(session.transition_to(SessionState::Ready).is_ok());
        assert_eq!(session.state, SessionState::Ready);

        // Ready → Running
        assert!(session.transition_to(SessionState::Running).is_ok());

        // Running → Paused
        assert!(session.transition_to(SessionState::Paused).is_ok());

        // Paused → Running (恢复)
        assert!(session.transition_to(SessionState::Running).is_ok());

        // Running → Archived
        assert!(session.transition_to(SessionState::Archived).is_ok());

        // Archived → Deleted
        assert!(session.transition_to(SessionState::Deleted).is_ok());
    }

    #[test]
    fn test_invalid_state_transitions() {
        let mut session = Session::new("Test");

        // Created → Running (跳过 Ready) — 非法
        assert!(session.transition_to(SessionState::Running).is_err());

        // Created → Deleted (跳跃) — 非法
        assert!(session.transition_to(SessionState::Deleted).is_err());

        // 先到 Ready
        session.transition_to(SessionState::Ready).unwrap();
        session.transition_to(SessionState::Running).unwrap();

        // Running → Deleted (跳跃) — 非法
        assert!(session.transition_to(SessionState::Deleted).is_err());
    }

    #[test]
    fn test_terminal_state_no_transition() {
        let mut session = Session::new("Test");
        session.transition_to(SessionState::Ready).unwrap();
        session.transition_to(SessionState::Running).unwrap();
        session.transition_to(SessionState::Archived).unwrap();
        session.transition_to(SessionState::Deleted).unwrap();

        assert!(session.state.is_terminal());

        // Deleted 状态下不可再转换
        assert!(session.transition_to(SessionState::Archived).is_err());
    }

    #[test]
    fn test_archive_requires_running_or_paused() {
        let mut session = Session::new("Test");
        session.transition_to(SessionState::Ready).unwrap();

        assert!(session.transition_to(SessionState::Archived).is_err());

        session.transition_to(SessionState::Running).unwrap();
        assert!(session.transition_to(SessionState::Archived).is_ok());
    }
}
