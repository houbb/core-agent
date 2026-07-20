use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `cron.list` — List scheduled tasks.
pub struct CronListTool;

#[async_trait]
impl Tool for CronListTool {
    fn key(&self) -> &str { "builtin/cron.list@1.0.0" }

    async fn execute(&self, _request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        Ok(RawToolOutput::text("No scheduled tasks."))
    }
}

pub fn cron_list_tool() -> Arc<dyn Tool> { Arc::new(CronListTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn lists_empty() {
        let result = CronListTool.execute(&ToolRequest::new(
            "builtin/cron.list@1.0.0", serde_json::json!({}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("No scheduled tasks"));
    }
}