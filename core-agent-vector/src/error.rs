use thiserror::Error;

#[derive(Debug, Error)]
pub enum VectorError {
    #[error("vector validation failed: {0}")]
    Validation(String),
    #[error("vector not found: {0}")]
    NotFound(String),
    #[error("vector dimension mismatch: {0}")]
    DimensionMismatch(String),
    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),
    #[error("vector database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("vector serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("vector internal failure: {0}")]
    Internal(String),
}

impl VectorError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::DimensionMismatch(_) => "DIMENSION_MISMATCH",
            Self::EmbeddingFailed(_) => "EMBEDDING_FAILED",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type VectorResult<T> = Result<T, VectorError>;