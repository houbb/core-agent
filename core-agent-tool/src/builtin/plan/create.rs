use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.create` — Create an execution plan.
pub struct PlanCreateTool;

#[async_trait]
impl Tool for PlanCreateTool {
    fn key(&self) -> &str { "builtin/plan.create@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let goal = request.parameters["goal"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("goal is required".into()))?;
        Ok(RawToolOutput::text(format!("[PLAN_CREATE] Plan created for: {goal}")))
    }
}

pub fn plan_create_tool() -> Arc<dyn Tool> { Arc::new(PlanCreateTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn creates_plan() {
        let result = PlanCreateTool.execute(&ToolRequest::new(
            "builtin/plan.create@1.0.0",
            serde_json::json!({"goal": "refactor module"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("refactor module"));
    }
}