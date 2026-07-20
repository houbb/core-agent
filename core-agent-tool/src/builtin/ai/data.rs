use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `data.analyze` — Analyze data from SQL/CSV/Excel sources.
/// Stub — requires data source configuration.
pub struct DataAnalyzeTool;

#[async_trait]
impl Tool for DataAnalyzeTool {
    fn key(&self) -> &str { "builtin/data.analyze@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let source = request.parameters["source"].as_str().unwrap_or("");
        let query = request.parameters["query"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[DATA_ANALYSIS] Source: {source}, Query: {query}\n\nStatus: Data analysis requires data source configuration.\n\nTo configure:\n  1. Set DATA_SOURCE_TYPE (sql | csv | excel | dataframe)\n  2. Provide connection string or file path"
        )))
    }
}

pub fn data_analyze_tool() -> Arc<dyn Tool> { Arc::new(DataAnalyzeTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = DataAnalyzeTool.execute(&ToolRequest::new("builtin/data.analyze@1.0.0", serde_json::json!({"source": "database"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("DATA_ANALYSIS")), _ => panic!("expected text") };
    }
}