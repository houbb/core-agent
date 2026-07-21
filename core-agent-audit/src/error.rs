use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuditError {
    #[error("audit validation failed: {0}")]
    Validation(String),
    #[error("audit event not found: {0}")]
    NotFound(String),
    #[error("audit state conflict: {0}")]
    Conflict(String),
    #[error("audit database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("audit serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("audit internal failure: {0}")]
    Internal(String),
}

impl AuditError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type AuditResult<T> = Result<T, AuditError>;