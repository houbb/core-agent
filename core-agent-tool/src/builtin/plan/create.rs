use std::sync::Arc;

use async_trait::async_trait;
use core_agent_plan::{
    ActionDraft, ActionKind, CreateGoalRequest, PlanDraft, PlanningContext, PlanningManager,
    StepDraft, TaskDraft,
};

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `plan.create` — Create an execution plan using PlanningManager.
pub struct PlanCreateTool {
    planning: Arc<PlanningManager>,
}

impl PlanCreateTool {
    pub fn new(planning: Arc<PlanningManager>) -> Self {
        Self { planning }
    }
}

#[async_trait]
impl Tool for PlanCreateTool {
    fn key(&self) -> &str {
        "builtin/plan.create@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _ctx: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let goal = request.parameters["goal"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("goal is required".into()))?
            .to_string();
        let description = request.parameters["description"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // 1. Create goal
        let mut goal_req = CreateGoalRequest::new(&goal, &description);
        goal_req.actor = "assistant".into();
        let created_goal = self
            .planning
            .create_goal(goal_req)
            .await
            .map_err(|e| ToolError::execution("plan.create", e.to_string(), false))?;

        // 2. Build plan draft from LLM-provided tasks
        let tasks = request.parameters["tasks"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidArgument("tasks array is required".into()))?;

        let mut task_drafts = Vec::new();
        for (i, task) in tasks.iter().enumerate() {
            let task_name = task["name"]
                .as_str()
                .unwrap_or(&format!("Task {}", i + 1))
                .to_string();
            let task_key = task["key"]
                .as_str()
                .unwrap_or(&format!("task_{}", i + 1))
                .to_string();
            let steps = task["steps"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .enumerate()
                        .map(|(j, step)| {
                            let step_name = step["name"]
                                .as_str()
                                .unwrap_or(&format!("Step {}", j + 1))
                                .to_string();
                            StepDraft {
                                key: format!("{}_step_{}", task_key, j + 1),
                                name: step_name,
                                depends_on: vec![],
                                max_attempts: 1,
                                action: ActionDraft {
                                    kind: ActionKind::Produce,
                                    tool_key: step["tool_key"].as_str().map(String::from),
                                    capability: None,
                                    target_uri: None,
                                    parameters: serde_json::json!({}),
                                },
                                metadata: std::collections::BTreeMap::new(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_else(|| {
                    vec![StepDraft {
                        key: format!("{}_step_1", task_key),
                        name: task_name.to_string(),
                        depends_on: vec![],
                        max_attempts: 1,
                        action: ActionDraft {
                            kind: ActionKind::Produce,
                            tool_key: None,
                            capability: None,
                            target_uri: None,
                            parameters: serde_json::json!({}),
                        },
                        metadata: std::collections::BTreeMap::new(),
                    }]
                });
            task_drafts.push(TaskDraft {
                key: task_key.to_string(),
                name: task_name.to_string(),
                priority: (tasks.len() - i) as i32 * 10,
                depends_on: if i > 0 {
                    vec![format!("task_{}", i)]
                } else {
                    vec![]
                },
                steps,
                metadata: std::collections::BTreeMap::new(),
            });
        }

        let draft = PlanDraft {
            tasks: task_drafts,
            metadata: std::collections::BTreeMap::new(),
        };

        let context = PlanningContext::default();

        // 3. Create plan from draft
        let plan = self
            .planning
            .create_plan_from_draft(created_goal.id, draft, context, "assistant")
            .await
            .map_err(|e| ToolError::execution("plan.create", e.to_string(), false))?;

        // 4. Format output
        let mut output = format!(
            "[PLAN_CREATE] Plan created successfully\n\nGoal: {goal}\nPlan ID: {}\nStatus: {}\nTasks:\n",
            plan.id,
            plan.status.as_str()
        );
        for task in plan.tasks.values() {
            output.push_str(&format!(
                "\n  [{status}] {name} (key: {key})",
                status = task.status.as_str(),
                name = task.name,
                key = task.key
            ));
            for step in task.steps.values() {
                output.push_str(&format!(
                    "\n    - {name} [{status}]",
                    name = step.name,
                    status = step.status.as_str()
                ));
            }
        }

        Ok(RawToolOutput::text(output))
    }
}

/// Old stub — kept for BuiltinToolProvider compatibility (standalone/embedded use).
struct PlanCreateToolStub;

#[async_trait]
impl Tool for PlanCreateToolStub {
    fn key(&self) -> &str {
        "builtin/plan.create@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _ctx: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let goal = request.parameters["goal"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("goal is required".into()))?;
        Ok(RawToolOutput::text(format!(
            "[PLAN_CREATE] Plan created for: {goal}"
        )))
    }
}

pub fn plan_create_tool() -> Arc<dyn Tool> {
    Arc::new(PlanCreateToolStub)
}

/// New factory — requires `Arc<PlanningManager>`.
pub fn plan_create_tool_with_planning(planning: Arc<PlanningManager>) -> Arc<dyn Tool> {
    Arc::new(PlanCreateTool::new(planning))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn creates_plan() {
        use core_agent_plan::PlanningManager;
        let planning = Arc::new(PlanningManager::builder().build());
        let tool = PlanCreateTool::new(planning);
        let result = tool
            .execute(
                &ToolRequest::new(
                    "builtin/plan.create@1.0.0",
                    serde_json::json!({
                        "goal": "refactor module",
                        "tasks": [{"name": "Analyze", "key": "analyze", "steps": [{"name": "Review code"}]}]
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
        assert!(text.contains("Plan created"));
    }
}