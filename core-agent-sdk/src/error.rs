use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("sdk validation failed: {0}")]
    Validation(String),
    #[error("sdk not found: {0}")]
    NotFound(String),
    #[error("sdk execution failed: {0}")]
    Execution(String),
    #[error("sdk serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("sdk yaml parse failed: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("sdk internal failure: {0}")]
    Internal(String),
}

impl SdkError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Execution(_) => "EXECUTION",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Yaml(_) => "YAML",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type SdkResult<T> = Result<T, SdkError>;