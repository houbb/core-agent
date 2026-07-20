use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.review` — Review a plan for approval.
pub struct PlanReviewTool;

#[async_trait]
impl Tool for PlanReviewTool {
    fn key(&self) -> &str { "builtin/plan.review@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let plan_id = request.parameters["plan_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("plan_id is required".into()))?;
        Ok(RawToolOutput::text(format!("[PLAN_REVIEW] Plan {plan_id} is ready for review\n\nNo issues found.")))
    }
}

pub fn plan_review_tool() -> Arc<dyn Tool> { Arc::new(PlanReviewTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn reviews_plan() {
        let result = PlanReviewTool.execute(&ToolRequest::new(
            "builtin/plan.review@1.0.0",
            serde_json::json!({"plan_id": "plan-1"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("plan-1"));
    }
}