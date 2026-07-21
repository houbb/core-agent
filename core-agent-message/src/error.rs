use uuid::Uuid;

pub type MessageResult<T> = Result<T, MessageError>;

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("message validation failed: {0}")]
    Validation(String),

    #[error("message not found: {0}")]
    NotFound(String),

    #[error("message version conflict: {0}")]
    Conflict(String),

    #[error("message bus error: {0}")]
    BusError(String),

    #[error("message database failed: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("message serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("message internal failure: {0}")]
    Internal(String),
}

impl MessageError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}