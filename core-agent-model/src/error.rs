//! Model Runtime error contract.

use crate::domain::ModelCapability;

/// All failures exposed by Model Runtime.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// A caller supplied an invalid or contradictory request.
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// No enabled profile satisfied the routing request.
    #[error("No model route found: {0}")]
    RouteNotFound(String),

    /// A profile points at a Provider that was not registered at runtime.
    #[error("Provider not registered: {0}")]
    ProviderNotFound(String),

    /// The selected model lacks a required capability.
    #[error("Model profile {profile} does not support {capability:?}")]
    UnsupportedCapability {
        profile: String,
        capability: ModelCapability,
    },

    /// The central Engine timeout elapsed.
    #[error("Provider {provider} timed out after {timeout_ms} ms")]
    Timeout { provider: String, timeout_ms: u64 },

    /// A rate limiter rejected the request.
    #[error("Provider {provider} was rate limited: {message}")]
    RateLimited { provider: String, message: String },

    /// A Provider returned an error. The retry flag is explicit so the Engine,
    /// rather than the Provider, owns retry decisions.
    #[error("Provider {provider} failed: {message}")]
    Provider {
        provider: String,
        message: String,
        status: Option<u16>,
        retryable: bool,
    },

    /// Catalog or Usage persistence failed.
    #[error("Persistence error: {0}")]
    Persistence(String),

    /// Serialization or protocol decoding failed.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// An interceptor rejected the request or response.
    #[error("Interceptor error: {0}")]
    Interceptor(String),

    /// A usage audit failure occurred, possibly after inference succeeded.
    #[error("Usage collection error: {message}")]
    Usage {
        message: String,
        #[source]
        source: Option<Box<ModelError>>,
    },

    /// Internal task or synchronization failure.
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ModelError {
    /// Whether the same Provider operation may be retried by RetryPolicy.
    pub fn is_retryable(&self) -> bool {
        if let Self::Usage {
            source: Some(source),
            ..
        } = self
        {
            return source.is_retryable();
        }
        matches!(
            self,
            Self::Timeout { .. }
                | Self::RateLimited { .. }
                | Self::Provider {
                    retryable: true,
                    ..
                }
        )
    }

    /// Whether routing may continue to a different profile before output starts.
    pub fn is_fallback_eligible(&self) -> bool {
        if let Self::Usage {
            source: Some(source),
            ..
        } = self
        {
            return source.is_fallback_eligible();
        }
        self.is_retryable() || matches!(self, Self::ProviderNotFound(_))
    }

    /// Stable category suitable for audit metadata without sensitive details.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "INVALID_ARGUMENT",
            Self::RouteNotFound(_) => "ROUTE_NOT_FOUND",
            Self::ProviderNotFound(_) => "PROVIDER_NOT_FOUND",
            Self::UnsupportedCapability { .. } => "UNSUPPORTED_CAPABILITY",
            Self::Timeout { .. } => "TIMEOUT",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Provider { .. } => "PROVIDER",
            Self::Persistence(_) => "PERSISTENCE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Interceptor(_) => "INTERCEPTOR",
            Self::Usage {
                source: Some(source),
                ..
            } => source.kind(),
            Self::Usage { source: None, .. } => "USAGE",
            Self::Internal(_) => "INTERNAL",
        }
    }

    /// Provider attribution used by durable failure records.
    pub fn provider_key(&self) -> Option<&str> {
        match self {
            Self::Provider { provider, .. }
            | Self::Timeout { provider, .. }
            | Self::RateLimited { provider, .. } => Some(provider),
            Self::Usage {
                source: Some(source),
                ..
            } => source.provider_key(),
            _ => None,
        }
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage {
            message: message.into(),
            source: None,
        }
    }

    pub fn usage_with_source(message: impl Into<String>, source: ModelError) -> Self {
        Self::Usage {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

/// Result alias used throughout Model Runtime.
pub type ModelResult<T> = Result<T, ModelError>;
