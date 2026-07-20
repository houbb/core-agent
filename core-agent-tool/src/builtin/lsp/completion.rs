use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.completion` — Get code completion suggestions.
pub struct LspCompletionTool;

#[async_trait]
impl Tool for LspCompletionTool {
    fn key(&self) -> &str { "builtin/lsp.completion@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let prefix = request.parameters["prefix"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("prefix is required".into()))?;
        Ok(RawToolOutput::text(format!("[LSP_COMPLETION] {prefix}\n\n(LSP client not configured)")))
    }
}

pub fn lsp_completion_tool() -> Arc<dyn Tool> { Arc::new(LspCompletionTool) }