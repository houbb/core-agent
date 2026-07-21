use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApprovalError {
    #[error("approval validation failed: {0}")]
    Validation(String),
    #[error("approval not found: {0}")]
    NotFound(String),
    #[error("approval state conflict: {0}")]
    Conflict(String),
    #[error("approval authorization denied: {0}")]
    Denied(String),
    #[error("approval database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("approval serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("approval internal failure: {0}")]
    Internal(String),
}

impl ApprovalError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Denied(_) => "DENIED",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type ApprovalResult<T> = Result<T, ApprovalError>;