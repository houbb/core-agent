pub type CliResult<T> = Result<T, CliError>;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("no resumable session is available")]
    NoSession,
    #[error("Agent API request failed: {0}")]
    Api(String),
    #[error("Agent event stream failed: {0}")]
    Stream(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
