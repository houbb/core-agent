use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `metric.query` — Query metrics from Prometheus.
/// Stub — requires external system configuration.
pub struct MetricQueryTool;

#[async_trait]
impl Tool for MetricQueryTool {
    fn key(&self) -> &str { "builtin/metric.query@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"].as_str().unwrap_or("up");
        Ok(RawToolOutput::text(format!(
            "[METRIC_QUERY] Query: {query}\n\nStatus: Metric query tool requires Prometheus configuration.\n\nTo configure:\n  1. Set PROMETHEUS_URL environment variable\n  2. Set PROMETHEUS_API_TOKEN if required"
        )))
    }
}

pub fn metric_query_tool() -> Arc<dyn Tool> { Arc::new(MetricQueryTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = MetricQueryTool.execute(&ToolRequest::new("builtin/metric.query@1.0.0", serde_json::json!({"query": "up"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("METRIC_QUERY")), _ => panic!("expected text") };
    }
}