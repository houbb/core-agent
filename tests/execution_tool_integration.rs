use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::integrations::ToolActionExecutor;
use core_agent::{
    ActionDraft, ActionExecutor, ActionKind, CommandKind, CreateGoalRequest, CreatePlanRequest,
    ExecuteRequest, ExecutionCommand, ExecutionControl, ExecutionManager, ExecutionStatus,
    FunctionTool, PermissionDecision, PlanBuilder, PlanDraft, PlanningContext, PlanningManager,
    RawToolOutput, StaticToolProvider, StepDraft, TaskDraft, ToolDefinition, ToolManager,
    ToolProviderDefinition, ToolProviderKind, ToolReference, ToolRegistration,
};

struct OneToolPlan;

#[async_trait]
impl PlanBuilder for OneToolPlan {
    fn key(&self) -> &str {
        "one-tool"
    }

    async fn build(
        &self,
        _goal: &core_agent::Goal,
        _context: &PlanningContext,
    ) -> core_agent::PlanResult<PlanDraft> {
        Ok(PlanDraft {
            tasks: vec![TaskDraft {
                key: "execute".into(),
                name: "Execute Tool".into(),
                priority: 1,
                depends_on: Vec::new(),
                steps: vec![StepDraft {
                    key: "echo".into(),
                    name: "Echo".into(),
                    depends_on: Vec::new(),
                    max_attempts: 1,
                    action: ActionDraft {
                        kind: ActionKind::InvokeTool,
                        tool_key: Some("builtin/echo@1.0.0".into()),
                        capability: Some("utility.echo".into()),
                        target_uri: Some("file:///workspace/output.txt".into()),
                        parameters: serde_json::json!({"message": "hello"}),
                    },
                    metadata: BTreeMap::new(),
                }],
                metadata: BTreeMap::new(),
            }],
            metadata: BTreeMap::new(),
        })
    }
}

#[tokio::test]
async fn pre_cancelled_tool_command_never_starts_tool_runtime() {
    let adapter = ToolActionExecutor::new(Arc::new(ToolManager::builder().build()));
    let control = ExecutionControl::default();
    control.cancel();
    let execution_id = uuid::Uuid::new_v4();
    let command = ExecutionCommand {
        id: uuid::Uuid::new_v4(),
        execution_id,
        task_id: uuid::Uuid::new_v4(),
        step_id: uuid::Uuid::new_v4(),
        action_id: uuid::Uuid::new_v4(),
        attempt: 1,
        kind: CommandKind::Tool,
        action_kind: ActionKind::InvokeTool,
        tool_key: Some("builtin/not-registered@1.0.0".into()),
        capability: None,
        target_uri: None,
        parameters: serde_json::json!({}),
    };
    let failure = adapter.execute(&command, &control).await.unwrap_err();
    assert!(failure.cancelled);
}

#[tokio::test]
async fn approved_plan_executes_through_tool_runtime_adapter() {
    let manager = Arc::new(ToolManager::builder().build());
    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let mut definition = ToolDefinition::new(
        "builtin",
        "echo",
        "1.0.0",
        serde_json::json!({
            "type": "object",
            "required": ["message"],
            "properties": {"message": {"type": "string"}},
            "additionalProperties": false
        }),
    );
    definition.default_permission = PermissionDecision::Allow;
    definition
        .capabilities
        .insert(core_agent::ToolCapability::new("utility.echo").unwrap());
    let tool = Arc::new(FunctionTool::new(
        definition.key.clone(),
        |request, _| async move {
            assert_eq!(
                request
                    .metadata
                    .get("approved_capability")
                    .map(String::as_str),
                Some("utility.echo")
            );
            assert_eq!(
                request
                    .metadata
                    .get("approved_target_uri")
                    .map(String::as_str),
                Some("file:///workspace/output.txt")
            );
            Ok(RawToolOutput::text(
                request.parameters["message"].as_str().unwrap().to_owned(),
            ))
        },
    ));
    manager
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![ToolRegistration::new(definition.clone(), tool)],
        ))
        .await
        .unwrap();

    let planning = PlanningManager::builder()
        .builder(Arc::new(OneToolPlan))
        .build();
    let goal = planning
        .create_goal(CreateGoalRequest::new("echo", "run the approved Tool"))
        .await
        .unwrap();
    let mut context = PlanningContext::default();
    context.tools.push(ToolReference {
        key: definition.key,
        name: definition.name,
        capabilities: vec!["utility.echo".into()],
    });
    let mut request = CreatePlanRequest::new(goal.id, context);
    request.builder_key = Some("one-tool".into());
    let plan = planning.create_plan(request).await.unwrap();

    let execution = ExecutionManager::builder()
        .executor(Arc::new(ToolActionExecutor::new(manager)))
        .build()
        .execute(plan, ExecuteRequest::new("integration-test"))
        .await
        .unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);
    let result = execution
        .steps
        .values()
        .next()
        .unwrap()
        .result
        .as_ref()
        .unwrap();
    assert_eq!(result.summary, "Tool builtin/echo@1.0.0 completed");
    assert!(result.output_bytes > 0);
}
