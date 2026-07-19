use uuid::Uuid;

pub type MultiAgentResult<T> = Result<T, MultiAgentError>;

#[derive(Debug, thiserror::Error)]
pub enum MultiAgentError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("state conflict: {0}")]
    Conflict(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("operation denied: {0}")]
    Denied(String),
    #[error("no eligible Agent member: {0}")]
    NoRoute(String),
    #[error("external Agent outcome is unknown: {0}")]
    OutcomeUnknown(String),
    #[error("extension failed: {0}")]
    Extension(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl MultiAgentError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}
