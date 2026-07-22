use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("document validation failed: {0}")]
    Validation(String),
    #[error("document not found: {0}")]
    NotFound(String),
    #[error("document conflict: {0}")]
    Conflict(String),
    #[error("document parse error: {0}")]
    ParseError(String),
    #[error("unsupported document format: {0}")]
    UnsupportedFormat(String),
    #[error("document database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("document serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("document internal failure: {0}")]
    Internal(String),
}

impl DocumentError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::ParseError(_) => "PARSE_ERROR",
            Self::UnsupportedFormat(_) => "UNSUPPORTED_FORMAT",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type DocumentResult<T> = Result<T, DocumentError>;