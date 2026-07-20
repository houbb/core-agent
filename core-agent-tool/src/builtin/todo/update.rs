use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `todo.update` — Update a todo item status.
pub struct TodoUpdateTool;

#[async_trait]
impl Tool for TodoUpdateTool {
    fn key(&self) -> &str {
        "builtin/todo.update@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let id = request.parameters["id"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArgument("id is required".into()))?;
        let status = request.parameters["status"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("status is required".into()))?;

        let valid = ["pending", "in_progress", "completed", "cancelled"];
        if !valid.contains(&status) {
            return Err(ToolError::InvalidArgument(format!(
                "invalid status: {status}, expected one of: {}", valid.join(", ")
            )));
        }

        Ok(RawToolOutput::text(format!("[TODO] Updated #{id} → {status}")))
    }
}

pub fn todo_update_tool() -> Arc<dyn Tool> {
    Arc::new(TodoUpdateTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn updates_todo() {
        let tool = TodoUpdateTool;
        let request = ToolRequest::new(
            "builtin/todo.update@1.0.0",
            serde_json::json!({"id": 1, "status": "completed"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("completed"));
    }

    #[tokio::test]
    async fn rejects_invalid_status() {
        let tool = TodoUpdateTool;
        let request = ToolRequest::new(
            "builtin/todo.update@1.0.0",
            serde_json::json!({"id": 1, "status": "invalid_status"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}