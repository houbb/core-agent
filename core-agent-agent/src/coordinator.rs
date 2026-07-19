use std::sync::Arc;

use async_trait::async_trait;
use core_agent_execution::{ExecuteRequest, Execution, ExecutionManager};
use core_agent_plan::{CreatePlanRequest, PlanningManager};
use uuid::Uuid;

use crate::domain::{Agent, AgentGoalRequest, AgentRunReference};
use crate::error::{AgentError, AgentResult};
use crate::infrastructure::{AgentCoordinator, AgentExecutionControl};

pub struct RuntimeAgentCoordinator {
    planning: Arc<PlanningManager>,
    execution: Arc<ExecutionManager>,
}

impl RuntimeAgentCoordinator {
    pub fn new(planning: Arc<PlanningManager>, execution: Arc<ExecutionManager>) -> Self {
        Self {
            planning,
            execution,
        }
    }
}

#[async_trait]
impl AgentCoordinator for RuntimeAgentCoordinator {
    async fn next(
        &self,
        agent: &Agent,
        mut request: AgentGoalRequest,
    ) -> AgentResult<AgentRunReference> {
        bind_id("session", agent.session_id, &mut request.goal.session_id)?;
        bind_id(
            "workspace",
            agent.workspace_id,
            &mut request.goal.workspace_id,
        )?;
        bind_id(
            "planning context session",
            agent.session_id,
            &mut request.context.session_id,
        )?;
        if let Some(workspace) = &request.context.workspace {
            if let Some(bound) = agent.workspace_id {
                if workspace.id != bound {
                    return Err(AgentError::Validation(
                        "planning context workspace does not match Agent binding".into(),
                    ));
                }
            }
            match request.goal.workspace_id {
                Some(id) if id != workspace.id => {
                    return Err(AgentError::Validation(
                        "Goal and planning context workspace IDs differ".into(),
                    ))
                }
                None => request.goal.workspace_id = Some(workspace.id),
                _ => {}
            }
        } else if agent.workspace_id.is_some() {
            return Err(AgentError::Validation(
                "workspace-bound Agent requires a Planning workspace reference".into(),
            ));
        }
        for tool in &request.context.tools {
            if !agent.profile.toolset.contains(&tool.key) {
                return Err(AgentError::PolicyDenied(format!(
                    "Tool {} is not declared by Agent Profile {}",
                    tool.key, agent.profile.key
                )));
            }
        }

        let actor = request.goal.actor.clone();
        let goal = self.planning.create_goal(request.goal).await?;
        let mut create_plan = CreatePlanRequest::new(goal.id, request.context);
        create_plan.builder_key = agent.profile.planner_key.clone();
        create_plan.actor = actor.clone();
        let plan = self
            .planning
            .create_plan(create_plan)
            .await
            .map_err(|error| AgentError::PartialCoordination {
                stage: "PLAN".into(),
                goal_id: Some(goal.id),
                plan_id: None,
                execution_id: None,
                message: error.to_string(),
            })?;
        let plan_id = plan.id;
        let execution = self
            .execution
            .prepare(plan, ExecuteRequest::new(actor))
            .await
            .map_err(|error| AgentError::PartialCoordination {
                stage: "PREPARE_EXECUTION".into(),
                goal_id: Some(goal.id),
                plan_id: Some(plan_id),
                execution_id: None,
                message: error.to_string(),
            })?;
        Ok(AgentRunReference {
            goal_id: goal.id,
            plan_id: execution.plan_id,
            execution_id: execution.id,
        })
    }

    async fn run(
        &self,
        reference: &AgentRunReference,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        Ok(self
            .execution
            .start_with_control(reference.execution_id, control.execution_control())
            .await?)
    }

    async fn resume(
        &self,
        execution_id: Uuid,
        actor: &str,
        control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        Ok(self
            .execution
            .resume_with_control(execution_id, actor, control.execution_control())
            .await?)
    }

    async fn pause(&self, execution_id: Uuid) -> AgentResult<Execution> {
        self.execution.pause(execution_id).await?;
        self.execution
            .find(execution_id)
            .await?
            .ok_or_else(|| AgentError::NotFound(execution_id.to_string()))
    }

    async fn find_execution(&self, execution_id: Uuid) -> AgentResult<Option<Execution>> {
        Ok(self.execution.find(execution_id).await?)
    }
}

pub struct UnavailableAgentCoordinator;

#[async_trait]
impl AgentCoordinator for UnavailableAgentCoordinator {
    async fn next(
        &self,
        _agent: &Agent,
        _request: AgentGoalRequest,
    ) -> AgentResult<AgentRunReference> {
        Err(AgentError::Coordination(
            "Planning and Execution runtimes are not configured".into(),
        ))
    }

    async fn run(
        &self,
        _reference: &AgentRunReference,
        _control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        Err(AgentError::Coordination(
            "Execution runtime is not configured".into(),
        ))
    }

    async fn resume(
        &self,
        _execution_id: Uuid,
        _actor: &str,
        _control: &AgentExecutionControl,
    ) -> AgentResult<Execution> {
        Err(AgentError::Coordination(
            "Execution runtime is not configured".into(),
        ))
    }

    async fn pause(&self, _execution_id: Uuid) -> AgentResult<Execution> {
        Err(AgentError::Coordination(
            "Execution runtime is not configured".into(),
        ))
    }

    async fn find_execution(&self, _execution_id: Uuid) -> AgentResult<Option<Execution>> {
        Err(AgentError::Coordination(
            "Execution runtime is not configured".into(),
        ))
    }
}

fn bind_id(label: &str, bound: Option<Uuid>, value: &mut Option<Uuid>) -> AgentResult<()> {
    match (bound, *value) {
        (Some(expected), Some(actual)) if expected != actual => Err(AgentError::Validation(
            format!("{label} does not match Agent binding"),
        )),
        (Some(expected), None) => {
            *value = Some(expected);
            Ok(())
        }
        _ => Ok(()),
    }
}
