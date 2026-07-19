use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::domain::{
    PermissionDecision, RawToolOutput, ToolCapability, ToolDefinition, ToolExecutionRecord,
    ToolPermissionRule, ToolProviderDefinition, ToolRequest, ToolResult,
};
use crate::error::ToolRuntimeResult;

#[derive(Clone)]
pub struct ToolRegistration {
    pub definition: ToolDefinition,
    pub tool: Arc<dyn Tool>,
}

impl ToolRegistration {
    pub fn new(definition: ToolDefinition, tool: Arc<dyn Tool>) -> Self {
        Self { definition, tool }
    }
}

#[derive(Clone)]
pub struct ToolContext {
    pub request_id: uuid::Uuid,
    pub cancellation: CancellationToken,
}

impl ToolContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn key(&self) -> &str;

    async fn execute(
        &self,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput>;
}

#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn definition(&self) -> ToolProviderDefinition;
    async fn discover(&self) -> ToolRuntimeResult<Vec<ToolRegistration>>;
}

pub trait ToolRegistry: Send + Sync {
    fn register(&self, registration: ToolRegistration) -> ToolRuntimeResult<()>;
    fn remove(&self, key: &str) -> ToolRuntimeResult<Option<Arc<dyn Tool>>>;
    fn find(&self, key: &str) -> ToolRuntimeResult<Option<Arc<dyn Tool>>>;
    fn list(&self) -> ToolRuntimeResult<Vec<String>>;
}

#[async_trait]
pub trait ToolCatalog: Send + Sync {
    async fn upsert_provider(&self, provider: &ToolProviderDefinition) -> ToolRuntimeResult<()>;
    async fn find_provider(&self, key: &str) -> ToolRuntimeResult<Option<ToolProviderDefinition>>;
    async fn list_providers(&self) -> ToolRuntimeResult<Vec<ToolProviderDefinition>>;
    async fn remove_provider(&self, key: &str) -> ToolRuntimeResult<bool>;

    async fn upsert_tool(&self, tool: &ToolDefinition) -> ToolRuntimeResult<()>;
    async fn find_tool(&self, key: &str) -> ToolRuntimeResult<Option<ToolDefinition>>;
    async fn list_tools(&self) -> ToolRuntimeResult<Vec<ToolDefinition>>;
    async fn remove_tool(&self, key: &str) -> ToolRuntimeResult<bool>;
    async fn find_by_capability(
        &self,
        capability: &ToolCapability,
        include_descendants: bool,
    ) -> ToolRuntimeResult<Vec<ToolDefinition>>;
    async fn categories(&self) -> ToolRuntimeResult<Vec<String>>;
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn invoke(
        &self,
        tool: Arc<dyn Tool>,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput>;
}

pub trait ToolValidator: Send + Sync {
    fn validate_schema(&self, schema: &serde_json::Value) -> ToolRuntimeResult<()>;
    fn validate(
        &self,
        schema: &serde_json::Value,
        parameters: &serde_json::Value,
    ) -> ToolRuntimeResult<()>;
}

#[async_trait]
pub trait ToolPermission: Send + Sync {
    async fn check(
        &self,
        request: &ToolRequest,
        tool: &ToolDefinition,
    ) -> ToolRuntimeResult<PermissionDecision>;
}

pub trait ToolResultMapper: Send + Sync {
    fn map(
        &self,
        request: &ToolRequest,
        definition: &ToolDefinition,
        started_at: chrono::DateTime<chrono::Utc>,
        completed_at: chrono::DateTime<chrono::Utc>,
        output: ToolRuntimeResult<RawToolOutput>,
    ) -> ToolRuntimeResult<ToolResult>;
}

#[async_trait]
pub trait ToolLifecycle: Send + Sync {
    async fn transition(&self, record: &ToolExecutionRecord) -> ToolRuntimeResult<()>;
}

#[async_trait]
pub trait ToolInterceptor: Send + Sync {
    async fn intercept_request(&self, _request: &mut ToolRequest) -> ToolRuntimeResult<()> {
        Ok(())
    }

    async fn intercept_result(&self, _result: &mut ToolResult) -> ToolRuntimeResult<()> {
        Ok(())
    }
}

#[async_trait]
pub trait ToolPolicy: Send + Sync {
    async fn evaluate(&self, request: &ToolRequest, tool: &ToolDefinition)
        -> ToolRuntimeResult<()>;
}

#[async_trait]
pub trait ToolPermissionStore: Send + Sync {
    async fn upsert_permission(&self, rule: &ToolPermissionRule) -> ToolRuntimeResult<()>;
    async fn list_permissions(&self) -> ToolRuntimeResult<Vec<ToolPermissionRule>>;
    async fn remove_permission(&self, id: uuid::Uuid) -> ToolRuntimeResult<bool>;
}
