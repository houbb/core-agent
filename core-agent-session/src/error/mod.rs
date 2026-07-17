//! 统一错误类型
//!
//! Session Runtime 所有错误通过此模块定义。

use crate::domain::session::SessionStateError;

/// Session Runtime 错误类型
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// 未找到
    #[error("Not found: {0}")]
    NotFound(String),

    /// 已存在
    #[error("Already exists: {0}")]
    AlreadyExists(String),

    /// 非法状态转换
    #[error("Invalid state transition: {0}")]
    InvalidState(#[from] SessionStateError),

    /// 非法参数
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// 持久化错误
    #[error("Persistence error: {0}")]
    Persistence(String),

    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// 内部错误
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Session Runtime Result 别名
pub type SessionResult<T> = Result<T, SessionError>;