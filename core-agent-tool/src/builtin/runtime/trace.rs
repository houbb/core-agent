use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `trace.query` — Query traces from Jaeger/SkyWalking.
/// Stub — requires external system configuration.
pub struct TraceQueryTool;

#[async_trait]
impl Tool for TraceQueryTool {
    fn key(&self) -> &str { "builtin/trace.query@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let trace_id = request.parameters["trace_id"].as_str().unwrap_or("*");
        Ok(RawToolOutput::text(format!(
            "[TRACE_QUERY] Trace ID: {trace_id}\n\nStatus: Trace query tool requires Jaeger/SkyWalking configuration.\n\nTo configure:\n  1. Set TRACE_QUERY_ENDPOINT environment variable\n  2. Set TRACE_QUERY_TYPE (jaeger | skywalking | opentelemetry)"
        )))
    }
}

pub fn trace_query_tool() -> Arc<dyn Tool> { Arc::new(TraceQueryTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = TraceQueryTool.execute(&ToolRequest::new("builtin/trace.query@1.0.0", serde_json::json!({"trace_id": "abc123"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("TRACE_QUERY")), _ => panic!("expected text") };
    }
}