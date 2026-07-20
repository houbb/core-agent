use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `cron.create` — Create a scheduled task.
pub struct CronCreateTool;

#[async_trait]
impl Tool for CronCreateTool {
    fn key(&self) -> &str { "builtin/cron.create@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let expr = request.parameters["expression"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("expression is required".into()))?;
        let task = request.parameters["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("task is required".into()))?;
        Ok(RawToolOutput::text(format!("[CRON_CREATE] Scheduled: {expr} → {task}")))
    }
}

pub fn cron_create_tool() -> Arc<dyn Tool> { Arc::new(CronCreateTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn creates_cron() {
        let result = CronCreateTool.execute(&ToolRequest::new(
            "builtin/cron.create@1.0.0",
            serde_json::json!({"expression": "0 9 * * *", "task": "daily report"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("daily report"));
    }
}