use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `web.search` — Search the web using a search engine query.
/// NOTE: This is a stub that requires a search API key to be configured.
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn key(&self) -> &str {
        "builtin/web.search@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("query is required".into()))?;
        if query.is_empty() {
            return Err(ToolError::InvalidArgument("query must not be empty".into()));
        }

        // Basic implementation: use a search API if configured
        // For now, return a message indicating search is not fully configured
        Ok(RawToolOutput::text(format!(
            "Search query: {query}\n\nNote: Web search requires a search API key to be configured in the environment."
        )))
    }
}

pub fn web_search_tool() -> Arc<dyn Tool> {
    Arc::new(WebSearchTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn rejects_empty_query() {
        let tool = WebSearchTool;
        let request = ToolRequest::new(
            "builtin/web.search@1.0.0",
            serde_json::json!({"query": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn accepts_valid_query() {
        let tool = WebSearchTool;
        let request = ToolRequest::new(
            "builtin/web.search@1.0.0",
            serde_json::json!({"query": "rust programming"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("rust programming"));
    }
}