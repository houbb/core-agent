use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.references` — Find references to a symbol.
pub struct LspReferencesTool;

#[async_trait]
impl Tool for LspReferencesTool {
    fn key(&self) -> &str { "builtin/lsp.references@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let symbol = request.parameters["symbol"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("symbol is required".into()))?;
        Ok(RawToolOutput::text(format!("[LSP_REFERENCES] {symbol}\n\n(LSP client not configured)")))
    }
}

pub fn lsp_references_tool() -> Arc<dyn Tool> { Arc::new(LspReferencesTool) }