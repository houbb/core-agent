use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `todo.add` — Add a todo item.
pub struct TodoAddTool;

#[async_trait]
impl Tool for TodoAddTool {
    fn key(&self) -> &str {
        "builtin/todo.add@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let task = request.parameters["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("task is required".into()))?;
        if task.is_empty() {
            return Err(ToolError::InvalidArgument("task must not be empty".into()));
        }

        Ok(RawToolOutput::text(format!("[TODO] Added: {task}")))
    }
}

pub fn todo_add_tool() -> Arc<dyn Tool> {
    Arc::new(TodoAddTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn adds_todo() {
        let tool = TodoAddTool;
        let request = ToolRequest::new(
            "builtin/todo.add@1.0.0",
            serde_json::json!({"task": "Implement login"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Implement login"));
    }
}