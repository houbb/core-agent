use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `todo.list` — List all todo items.
pub struct TodoListTool;

#[async_trait]
impl Tool for TodoListTool {
    fn key(&self) -> &str {
        "builtin/todo.list@1.0.0"
    }

    async fn execute(
        &self,
        _request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        Ok(RawToolOutput::text("No todo items yet."))
    }
}

pub fn todo_list_tool() -> Arc<dyn Tool> {
    Arc::new(TodoListTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn lists_empty() {
        let tool = TodoListTool;
        let request = ToolRequest::new(
            "builtin/todo.list@1.0.0",
            serde_json::json!({}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("No todo items"));
    }
}