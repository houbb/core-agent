use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("network validation failed: {0}")]
    Validation(String),
    #[error("network agent not found: {0}")]
    NotFound(String),
    #[error("network state conflict: {0}")]
    Conflict(String),
    #[error("network database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("network serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("network internal failure: {0}")]
    Internal(String),
}

impl NetworkError {
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

pub type NetworkResult<T> = Result<T, NetworkError>;