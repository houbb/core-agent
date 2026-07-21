use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpRuntimeError {
    #[error("MCP configuration is invalid: {0}")]
    Invalid(String),
    #[error("MCP I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("MCP server {server} returned an error: {message}")]
    Remote { server: String, message: String },
    #[error("MCP server {0} disconnected")]
    Disconnected(String),
    #[error("MCP request to {0} timed out")]
    Timeout(String),
    #[error("MCP request to {0} was cancelled")]
    Cancelled(String),
}

pub type McpRuntimeResult<T> = Result<T, McpRuntimeError>;