use thiserror::Error;

#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("semantic validation failed: {0}")]
    Validation(String),
    #[error("semantic not found: {0}")]
    NotFound(String),
    #[error("semantic conflict: {0}")]
    Conflict(String),
    #[error("semantic extraction failed: {0}")]
    ExtractionFailed(String),
    #[error("semantic database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("semantic serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("semantic internal failure: {0}")]
    Internal(String),
}

impl SemanticError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::ExtractionFailed(_) => "EXTRACTION_FAILED",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type SemanticResult<T> = Result<T, SemanticError>;