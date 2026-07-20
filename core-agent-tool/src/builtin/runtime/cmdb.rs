use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `cmdb.query` — Query CMDB for service/instance/owner information.
/// Stub — requires external system configuration.
pub struct CmdbQueryTool;

#[async_trait]
impl Tool for CmdbQueryTool {
    fn key(&self) -> &str { "builtin/cmdb.query@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"].as_str().unwrap_or("*");
        let entity_type = request.parameters["type"].as_str().unwrap_or("service");
        Ok(RawToolOutput::text(format!(
            "[CMDB_QUERY] Type: {entity_type}, Query: {query}\n\nStatus: CMDB query tool requires configuration.\n\nTo configure:\n  1. Set CMDB_ENDPOINT environment variable\n  2. Set CMDB_API_TOKEN if required\n  3. Supported types: service, instance, cluster, owner, dependency"
        )))
    }
}

pub fn cmdb_query_tool() -> Arc<dyn Tool> { Arc::new(CmdbQueryTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = CmdbQueryTool.execute(&ToolRequest::new("builtin/cmdb.query@1.0.0", serde_json::json!({"query": "user-service"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("CMDB_QUERY")), _ => panic!("expected text") };
    }
}