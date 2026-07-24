use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("plugin validation failed: {0}")]
    Validation(String),
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("plugin conflict: {0}")]
    Conflict(String),
    #[error("plugin state error: {0}")]
    InvalidState(String),
    #[error("plugin I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("plugin serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("plugin YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("extension runtime error: {0}")]
    Extension(String),
    #[error("plugin package error: {0}")]
    Package(String),
    #[error("plugin exceeds limit {kind}: {limit}")]
    LimitExceeded { kind: String, limit: usize },
}

pub type PluginResult<T> = Result<T, PluginError>;