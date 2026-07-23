use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenApiError {
    #[error("openapi validation failed: {0}")]
    Validation(String),
    #[error("openapi not found: {0}")]
    NotFound(String),
    #[error("openapi authentication failed: {0}")]
    Authentication(String),
    #[error("openapi authorization failed: {0}")]
    Authorization(String),
    #[error("openapi rate limit exceeded: {0}")]
    RateLimit(String),
    #[error("openapi platform governance failed: {0}")]
    Platform(String),
    #[error("openapi serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("openapi internal failure: {0}")]
    Internal(String),
}

impl OpenApiError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Authentication(_) => "AUTHENTICATION",
            Self::Authorization(_) => "AUTHORIZATION",
            Self::RateLimit(_) => "RATE_LIMIT",
            Self::Platform(_) => "PLATFORM",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type OpenApiResult<T> = Result<T, OpenApiError>;