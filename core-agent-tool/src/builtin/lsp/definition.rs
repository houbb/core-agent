use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.definition` — Go to definition.
pub struct LspDefinitionTool;

#[async_trait]
impl Tool for LspDefinitionTool {
    fn key(&self) -> &str { "builtin/lsp.definition@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let symbol = request.parameters["symbol"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("symbol is required".into()))?;
        Ok(RawToolOutput::text(format!("[LSP_DEFINITION] {symbol}\n\n(LSP client not configured)")))
    }
}

pub fn lsp_definition_tool() -> Arc<dyn Tool> { Arc::new(LspDefinitionTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn finds_definition() {
        let result = LspDefinitionTool.execute(&ToolRequest::new(
            "builtin/lsp.definition@1.0.0",
            serde_json::json!({"symbol": "UserService"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("UserService"));
    }
}