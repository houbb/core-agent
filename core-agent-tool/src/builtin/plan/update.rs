use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::{PlanningManager, PlanStatus};

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.update` — Update plan status.
pub struct PlanUpdateTool {
    planning: Arc<PlanningManager>,
}

impl PlanUpdateTool {
    pub fn new(planning: Arc<PlanningManager>) -> Self {
        Self { planning }
    }
}

#[async_trait]
impl Tool for PlanUpdateTool {
    fn key(&self) -> &str {
        "builtin/plan.update@1.0.0"
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
        let status_str = request.parameters["status"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("status is required".into()))?;
        let version = request.parameters["version"].as_u64().unwrap_or(1);

        let plan_uuid = uuid::Uuid::parse_str(&plan_id)
            .map_err(|_| ToolError::InvalidArgument("invalid plan_id format".into()))?;

        let status = match status_str.to_uppercase().as_str() {
            "READY" => PlanStatus::Ready,
            "CANCELLED" => PlanStatus::Cancelled,
            "PLANNING" => PlanStatus::Planning,
            "REVIEWING" => PlanStatus::Reviewing,
            "CREATED" => PlanStatus::Created,
            "EXECUTING" => PlanStatus::Executing,
            "COMPLETED" => PlanStatus::Completed,
            "FAILED" => PlanStatus::Failed,
            _ => {
                return Err(ToolError::InvalidArgument(format!(
                    "unknown status: {status_str}"
                )))
            }
        };

        let plan = self
            .planning
            .transition_plan(plan_uuid, version, status, "assistant")
            .await
            .map_err(|e| ToolError::execution("plan.update", e.to_string(), false))?;

        Ok(RawToolOutput::text(format!(
            "[PLAN_UPDATE] Plan {plan_id} -> {}\nVersion: {}",
            status.as_str(),
            plan.version
        )))
    }
}

/// Old stub — kept for BuiltinToolProvider compatibility (standalone/embedded use).
struct PlanUpdateToolStub;

#[async_trait]
impl Tool for PlanUpdateToolStub {
    fn key(&self) -> &str { "builtin/plan.update@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let plan_id = request.parameters["plan_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("plan_id is required".into()))?;
        let status = request.parameters["status"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("status is required".into()))?;
        Ok(RawToolOutput::text(format!("[PLAN_UPDATE] Plan {plan_id} -> {status}")))
    }
}

pub fn plan_update_tool() -> Arc<dyn Tool> {
    Arc::new(PlanUpdateToolStub)
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn plan_update_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(PlanUpdateTool::new(planning))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn updates_plan() {
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
        let tool = PlanUpdateTool::new(planning.clone());
        let result = tool
            .execute(
                &ToolRequest::new(
                    "builtin/plan.update@1.0.0",
                    serde_json::json!({
                        "plan_id": plan.id.to_string(),
                        "status": "cancelled",
                        "version": plan.version
                    }),
                ),
                &ToolContext::default(),
            )
            .await
            .unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("CANCELLED"));
    }
}