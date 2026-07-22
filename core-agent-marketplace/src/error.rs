use thiserror::Error;

#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error("marketplace validation failed: {0}")]
    Validation(String),
    #[error("marketplace not found: {0}")]
    NotFound(String),
    #[error("marketplace conflict: {0}")]
    Conflict(String),
    #[error("marketplace database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("marketplace serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("marketplace internal failure: {0}")]
    Internal(String),
}

impl MarketplaceError {
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

pub type MarketplaceResult<T> = Result<T, MarketplaceError>;