use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `ask.user` — Ask the user a question and get their answer.
/// This tool produces a structured output that the Agent framework
/// translates into a user prompt.
pub struct AskUserTool;

#[async_trait]
impl Tool for AskUserTool {
    fn key(&self) -> &str {
        "builtin/ask.user@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let question = request.parameters["question"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("question is required".into()))?;
        if question.is_empty() {
            return Err(ToolError::InvalidArgument("question must not be empty".into()));
        }

        // This tool is handled by the Agent framework at a higher level.
        // The actual user interaction happens outside the tool runtime.
        // The tool returns a structured result that signals "ask the user".
        Ok(RawToolOutput::text(format!(
            "[ASK_USER] {question}\n\n(Waiting for user response...)"
        )))
    }
}

pub fn ask_user_tool() -> Arc<dyn Tool> {
    Arc::new(AskUserTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn returns_question() {
        let tool = AskUserTool;
        let request = ToolRequest::new(
            "builtin/ask.user@1.0.0",
            serde_json::json!({"question": "What is your favorite color?"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("favorite color"));
    }
}