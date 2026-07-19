use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("workspace not found: {0}")]
    NotFound(String),
    #[error("workspace conflict: {0}")]
    Conflict(String),
    #[error("invalid workspace lifecycle transition: {0}")]
    InvalidState(String),
    #[error("unsupported workspace URI: {0}")]
    UnsupportedUri(String),
    #[error("workspace provider not found: {0}")]
    ProviderNotFound(String),
    #[error("workspace policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("workspace limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("workspace I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("workspace database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("workspace database pool failed: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("workspace serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("workspace internal failure: {0}")]
    Internal(String),
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;
