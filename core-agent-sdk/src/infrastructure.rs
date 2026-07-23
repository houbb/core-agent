use async_trait::async_trait;

use crate::domain::{
    ChatRequest, ChatResponse, ExecuteRequest, ExecuteResponse,
};
use crate::error::SdkResult;

/// AgentClient — the primary interface for interacting with an agent.
///
/// Implementations can connect to a local runtime, a remote API, or
/// a mock for testing.
#[async_trait]
pub trait AgentClient: Send + Sync {
    /// Send a chat message and receive a response.
    async fn chat(&self, request: ChatRequest) -> SdkResult<ChatResponse>;

    /// Execute a task and return the result.
    async fn execute(&self, request: ExecuteRequest) -> SdkResult<ExecuteResponse>;
}

/// AgentClientProvider — factory for creating AgentClient instances.
#[async_trait]
pub trait AgentClientProvider: Send + Sync {
    /// Create a new AgentClient with the given configuration.
    async fn create_client(&self, endpoint: &str, api_key: &str) -> SdkResult<Box<dyn AgentClient>>;
}

/// PublishClient — the interface for publishing agents to the marketplace.
#[async_trait]
pub trait PublishClient: Send + Sync {
    /// Publish an agent package.
    async fn publish(&self, request: crate::domain::PublishRequest) -> SdkResult<crate::domain::AgentIdentity>;

    /// Unpublish (deprecate) a previously published agent.
    async fn unpublish(&self, key: &str, version: &str) -> SdkResult<()>;
}