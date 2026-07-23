use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    AgentChatApiRequest, AgentChatApiResponse, ApiKey, ApiKeyScope, KnowledgeSearchApiRequest,
    KnowledgeSearchApiResponse, RateLimitStatus, TaskApiRequest, TaskApiResponse,
    WorkflowRunApiRequest, WorkflowRunApiResponse,
};
use crate::error::OpenApiResult;

/// API Key store — manages key creation, lookup, and revocation.
#[async_trait]
pub trait ApiKeyStore: Send + Sync {
    /// Store a new API key.
    async fn store(&self, key: &ApiKey) -> OpenApiResult<()>;

    /// Find an API key by its hash.
    async fn find_by_hash(&self, key_hash: &str) -> OpenApiResult<Option<ApiKey>>;

    /// List all API keys for a tenant.
    async fn list_by_tenant(&self, tenant_id: Uuid) -> OpenApiResult<Vec<ApiKey>>;

    /// Revoke an API key.
    async fn revoke(&self, key_id: Uuid, actor: &str) -> OpenApiResult<()>;
}

/// Rate limiter — controls request frequency per API key.
#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Check if a request is allowed under the rate limit.
    async fn check(&self, api_key_id: Uuid, scope: ApiKeyScope) -> OpenApiResult<RateLimitStatus>;
}

/// Gateway — handles authentication, rate limiting, and routing to agent runtime.
///
/// This is the core abstraction for the API Gateway that sits between
/// external clients and the agent runtime.
#[async_trait]
pub trait Gateway: Send + Sync {
    /// Authenticate a request using an API key.
    async fn authenticate(&self, api_key: &str) -> OpenApiResult<ApiKey>;

    /// Authorize that the API key has the required scope.
    async fn authorize(&self, key: &ApiKey, scope: ApiKeyScope) -> OpenApiResult<()>;

    /// Route a chat request to the agent runtime.
    async fn chat(&self, request: AgentChatApiRequest, key: &ApiKey) -> OpenApiResult<AgentChatApiResponse>;

    /// Route a task execution request.
    async fn execute_task(&self, request: TaskApiRequest, key: &ApiKey) -> OpenApiResult<TaskApiResponse>;

    /// Route a workflow run request.
    async fn run_workflow(&self, request: WorkflowRunApiRequest, key: &ApiKey) -> OpenApiResult<WorkflowRunApiResponse>;

    /// Route a knowledge search request.
    async fn search_knowledge(&self, request: KnowledgeSearchApiRequest, key: &ApiKey) -> OpenApiResult<KnowledgeSearchApiResponse>;
}