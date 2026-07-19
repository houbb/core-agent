use std::collections::BTreeMap;

use core_agent::integrations::planning_context;
use core_agent::{
    ActionKind, CreateGoalRequest, CreatePlanRequest, PlanningManager, PlanningRequestKind,
    ToolCapability, ToolDefinition, Workspace, WorkspaceState,
};

#[tokio::test]
async fn workspace_and_tool_catalog_feed_a_ready_plan_without_execution() {
    let mut workspace = Workspace::new("demo", "local", "file:///demo/", BTreeMap::new()).unwrap();
    let mut tool = ToolDefinition::new(
        "builtin",
        "write_file",
        "1.0.0",
        serde_json::json!({"type": "object"}),
    );
    tool.capabilities
        .insert(ToolCapability::new("filesystem.read").unwrap());
    tool.capabilities
        .insert(ToolCapability::new("filesystem.write").unwrap());

    let unavailable = planning_context(
        Some(&workspace),
        &[tool.clone()],
        PlanningRequestKind::Coding,
    );
    assert!(unavailable.workspace.is_none());
    workspace.transition(WorkspaceState::Loaded).unwrap();
    workspace.transition(WorkspaceState::Ready).unwrap();

    let context = planning_context(
        Some(&workspace),
        &[tool.clone()],
        PlanningRequestKind::Coding,
    );
    let manager = PlanningManager::builder().build();
    let mut request = CreateGoalRequest::new("实现功能", "在 Workspace 中实现并验证功能");
    request.workspace_id = Some(workspace.id);
    let goal = manager.create_goal(request).await.unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, context))
        .await
        .unwrap();

    assert_eq!(plan.status, core_agent::PlanStatus::Ready);
    assert_eq!(plan.goal_id, goal.id);
    assert!(plan
        .tasks
        .values()
        .flat_map(|task| task.steps.values())
        .any(|step| {
            step.action.kind == ActionKind::InvokeTool
                && step.action.tool_key.as_deref() == Some(tool.key.as_str())
                && step.action.capability.as_deref() == Some("filesystem.write")
        }));
    assert!(manager.find_plan(plan.id).await.unwrap().is_some());
}
