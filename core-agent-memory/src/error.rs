use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("memory validation failed: {0}")]
    Validation(String),
    #[error("memory entity not found: {0}")]
    NotFound(String),
    #[error("memory version conflict: {0}")]
    Conflict(String),
    #[error("invalid memory lifecycle transition: {0}")]
    InvalidState(String),
    #[error("memory policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("memory extension failed: {0}")]
    Extension(String),
    #[error("memory database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("memory serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("memory internal failure: {0}")]
    Internal(String),
}

impl MemoryError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::PolicyDenied(_) => "POLICY_DENIED",
            Self::Extension(_) => "EXTENSION",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type MemoryResult<T> = Result<T, MemoryError>;
