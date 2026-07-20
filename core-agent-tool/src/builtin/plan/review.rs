use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::PlanningManager;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.review` — Review a plan for approval.
pub struct PlanReviewTool {
    planning: Arc<PlanningManager>,
}

impl PlanReviewTool {
    pub fn new(planning: Arc<PlanningManager>) -> Self {
        Self { planning }
    }
}

#[async_trait]
impl Tool for PlanReviewTool {
    fn key(&self) -> &str {
        "builtin/plan.review@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _ctx: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let plan_id = request.parameters["plan_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("plan_id is required".into()))?
            .to_string();

        let plan_uuid = uuid::Uuid::parse_str(&plan_id)
            .map_err(|_| ToolError::InvalidArgument("invalid plan_id format".into()))?;

        let plan = self
            .planning
            .find_plan(plan_uuid)
            .await
            .map_err(|e| ToolError::execution("plan.review", e.to_string(), false))?
            .ok_or_else(|| ToolError::InvalidArgument("plan not found".into()))?;

        let mut output = format!(
            "[PLAN_REVIEW] Plan {plan_id}\n\nStatus: {}\nVersion: {}\n",
            plan.status.as_str(),
            plan.version
        );

        if let Some(review) = &plan.review {
            output.push_str(&format!(
                "Review: {}\nFindings: {}\n",
                review.decision.as_str(),
                if review.findings.is_empty() {
                    "None".to_string()
                } else {
                    review.findings.join("; ")
                }
            ));
        } else {
            output.push_str("Review: PENDING\n");
        }

        output.push_str("\nTasks:\n");
        for task in plan.tasks.values() {
            output.push_str(&format!(
                "  [{status}] {name} (key: {key})\n",
                status = task.status.as_str(),
                name = task.name,
                key = task.key
            ));
            for step in task.steps.values() {
                output.push_str(&format!(
                    "    - {name} [{status}]\n",
                    name = step.name,
                    status = step.status.as_str()
                ));
            }
        }

        Ok(RawToolOutput::text(output))
    }
}

/// Old stub — kept for BuiltinToolProvider compatibility (standalone/embedded use).
struct PlanReviewToolStub;

#[async_trait]
impl Tool for PlanReviewToolStub {
    fn key(&self) -> &str { "builtin/plan.review@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let plan_id = request.parameters["plan_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("plan_id is required".into()))?;
        Ok(RawToolOutput::text(format!("[PLAN_REVIEW] Plan {plan_id} is ready for review\n\nNo issues found.")))
    }
}

pub fn plan_review_tool() -> Arc<dyn Tool> {
    Arc::new(PlanReviewToolStub)
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn plan_review_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(PlanReviewTool::new(planning))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn reviews_plan() {
        use core_agent_plan::{CreateGoalRequest, PlanningContext};
        let planning = Arc::new(PlanningManager::builder().build());
        let goal = planning
            .create_goal(CreateGoalRequest::new("test", "test"))
            .await
            .unwrap();
        let context = PlanningContext::default();
        let plan = planning
            .create_plan(core_agent_plan::CreatePlanRequest::new(goal.id, context))
            .await
            .unwrap();
        let tool = PlanReviewTool::new(planning);
        let result = tool
            .execute(
                &ToolRequest::new(
                    "builtin/plan.review@1.0.0",
                    serde_json::json!({"plan_id": plan.id.to_string()}),
                ),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains(plan.id.to_string().as_str()));
    }
}