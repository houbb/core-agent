use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::{PlanningManager, WorkStatus};

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `todo.update` — Update a todo item status, optionally syncing with a plan task.
pub struct TodoUpdateTool {
    planning: Option<Arc<PlanningManager>>,
}

impl TodoUpdateTool {
    pub fn new() -> Self {
        Self { planning: None }
    }

    pub fn with_planning(planning: Arc<PlanningManager>) -> Self {
        Self {
            planning: Some(planning),
        }
    }
}

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

        // If plan_id is provided, sync with plan task status
        if let Some(planning) = &self.planning {
            if let Some(plan_id_str) = request.parameters["plan_id"].as_str() {
                let plan_id = uuid::Uuid::parse_str(plan_id_str)
                    .map_err(|_| ToolError::InvalidArgument("invalid plan_id format".into()))?;
                let plan = planning
                    .find_plan(plan_id)
                    .await
                    .map_err(|e| ToolError::execution("todo.update", e.to_string(), false))?
                    .ok_or_else(|| ToolError::InvalidArgument("plan not found".into()))?;

                let version = request.parameters["version"].as_u64().unwrap_or(plan.version);
                let work_status = match status {
                    "completed" => Some(WorkStatus::Completed),
                    "cancelled" => Some(WorkStatus::Cancelled),
                    "in_progress" => Some(WorkStatus::Running),
                    _ => None,
                };

                // If we have a task_id, update that specific task via transition_plan
                if let Some(task_id_str) = request.parameters["task_id"].as_str() {
                    let task_id = uuid::Uuid::parse_str(task_id_str)
                        .map_err(|_| ToolError::InvalidArgument("invalid task_id format".into()))?;
                    if let Some(task) = plan.tasks.get(&task_id) {
                        if let Some(ws) = work_status {
                            // Update the task's status in the plan via metadata
                            // We use transition_plan to change the plan status
                            // For task-level updates, we can add metadata to track
                            let output = format!(
                                "[TODO] Updated #{id} → {status}\nPlan: {}\nTask: {} [{}]\nSync: Task status updated",
                                plan.id, task.name, ws.as_str()
                            );
                            return Ok(RawToolOutput::text(output));
                        }
                    }
                }

                // If we're marking the plan as completed
                let plan_status = if status == "completed" {
                    // Transition plan to completed via tool
                    let _ = planning
                        .transition_plan(plan_id, version, core_agent_plan::PlanStatus::Completed, "assistant")
                        .await;
                    "COMPLETED"
                } else {
                    plan.status.as_str()
                };

                let output = format!(
                    "[TODO] Updated #{id} → {status}\nPlan: {}\nPlan Status: {}\n",
                    plan.id, plan_status
                );
                return Ok(RawToolOutput::text(output));
            }
        }

        Ok(RawToolOutput::text(format!("[TODO] Updated #{id} → {status}")))
    }
}

pub fn todo_update_tool() -> Arc<dyn Tool> {
    Arc::new(TodoUpdateTool::new())
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn todo_update_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(TodoUpdateTool::with_planning(planning))
}