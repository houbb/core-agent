use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `knowledge.search` — Search knowledge base / Vector DB / Wiki.
/// Uses core-agent-rag for real retrieval when configured.
pub struct KnowledgeSearchTool {
    rag_manager: Option<Arc<core_agent_rag::RagManager>>,
}

impl KnowledgeSearchTool {
    pub fn new(rag_manager: Option<Arc<core_agent_rag::RagManager>>) -> Self {
        Self { rag_manager }
    }
}

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn key(&self) -> &str { "builtin/knowledge.search@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"].as_str().unwrap_or("");
        let namespace = request.parameters["namespace"].as_str().unwrap_or("default");
        let top_k = request.parameters["top_k"].as_u64().unwrap_or(5) as usize;

        if query.is_empty() {
            return Ok(RawToolOutput::text("Please provide a query to search the knowledge base."));
        }

        let Some(rag) = &self.rag_manager else {
            return Ok(RawToolOutput::text(
                "[KNOWLEDGE_SEARCH] Knowledge search is not configured.\n\n\
                 To configure:\n  1. Set up a knowledge base with documents\n  \
                 2. Configure the RAG pipeline with a VectorManager"
            ));
        };

        let result = rag.ask(query, namespace, "knowledge-tool").await
            .map_err(|e| ToolError::execution("knowledge.search", e.to_string(), false))?;

        let sources = result.sources.iter()
            .map(|s| format!("  - [{}] (score: {:.2})", s.source, s.score))
            .collect::<Vec<_>>()
            .join("\n");

        let output = if sources.is_empty() {
            format!("No relevant knowledge found for query: {query}")
        } else {
            format!(
                "Knowledge search results for: {query}\n\nRelevant sources:\n{sources}\n\nAnswer:\n{}",
                result.answer
            )
        };

        Ok(RawToolOutput::text(output))
    }
}

pub fn knowledge_search_tool() -> Arc<dyn Tool> {
    Arc::new(KnowledgeSearchTool::new(None))
}

pub fn knowledge_search_tool_with_rag(rag: Arc<core_agent_rag::RagManager>) -> Arc<dyn Tool> {
    Arc::new(KnowledgeSearchTool::new(Some(rag)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn stub_returns_config_message() {
        let tool = KnowledgeSearchTool::new(None);
        let result = tool.execute(&ToolRequest::new("builtin/knowledge.search@1.0.0", serde_json::json!({"query": "auth"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("KNOWLEDGE_SEARCH")), _ => panic!("expected text") };
    }

    #[tokio::test]
    async fn empty_query_returns_prompt() {
        let tool = KnowledgeSearchTool::new(None);
        let result = tool.execute(&ToolRequest::new("builtin/knowledge.search@1.0.0", serde_json::json!({"query": ""})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("Please provide")), _ => panic!("expected text") };
    }
}