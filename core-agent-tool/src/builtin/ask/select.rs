use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `ask.select` — Ask the user to choose from a list of options.
pub struct AskSelectTool;

#[async_trait]
impl Tool for AskSelectTool {
    fn key(&self) -> &str {
        "builtin/ask.select@1.0.0"
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

        let options = request.parameters["options"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidArgument("options must be an array".into()))?;
        if options.is_empty() {
            return Err(ToolError::InvalidArgument("options must not be empty".into()));
        }
        if options.len() > 10 {
            return Err(ToolError::InvalidArgument("options exceeds max of 10".into()));
        }

        let options_str: Vec<String> = options
            .iter()
            .enumerate()
            .map(|(i, opt)| {
                let label = opt.as_str().unwrap_or("option");
                format!("  {}) {label}", i + 1)
            })
            .collect();

        let mut output = RawToolOutput::text(format!(
            "[ASK_SELECT] {question}\n\nOptions:\n{}\n\n(Waiting for selection...)",
            options_str.join("\n")
        ));
        output.metadata.insert("user_input_required".to_string(), "true".to_string());
        output.metadata.insert("question".to_string(), question.to_string());
        Ok(output)
    }
}

pub fn ask_select_tool() -> Arc<dyn Tool> {
    Arc::new(AskSelectTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn shows_options() {
        let tool = AskSelectTool;
        let request = ToolRequest::new(
            "builtin/ask.select@1.0.0",
            serde_json::json!({
                "question": "Choose a color",
                "options": ["Red", "Blue", "Green"]
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Red"));
        assert!(text.contains("Blue"));
        assert!(text.contains("Green"));
    }

    #[tokio::test]
    async fn rejects_empty_options() {
        let tool = AskSelectTool;
        let request = ToolRequest::new(
            "builtin/ask.select@1.0.0",
            serde_json::json!({"question": "Pick one", "options": []}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}