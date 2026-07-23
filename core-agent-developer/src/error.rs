use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeveloperError {
    #[error("developer validation failed: {0}")]
    Validation(String),
    #[error("developer not found: {0}")]
    NotFound(String),
    #[error("developer conflict: {0}")]
    Conflict(String),
    #[error("developer authorization denied: {0}")]
    Denied(String),
    #[error("developer platform governance failed: {0}")]
    Platform(String),
    #[error("developer serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("developer yaml parse failed: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("developer internal failure: {0}")]
    Internal(String),
}

impl DeveloperError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Denied(_) => "DENIED",
            Self::Platform(_) => "PLATFORM",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Yaml(_) => "YAML",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type DeveloperResult<T> = Result<T, DeveloperError>;