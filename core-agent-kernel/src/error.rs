pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("runtime already registered: {0}")]
    Duplicate(String),
    #[error("runtime not found: {0}")]
    NotFound(String),
    #[error("dependency graph invalid: {0}")]
    Dependency(String),
    #[error("runtime version incompatible: {0}")]
    Version(String),
    #[error("invalid Kernel state: {0}")]
    InvalidState(String),
    #[error("runtime {runtime} failed during {operation}: {message}")]
    Lifecycle {
        runtime: String,
        operation: String,
        message: String,
    },
    #[error("Kernel hook failed: {0}")]
    Hook(String),
    #[error("service registry failed: {0}")]
    Service(String),
    #[error("Kernel internal error: {0}")]
    Internal(String),
}
