use uuid::Uuid;

pub type SubAgentResult<T> = Result<T, SubAgentError>;

#[derive(Debug, thiserror::Error)]
pub enum SubAgentError {
    #[error("subagent validation failed: {0}")]
    Validation(String),

    #[error("subagent not found: {0}")]
    NotFound(String),

    #[error("subagent version conflict: {0}")]
    Conflict(String),

    #[error("invalid subagent lifecycle transition: {0}")]
    InvalidState(String),

    #[error("subagent database failed: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("subagent serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("subagent internal failure: {0}")]
    Internal(String),
}

impl SubAgentError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}