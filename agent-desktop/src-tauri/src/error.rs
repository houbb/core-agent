pub type DesktopResult<T> = Result<T, DesktopError>;

#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("state conflict: {0}")]
    Conflict(String),
    #[error("desktop state not found: {0}")]
    NotFound(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("desktop internal error: {0}")]
    Internal(String),
    #[error("Agent Runtime error: {0}")]
    Agent(String),
}

impl serde::Serialize for DesktopError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
