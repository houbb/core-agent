use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::PlanningManager;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `todo.add` — Add a todo item, optionally linked to a plan task.
pub struct TodoAddTool {
    planning: Option<Arc<PlanningManager>>,
}

impl TodoAddTool {
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
impl Tool for TodoAddTool {
    fn key(&self) -> &str {
        "builtin/todo.add@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let task = request.parameters["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("task is required".into()))?;
        if task.is_empty() {
            return Err(ToolError::InvalidArgument("task must not be empty".into()));
        }

        // If plan_id is provided, link to a plan task
        if let Some(planning) = &self.planning {
            if let Some(plan_id_str) = request.parameters["plan_id"].as_str() {
                let plan_id = uuid::Uuid::parse_str(plan_id_str)
                    .map_err(|_| ToolError::InvalidArgument("invalid plan_id format".into()))?;
                let plan = planning
                    .find_plan(plan_id)
                    .await
                    .map_err(|e| ToolError::execution("todo.add", e.to_string(), false))?
                    .ok_or_else(|| ToolError::InvalidArgument("plan not found".into()))?;

                let mut output = format!("[TODO] Added: {task}\n\nLinked to Plan: {}\n\nCurrent Tasks:\n", plan.id);
                for (i, t) in plan.tasks.values().enumerate() {
                    let marker = match t.status.as_str() {
                        "COMPLETED" => "✅",
                        "RUNNING" => "⏳",
                        "FAILED" => "❌",
                        "CANCELLED" => "⛔",
                        _ => "⬜",
                    };
                    output.push_str(&format!("  {}  {}. {}  [{}]\n", marker, i + 1, t.name, t.status.as_str()));
                    for step in t.steps.values() {
                        let s_marker = match step.status.as_str() {
                            "COMPLETED" => "✅",
                            "RUNNING" => "⏳",
                            "FAILED" => "❌",
                            "CANCELLED" => "⛔",
                            _ => "⬜",
                        };
                        output.push_str(&format!("       {} {}\n", s_marker, step.name));
                    }
                }
                return Ok(RawToolOutput::text(output));
            }
        }

        Ok(RawToolOutput::text(format!("[TODO] Added: {task}")))
    }
}

pub fn todo_add_tool() -> Arc<dyn Tool> {
    Arc::new(TodoAddTool::new())
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn todo_add_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(TodoAddTool::with_planning(planning))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn adds_todo() {
        let tool = TodoAddTool::new();
        let request = ToolRequest::new(
            "builtin/todo.add@1.0.0",
            serde_json::json!({"task": "Implement login"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Implement login"));
    }
}