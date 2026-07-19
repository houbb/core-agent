use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("execution validation failed: {0}")]
    Validation(String),
    #[error("execution entity not found: {0}")]
    NotFound(String),
    #[error("execution version conflict: {0}")]
    Conflict(String),
    #[error("invalid execution lifecycle transition: {0}")]
    InvalidState(String),
    #[error("execution outcome is unknown: {0}")]
    OutcomeUnknown(String),
    #[error("execution policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("execution command failed: {0}")]
    Command(String),
    #[error("execution database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("execution serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("execution extension failed: {0}")]
    Extension(String),
    #[error("execution internal failure: {0}")]
    Internal(String),
}

pub type ExecutionResult<T> = Result<T, ExecutionError>;
