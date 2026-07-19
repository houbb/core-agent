//! 统一错误类型
//!
//! Context Runtime 所有错误通过此模块定义。

/// Context Runtime 错误类型
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// 未找到
    #[error("Not found: {0}")]
    NotFound(String),

    /// 非法参数
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Token 预算超出
    #[error("Token budget exceeded: {0}")]
    TokenBudgetExceeded(String),

    /// 持久化错误
    #[error("Persistence error: {0}")]
    Persistence(String),

    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// 内部错误
    #[error("Internal error: {0}")]
    Internal(String),

    /// Session Runtime 错误
    #[error("Session error: {0}")]
    Session(String),
}

impl From<core_agent_session::SessionError> for ContextError {
    fn from(e: core_agent_session::SessionError) -> Self {
        ContextError::Session(e.to_string())
    }
}

/// Context Runtime Result 别名
pub type ContextResult<T> = Result<T, ContextError>;
