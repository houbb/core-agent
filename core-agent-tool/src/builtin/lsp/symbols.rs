use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.symbols` — Search workspace symbols.
pub struct LspSymbolsTool;

#[async_trait]
impl Tool for LspSymbolsTool {
    fn key(&self) -> &str { "builtin/lsp.symbols@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("query is required".into()))?;
        Ok(RawToolOutput::text(format!("[LSP_SYMBOLS] {query}\n\n(LSP client not configured)")))
    }
}

pub fn lsp_symbols_tool() -> Arc<dyn Tool> { Arc::new(LspSymbolsTool) }