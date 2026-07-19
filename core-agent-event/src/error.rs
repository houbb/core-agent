use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("event validation failed: {0}")]
    Validation(String),
    #[error("event entity not found: {0}")]
    NotFound(String),
    #[error("event version conflict: {0}")]
    Conflict(String),
    #[error("invalid event lifecycle transition: {0}")]
    InvalidState(String),
    #[error("event policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("event handler failed: {0}")]
    Handler(String),
    #[error("event extension failed: {0}")]
    Extension(String),
    #[error("event database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("event serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("event internal failure: {0}")]
    Internal(String),
}

impl EventError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::PolicyDenied(_) => "POLICY_DENIED",
            Self::Handler(_) => "HANDLER",
            Self::Extension(_) => "EXTENSION",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type EventResult<T> = Result<T, EventError>;
