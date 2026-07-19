use uuid::Uuid;

pub type ExtensionResult<T> = Result<T, ExtensionError>;

#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("state conflict: {0}")]
    Conflict(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("extension policy denied: {0}")]
    Denied(String),
    #[error("extension outcome is unknown: {0}")]
    OutcomeUnknown(String),
    #[error("extension host failed: {0}")]
    Host(String),
    #[error("extension loader failed: {0}")]
    Loader(String),
    #[error("extension hook failed: {0}")]
    Extension(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

impl ExtensionError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}
