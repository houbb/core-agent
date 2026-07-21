use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlashError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("command execution failed: {0}")]
    Execution(String),
    #[error("command not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type SlashResult<T> = Result<T, SlashError>;