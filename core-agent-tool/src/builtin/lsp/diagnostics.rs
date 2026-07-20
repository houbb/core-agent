use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.diagnostics` — Get diagnostics for a file.
pub struct LspDiagnosticsTool;

#[async_trait]
impl Tool for LspDiagnosticsTool {
    fn key(&self) -> &str { "builtin/lsp.diagnostics@1.0.0" }

    async fn execute(&self, _request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        Ok(RawToolOutput::text("No diagnostics (LSP client not configured)."))
    }
}

pub fn lsp_diagnostics_tool() -> Arc<dyn Tool> { Arc::new(LspDiagnosticsTool) }