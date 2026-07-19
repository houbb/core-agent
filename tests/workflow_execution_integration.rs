use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::integrations::{ExecutionWorkflowEngine, WorkflowPlanResolver};
use core_agent::{
    CreateGoalRequest, CreatePlanRequest, ExecutionManager, ExecutionStatus, Plan, PlanningContext,
    PlanningManager, StartWorkflowRequest, WorkflowAction, WorkflowActionContext, WorkflowActivity,
    WorkflowDefinition, WorkflowManager, WorkflowStageDefinition, WorkflowState,
};

struct FixedPlanResolver {
    plan: Plan,
    calls: AtomicUsize,
}

#[async_trait]
impl WorkflowPlanResolver for FixedPlanResolver {
    async fn resolve(
        &self,
        action: &WorkflowAction,
        context: &WorkflowActionContext,
    ) -> Result<Plan, String> {
        assert_eq!(action.kind, "execution.plan");
        assert_eq!(context.attempt, 1);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.plan.clone())
    }
}

#[tokio::test]
async fn workflow_prepares_and_runs_a_real_execution_without_executing_tools_itself() {
    let planning = PlanningManager::builder().build();
    let goal = planning
        .create_goal(CreateGoalRequest::new(
            "workflow integration",
            "execute one approved business Activity",
        ))
        .await
        .unwrap();
    let plan = planning
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    let resolver = Arc::new(FixedPlanResolver {
        plan,
        calls: AtomicUsize::new(0),
    });
    let executions = Arc::new(ExecutionManager::builder().build());
    let engine = Arc::new(ExecutionWorkflowEngine::new(
        executions.clone(),
        resolver.clone(),
    ));
    let workflows = WorkflowManager::builder().engine(engine).build();
    let definition = WorkflowDefinition::new(
        "release",
        "Release",
        vec![WorkflowStageDefinition::new(
            "verify",
            "Verify",
            vec![WorkflowActivity::new(
                "quality-gate",
                "Quality gate",
                vec![WorkflowAction::new(
                    "execute-plan",
                    "Execute approved Plan",
                    "execution.plan",
                )],
            )],
        )],
        "designer",
    )
    .unwrap();
    workflows.register(definition).await.unwrap();

    let instance = workflows
        .start(StartWorkflowRequest::new("release", "workflow-operator"))
        .await
        .unwrap();
    assert_eq!(instance.state, WorkflowState::Completed);
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    let binding = instance
        .action_progress()
        .next()
        .unwrap()
        .binding
        .as_ref()
        .unwrap();
    let execution = executions.find(binding.external_id).await.unwrap().unwrap();
    assert_eq!(execution.status, ExecutionStatus::Completed);
    let instance_id = instance.id.to_string();
    assert_eq!(
        execution
            .metadata
            .get("workflow_instance_id")
            .map(String::as_str),
        Some(instance_id.as_str())
    );
}
