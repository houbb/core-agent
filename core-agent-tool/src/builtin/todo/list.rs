use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::PlanningManager;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `todo.list` — List all todo items, optionally from a plan.
pub struct TodoListTool {
    planning: Option<Arc<PlanningManager>>,
}

impl TodoListTool {
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
impl Tool for TodoListTool {
    fn key(&self) -> &str {
        "builtin/todo.list@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        // If plan_id is provided, read from plan tasks
        if let Some(planning) = &self.planning {
            if let Some(plan_id_str) = request.parameters["plan_id"].as_str() {
                let plan_id = uuid::Uuid::parse_str(plan_id_str)
                    .map_err(|_| ToolError::InvalidArgument("invalid plan_id format".into()))?;
                let plan = planning
                    .find_plan(plan_id)
                    .await
                    .map_err(|e| ToolError::execution("todo.list", e.to_string(), false))?
                    .ok_or_else(|| ToolError::InvalidArgument("plan not found".into()))?;

                let mut output = format!("Plan: {}\nStatus: {}\n\nTasks:\n", plan.id, plan.status.as_str());
                for (i, task) in plan.tasks.values().enumerate() {
                    let marker = match task.status.as_str() {
                        "COMPLETED" => "✅",
                        "RUNNING" => "⏳",
                        "FAILED" => "❌",
                        "CANCELLED" => "⛔",
                        _ => "⬜",
                    };
                    output.push_str(&format!(
                        "  {}  {}. {}  [{}]\n",
                        marker, i + 1, task.name, task.status.as_str()
                    ));
                    for step in task.steps.values() {
                        let s_marker = match step.status.as_str() {
                            "COMPLETED" => "✅",
                            "RUNNING" => "⏳",
                            "FAILED" => "❌",
                            "CANCELLED" => "⛔",
                            _ => "⬜",
                        };
                        output.push_str(&format!("       {} {} [{}]\n", s_marker, step.name, step.status.as_str()));
                    }
                }
                return Ok(RawToolOutput::text(output));
            }
        }

        Ok(RawToolOutput::text("No todo items yet."))
    }
}

pub fn todo_list_tool() -> Arc<dyn Tool> {
    Arc::new(TodoListTool::new())
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn todo_list_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(TodoListTool::with_planning(planning))
}