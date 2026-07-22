use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("evaluation validation failed: {0}")]
    Validation(String),
    #[error("evaluation not found: {0}")]
    NotFound(String),
    #[error("evaluation state conflict: {0}")]
    Conflict(String),
    #[error("evaluation database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("evaluation serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("evaluation internal failure: {0}")]
    Internal(String),
}

impl EvaluationError {
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

pub type EvaluationResult<T> = Result<T, EvaluationError>;