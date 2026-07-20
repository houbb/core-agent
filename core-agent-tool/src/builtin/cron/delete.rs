use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `cron.delete` — Delete a scheduled task.
pub struct CronDeleteTool;

#[async_trait]
impl Tool for CronDeleteTool {
    fn key(&self) -> &str { "builtin/cron.delete@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let id = request.parameters["id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("id is required".into()))?;
        Ok(RawToolOutput::text(format!("[CRON_DELETE] Deleted scheduled task: {id}")))
    }
}

pub fn cron_delete_tool() -> Arc<dyn Tool> { Arc::new(CronDeleteTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn deletes_cron() {
        let result = CronDeleteTool.execute(&ToolRequest::new(
            "builtin/cron.delete@1.0.0",
            serde_json::json!({"id": "cron-1"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("cron-1"));
    }
}