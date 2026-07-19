//! Provider-neutral domain contracts.

mod capability;
mod metadata;
mod profile;
mod request;
mod request_metric;
mod response;
mod usage;

pub use capability::ModelCapability;
pub use profile::{
    ModelLimits, ModelPerformance, ModelPolicy, ModelPricing, ModelProfile, ModelRoute,
    ProviderDefinition, RoutingRequest, RoutingStrategy,
};
pub use request::{
    ContentPart, EmbeddingRequest, ImageDetail, ModelConfig, ModelMessage, ModelRequest, ModelRole,
    ModelToolCall, ModelToolDefinition,
};
pub use request_metric::{AgentRequestMetric, RequestStatus, UsageBucket};
pub use response::{
    EmbeddingResponse, FinishReason, ModelResponse, ModelStreamEvent, ToolCallDelta,
    ToolCallRequest,
};
pub use usage::{ModelOperation, ModelUsage, UsageRecord};

pub(crate) use metadata::{audit_metadata, validate_audit_metadata, validate_metadata};
