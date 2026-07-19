//! Tool Runtime error contract.

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool is disabled: {0}")]
    ToolDisabled(String),

    #[error("Tool provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Tool provider is disabled: {0}")]
    ProviderDisabled(String),

    #[error("Tool registry error: {0}")]
    Registry(String),

    #[error("Tool validation failed: {0}")]
    Validation(String),

    #[error("Tool approval is required: {0}")]
    ApprovalRequired(String),

    #[error("Tool permission denied: {0}")]
    PermissionDenied(String),

    #[error("Tool policy denied execution: {0}")]
    PolicyDenied(String),

    #[error("Tool {tool} timed out after {timeout_ms} ms")]
    Timeout { tool: String, timeout_ms: u64 },

    #[error("Tool execution was cancelled: {0}")]
    Cancelled(String),

    #[error("Tool {tool} failed: {message}")]
    Execution {
        tool: String,
        message: String,
        retryable: bool,
    },

    #[error("Tool result mapping failed: {0}")]
    Mapping(String),

    #[error("Tool interceptor failed: {0}")]
    Interceptor(String),

    #[error("Tool lifecycle failed: {0}")]
    Lifecycle(String),

    #[error("Tool persistence failed: {0}")]
    Persistence(String),

    #[error("Tool serialization failed: {0}")]
    Serialization(String),

    #[error("Tool Runtime internal error: {0}")]
    Internal(String),
}

impl ToolError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "INVALID_ARGUMENT",
            Self::ToolNotFound(_) => "TOOL_NOT_FOUND",
            Self::ToolDisabled(_) => "TOOL_DISABLED",
            Self::ProviderNotFound(_) => "PROVIDER_NOT_FOUND",
            Self::ProviderDisabled(_) => "PROVIDER_DISABLED",
            Self::Registry(_) => "REGISTRY",
            Self::Validation(_) => "VALIDATION",
            Self::ApprovalRequired(_) => "APPROVAL_REQUIRED",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::PolicyDenied(_) => "POLICY_DENIED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Cancelled(_) => "CANCELLED",
            Self::Execution { .. } => "EXECUTION",
            Self::Mapping(_) => "MAPPING",
            Self::Interceptor(_) => "INTERCEPTOR",
            Self::Lifecycle(_) => "LIFECYCLE",
            Self::Persistence(_) => "PERSISTENCE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Timeout { .. }
                | Self::Execution {
                    retryable: true,
                    ..
                }
        )
    }

    pub fn execution(tool: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self::Execution {
            tool: tool.into(),
            message: message.into(),
            retryable,
        }
    }
}

pub type ToolRuntimeResult<T> = Result<T, ToolError>;
