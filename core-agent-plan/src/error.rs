use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("planning validation failed: {0}")]
    Validation(String),
    #[error("planning entity not found: {0}")]
    NotFound(String),
    #[error("planning version conflict: {0}")]
    Conflict(String),
    #[error("invalid planning lifecycle transition: {0}")]
    InvalidState(String),
    #[error("planning strategy or builder not found: {0}")]
    BuilderNotFound(String),
    #[error("planning policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("planning database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("planning database pool failed: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("planning serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("planning extension failed: {0}")]
    Extension(String),
    #[error("planning internal failure: {0}")]
    Internal(String),
}

pub type PlanResult<T> = Result<T, PlanError>;
