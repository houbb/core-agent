use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::integrations::{
    AgentAssignmentResolver, RuntimeAgentDirectory, RuntimeAgentDispatcher, ToolActionExecutor,
};
use core_agent::{
    AgentCapability, AgentGoalRequest, AgentManager, AgentProfile, AgentState, AssignmentRequest,
    CollaborationState, CreateAgentRequest, CreateGoalRequest, CreateTeamRequest, ExecutionManager,
    FunctionTool, MultiAgentManager, Organization, PermissionDecision, PlanningContext,
    PlanningManager, RawToolOutput, RuntimeAgentCoordinator, StaticToolProvider, ToolCapability,
    ToolDefinition, ToolManager, ToolProviderDefinition, ToolProviderKind, ToolReference,
    ToolRegistration,
};

struct AssignmentResolver {
    context: PlanningContext,
}

#[async_trait]
impl AgentAssignmentResolver for AssignmentResolver {
    async fn resolve(
        &self,
        _dispatch_id: uuid::Uuid,
        _agent_id: uuid::Uuid,
        message: &core_agent::AgentMessage,
    ) -> Result<AgentGoalRequest, String> {
        let mut goal = CreateGoalRequest::new("team assignment", "execute assigned Team work");
        goal.actor = message.actor.clone();
        Ok(AgentGoalRequest::new(goal, self.context.clone()))
    }
}

#[tokio::test]
async fn team_routes_a_typed_assignment_through_real_agent_planning_execution_and_tool() {
    let calls = Arc::new(AtomicUsize::new(0));
    let tools = Arc::new(ToolManager::builder().build());
    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let mut definition = ToolDefinition::new(
        "builtin",
        "team-echo",
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
    let observed = calls.clone();
    let tool = Arc::new(FunctionTool::new(definition.key.clone(), move |_, _| {
        let observed = observed.clone();
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(RawToolOutput::text("team work completed"))
        }
    }));
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
    let agents = Arc::new(
        AgentManager::builder()
            .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
            .build(),
    );
    let mut profile = AgentProfile::new("team-coder", "Team Coder");
    profile
        .capabilities
        .insert(AgentCapability::new("code.write").unwrap());
    profile.toolset.insert(definition.key.clone());
    let profile = agents.register_profile(profile, "test").await.unwrap();
    let agent = agents
        .create(CreateAgentRequest::new("coder", profile.id))
        .await
        .unwrap();
    agents.start(agent.id, "test").await.unwrap();

    let mut context = PlanningContext::default();
    context.tools.push(ToolReference {
        key: definition.key,
        name: definition.name,
        capabilities: vec!["execute".into()],
    });
    let teams = MultiAgentManager::builder()
        .directory(Arc::new(RuntimeAgentDirectory::new(agents.clone())))
        .dispatcher(Arc::new(RuntimeAgentDispatcher::new(
            agents.clone(),
            Arc::new(AssignmentResolver { context }),
        )))
        .build();
    let organization = teams
        .create_organization(Organization::new("engineering", "Engineering", "lead"))
        .await
        .unwrap();
    let mut role = core_agent::MultiAgentRole::new(organization.id, "coder", "Coder", "lead");
    role.required_capabilities.insert("code.write".into());
    let role = teams.create_role(role).await.unwrap();
    let team = teams
        .create_team(CreateTeamRequest::new(
            organization.id,
            "coding",
            "Coding Team",
            "Ship change",
            "lead",
        ))
        .await
        .unwrap();
    teams
        .join(team.id, role.id, agent.id, "lead")
        .await
        .unwrap();
    teams.activate_team(team.id, "lead").await.unwrap();
    let collaboration = teams
        .assign(AssignmentRequest::new(team.id, "Use the tool", "lead"))
        .await
        .unwrap();

    assert_eq!(collaboration.state, CollaborationState::Completed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        agents.find(agent.id).await.unwrap().unwrap().state,
        AgentState::Waiting
    );
}
