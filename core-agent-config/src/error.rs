pub type ConfigResult<T> = Result<T, ConfigError>;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration validation failed: {0}")]
    Validation(String),
    #[error("configuration source failed: {0}")]
    Source(String),
    #[error("configuration source is ambiguous: {0}")]
    Ambiguous(String),
    #[error("configuration secret could not be resolved: {0}")]
    Secret(String),
    #[error("configuration I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration YAML failed: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
