use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use core_agent::integrations::ToolActionExecutor;
use core_agent::{
    AgentGoalRequest, AgentManager, AgentProfile, AgentState, CreateAgentRequest,
    CreateGoalRequest, ExecutionManager, ExecutionStatus, FunctionTool, PermissionDecision,
    PlanningContext, PlanningManager, RawToolOutput, RuntimeAgentCoordinator, StaticToolProvider,
    ToolCapability, ToolDefinition, ToolManager, ToolProviderDefinition, ToolProviderKind,
    ToolReference, ToolRegistration,
};

#[tokio::test]
async fn agent_coordinates_planning_execution_and_tool_runtime() {
    let calls = Arc::new(AtomicUsize::new(0));
    let tools = Arc::new(ToolManager::builder().build());
    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let mut definition = ToolDefinition::new(
        "builtin",
        "agent-echo",
        "1.0.0",
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    );
    definition.default_permission = PermissionDecision::Allow;
    definition
        .capabilities
        .insert(ToolCapability::new("execute").unwrap());
    let observed = Arc::clone(&calls);
    let tool = Arc::new(FunctionTool::new(
        definition.key.clone(),
        move |request, _| {
            let observed = Arc::clone(&observed);
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                assert!(request.metadata.contains_key("execution_id"));
                Ok(RawToolOutput::text("agent tool completed"))
            }
        },
    ));
    tools
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![ToolRegistration::new(definition.clone(), tool)],
        ))
        .await
        .unwrap();

    let planning = Arc::new(PlanningManager::builder().build());
    let execution = Arc::new(
        ExecutionManager::builder()
            .executor(Arc::new(ToolActionExecutor::new(tools)))
            .build(),
    );
    let manager = AgentManager::builder()
        .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
        .build();
    let mut profile = AgentProfile::new("tool-user", "Tool User");
    profile.toolset.insert(definition.key.clone());
    let profile = manager.register_profile(profile, "test").await.unwrap();
    let agent = manager
        .create(CreateAgentRequest::new("tool-agent", profile.id))
        .await
        .unwrap();
    manager.start(agent.id, "test").await.unwrap();

    let mut context = PlanningContext::default();
    context.tools.push(ToolReference {
        key: definition.key,
        name: definition.name,
        capabilities: vec!["execute".into()],
    });
    let outcome = manager
        .run_goal(
            agent.id,
            AgentGoalRequest::new(
                CreateGoalRequest::new("use tool", "execute through the Tool Runtime"),
                context,
            ),
        )
        .await
        .unwrap();

    assert_eq!(outcome.execution_status, ExecutionStatus::Completed);
    assert_eq!(outcome.agent.state, AgentState::Waiting);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
