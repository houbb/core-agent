use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `ask.confirm` — Ask the user for a Yes/No confirmation.
pub struct AskConfirmTool;

#[async_trait]
impl Tool for AskConfirmTool {
    fn key(&self) -> &str {
        "builtin/ask.confirm@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let message = request.parameters["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("message is required".into()))?;
        if message.is_empty() {
            return Err(ToolError::InvalidArgument("message must not be empty".into()));
        }

        Ok(RawToolOutput::text(format!(
            "[ASK_CONFIRM] {message}\n\n(Waiting for Yes/No confirmation...)"
        )))
    }
}

pub fn ask_confirm_tool() -> Arc<dyn Tool> {
    Arc::new(AskConfirmTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn returns_confirmation_message() {
        let tool = AskConfirmTool;
        let request = ToolRequest::new(
            "builtin/ask.confirm@1.0.0",
            serde_json::json!({"message": "Delete the file?"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Delete the file"));
    }
}