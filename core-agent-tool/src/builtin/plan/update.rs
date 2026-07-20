use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.update` — Update plan status.
pub struct PlanUpdateTool;

#[async_trait]
impl Tool for PlanUpdateTool {
    fn key(&self) -> &str { "builtin/plan.update@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let plan_id = request.parameters["plan_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("plan_id is required".into()))?;
        let status = request.parameters["status"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("status is required".into()))?;
        Ok(RawToolOutput::text(format!("[PLAN_UPDATE] Plan {plan_id} → {status}")))
    }
}

pub fn plan_update_tool() -> Arc<dyn Tool> { Arc::new(PlanUpdateTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn updates_plan() {
        let result = PlanUpdateTool.execute(&ToolRequest::new(
            "builtin/plan.update@1.0.0",
            serde_json::json!({"plan_id": "plan-1", "status": "approved"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("plan-1") && text.contains("approved"));
    }
}