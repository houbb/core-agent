use thiserror::Error;

pub type ScannerResult<T> = Result<T, ScannerError>;

#[derive(Debug, Error)]
pub enum ScannerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid extension root '{path}': {reason}")]
    InvalidRoot { path: String, reason: String },

    #[error("Duplicate extension '{name}' at {first} and {second}")]
    DuplicateEntry { name: String, first: String, second: String },

    #[error("Limit exceeded: {kind} max {limit}")]
    LimitExceeded { kind: String, limit: usize },

    #[error("Invalid UTF-8 in file: {0}")]
    InvalidUtf8(String),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}