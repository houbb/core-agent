use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `knowledge.search` — Search knowledge base / Vector DB / Wiki.
/// Stub — requires external system configuration.
pub struct KnowledgeSearchTool;

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn key(&self) -> &str { "builtin/knowledge.search@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[KNOWLEDGE_SEARCH] Query: {query}\n\nStatus: Knowledge search requires Vector DB / Wiki configuration.\n\nTo configure:\n  1. Set KNOWLEDGE_SEARCH_ENDPOINT\n  2. Set KNOWLEDGE_SEARCH_TYPE (vector | wiki | confluence | markdown)"
        )))
    }
}

pub fn knowledge_search_tool() -> Arc<dyn Tool> { Arc::new(KnowledgeSearchTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = KnowledgeSearchTool.execute(&ToolRequest::new("builtin/knowledge.search@1.0.0", serde_json::json!({"query": "auth"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("KNOWLEDGE_SEARCH")), _ => panic!("expected text") };
    }
}