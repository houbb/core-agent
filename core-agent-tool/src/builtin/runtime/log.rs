use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `log.query` — Query logs from ELK/Loki/ClickHouse.
/// Stub — requires external system configuration.
pub struct LogQueryTool;

#[async_trait]
impl Tool for LogQueryTool {
    fn key(&self) -> &str { "builtin/log.query@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"].as_str().unwrap_or("*");
        Ok(RawToolOutput::text(format!(
            "[LOG_QUERY] Query: {query}\n\nStatus: Log query tool requires ELK/Loki/ClickHouse configuration.\n\nTo configure:\n  1. Set LOG_QUERY_ENDPOINT environment variable\n  2. Set LOG_QUERY_TYPE (elasticsearch | loki | clickhouse)\n  3. Provide authentication credentials"
        )))
    }
}

pub fn log_query_tool() -> Arc<dyn Tool> { Arc::new(LogQueryTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = LogQueryTool.execute(&ToolRequest::new("builtin/log.query@1.0.0", serde_json::json!({"query": "error"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("LOG_QUERY")), _ => panic!("expected text") };
    }
}