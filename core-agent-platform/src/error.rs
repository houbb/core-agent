use uuid::Uuid;

pub type PlatformResult<T> = Result<T, PlatformError>;

#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("state conflict: {0}")]
    Conflict(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("authorization denied: {0}")]
    Denied(String),
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),
    #[error("platform extension failed: {0}")]
    Extension(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl PlatformError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}
