use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::domain::{
    validate_actor, ActionDraft, ActionKind, Goal, Plan, PlanDraft, PlanReview, PlanSnapshot,
    PlanStatus, PlanningContext, PlanningNodeKind, PlanningRequestKind, ReviewDecision, StepDraft,
    TaskDraft,
};
use crate::error::{PlanError, PlanResult};
use crate::infrastructure::{
    GoalStore, PlanBuilder, PlanReviewer, PlanSnapshotStore, PlanStore, PlanningLifecycle,
    PlanningObservation, PlanningObserver, PlanningOperation, PlanningPolicy, PlanningStrategy,
};

#[derive(Default)]
struct MemoryState {
    goals: BTreeMap<Uuid, Goal>,
    plans: BTreeMap<Uuid, Plan>,
    snapshots: BTreeMap<Uuid, PlanSnapshot>,
}

#[derive(Default)]
pub struct InMemoryPlanningCatalog {
    state: RwLock<MemoryState>,
}

#[async_trait]
impl GoalStore for InMemoryPlanningCatalog {
    async fn save_goal(&self, goal: &Goal, actor: &str) -> PlanResult<()> {
        goal.validate()?;
        validate_actor(actor)?;
        let mut state = self
            .state
            .write()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?;
        let valid_version = match state.goals.get(&goal.id) {
            None => goal.version == 1,
            Some(current) => current.version.checked_add(1) == Some(goal.version),
        };
        if !valid_version {
            return Err(PlanError::Conflict(format!(
                "goal {} was concurrently modified",
                goal.id
            )));
        }
        state.goals.insert(goal.id, goal.clone());
        Ok(())
    }

    async fn find_goal(&self, id: Uuid) -> PlanResult<Option<Goal>> {
        Ok(self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .goals
            .get(&id)
            .cloned())
    }

    async fn list_goals(&self) -> PlanResult<Vec<Goal>> {
        Ok(self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .goals
            .values()
            .cloned()
            .collect())
    }
}

#[async_trait]
impl PlanStore for InMemoryPlanningCatalog {
    async fn save_plan(
        &self,
        plan: &Plan,
        previous: Option<&PlanSnapshot>,
        actor: &str,
    ) -> PlanResult<()> {
        plan.validate()?;
        validate_actor(actor)?;
        if let Some(snapshot) = previous {
            snapshot.validate()?;
        }
        let mut state = self
            .state
            .write()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?;
        let goal = state
            .goals
            .get(&plan.goal_id)
            .ok_or_else(|| PlanError::NotFound(plan.goal_id.to_string()))?;
        ensure_intent_matches(goal, plan)?;
        match (state.plans.get(&plan.id), previous) {
            (None, None) if plan.version == 1 => {}
            (Some(current), Some(snapshot))
                if current == &snapshot.content
                    && current.version == snapshot.plan_version
                    && plan.goal_id == current.goal_id
                    && plan.created_at == current.created_at
                    && current.version.checked_add(1) == Some(plan.version) => {}
            _ => {
                return Err(PlanError::Conflict(format!(
                    "plan {} was concurrently modified",
                    plan.id
                )))
            }
        }
        if let Some(snapshot) = previous {
            if state.snapshots.contains_key(&snapshot.id) {
                return Err(PlanError::Conflict(format!(
                    "snapshot {} already exists",
                    snapshot.id
                )));
            }
            state.snapshots.insert(snapshot.id, snapshot.clone());
        }
        state.plans.insert(plan.id, plan.clone());
        Ok(())
    }

    async fn find_plan(&self, id: Uuid) -> PlanResult<Option<Plan>> {
        Ok(self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .plans
            .get(&id)
            .cloned())
    }

    async fn list_plans(&self, goal_id: Uuid) -> PlanResult<Vec<Plan>> {
        Ok(self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .plans
            .values()
            .filter(|plan| plan.goal_id == goal_id)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl PlanSnapshotStore for InMemoryPlanningCatalog {
    async fn save_snapshot(&self, snapshot: &PlanSnapshot, actor: &str) -> PlanResult<()> {
        snapshot.validate()?;
        validate_actor(actor)?;
        let mut state = self
            .state
            .write()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?;
        if !state.plans.contains_key(&snapshot.plan_id) {
            return Err(PlanError::NotFound(snapshot.plan_id.to_string()));
        }
        if state.snapshots.contains_key(&snapshot.id) {
            return Err(PlanError::Conflict(format!(
                "snapshot {} already exists",
                snapshot.id
            )));
        }
        state.snapshots.insert(snapshot.id, snapshot.clone());
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> PlanResult<Option<PlanSnapshot>> {
        Ok(self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .snapshots
            .get(&id)
            .cloned())
    }

    async fn list_snapshots(&self, plan_id: Uuid) -> PlanResult<Vec<PlanSnapshot>> {
        let mut snapshots = self
            .state
            .read()
            .map_err(|_| PlanError::Internal("planning catalog lock poisoned".into()))?
            .snapshots
            .values()
            .filter(|snapshot| snapshot.plan_id == plan_id)
            .cloned()
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse((snapshot.created_at, snapshot.id)));
        Ok(snapshots)
    }
}

fn ensure_intent_matches(goal: &Goal, plan: &Plan) -> PlanResult<()> {
    let plan_intent = plan
        .graph
        .nodes
        .iter()
        .find(|node| node.kind == PlanningNodeKind::Intent)
        .map(|node| node.id);
    let goal_intent = goal.intent.as_ref().map(|intent| intent.id);
    if plan_intent != goal_intent {
        return Err(PlanError::Validation(
            "plan graph intent does not match its goal".into(),
        ));
    }
    Ok(())
}

pub struct DefaultPlanningLifecycle;

impl PlanningLifecycle for DefaultPlanningLifecycle {
    fn transition(&self, plan: &mut Plan, next: PlanStatus) -> PlanResult<()> {
        if plan.status == next {
            return Ok(());
        }
        if !plan.status.can_transition_to(next) {
            return Err(PlanError::InvalidState(format!(
                "{} -> {}",
                plan.status.as_str(),
                next.as_str()
            )));
        }
        plan.status = next;
        plan.updated_at = Utc::now();
        Ok(())
    }
}

pub struct AllowAllPlanningPolicy;

impl PlanningPolicy for AllowAllPlanningPolicy {
    fn evaluate(
        &self,
        _operation: PlanningOperation,
        _goal: Option<&Goal>,
        _plan: Option<&Plan>,
    ) -> PlanResult<()> {
        Ok(())
    }
}

pub struct NoopPlanningObserver;

impl PlanningObserver for NoopPlanningObserver {
    fn on_observation(&self, _observation: &PlanningObservation) {}
}

pub struct DefaultPlanningStrategy;

impl PlanningStrategy for DefaultPlanningStrategy {
    fn key(&self) -> &str {
        "default"
    }

    fn select_builder(&self, _goal: &Goal, _context: &PlanningContext) -> PlanResult<String> {
        Ok("rule".into())
    }
}

pub struct ExternalPlanBuilder {
    pub draft: PlanDraft,
}

impl ExternalPlanBuilder {
    pub fn new(draft: PlanDraft) -> Self {
        Self { draft }
    }
}

#[async_trait]
impl PlanBuilder for ExternalPlanBuilder {
    fn key(&self) -> &str {
        "external"
    }

    async fn build(&self, _goal: &Goal, _context: &PlanningContext) -> PlanResult<PlanDraft> {
        Ok(self.draft.clone())
    }
}

pub struct RulePlanBuilder;

impl RulePlanBuilder {
    fn step(key: &str, name: &str, kind: ActionKind, depends_on: Vec<&str>) -> StepDraft {
        StepDraft {
            key: key.into(),
            name: name.into(),
            depends_on: depends_on.into_iter().map(str::to_string).collect(),
            max_attempts: 1,
            action: ActionDraft {
                kind,
                tool_key: None,
                capability: None,
                target_uri: None,
                parameters: json!({}),
            },
            metadata: BTreeMap::new(),
        }
    }

    fn task(
        key: &str,
        name: &str,
        priority: i32,
        depends_on: Vec<&str>,
        steps: Vec<StepDraft>,
    ) -> TaskDraft {
        TaskDraft {
            key: key.into(),
            name: name.into(),
            priority,
            depends_on: depends_on.into_iter().map(str::to_string).collect(),
            steps,
            metadata: BTreeMap::new(),
        }
    }
}

#[async_trait]
impl PlanBuilder for RulePlanBuilder {
    fn key(&self) -> &str {
        "rule"
    }

    async fn build(&self, goal: &Goal, context: &PlanningContext) -> PlanResult<PlanDraft> {
        goal.validate()?;
        context.validate()?;
        let (first, second, third) = match context.request_kind {
            PlanningRequestKind::Coding => ("分析范围", "实现变更", "验证结果"),
            PlanningRequestKind::RootCauseAnalysis => ("收集证据", "验证假设", "形成结论"),
            PlanningRequestKind::Report => ("收集材料", "组织内容", "生成报告"),
            PlanningRequestKind::General => ("理解目标", "完成目标", "审查结果"),
        };
        let mut implementation = Self::step("execute", second, ActionKind::Produce, vec![]);
        if let Some((tool, matched_capability)) = context.tools.iter().find_map(|tool| {
            tool.capabilities.iter().find_map(|capability| {
                let normalized = capability.to_ascii_lowercase();
                (normalized == "write"
                    || normalized == "execute"
                    || normalized == "apply_patch"
                    || normalized.ends_with(".write")
                    || normalized.ends_with(".execute")
                    || normalized.ends_with(".apply_patch"))
                .then_some((tool, capability.clone()))
            })
        }) {
            implementation.action.kind = ActionKind::InvokeTool;
            implementation.action.tool_key = Some(tool.key.clone());
            implementation.action.capability = Some(matched_capability);
        }
        let tasks = vec![
            Self::task(
                "analyze",
                first,
                100,
                vec![],
                vec![Self::step("analyze", first, ActionKind::Analyze, vec![])],
            ),
            Self::task("execute", second, 80, vec!["analyze"], vec![implementation]),
            Self::task(
                "verify",
                third,
                60,
                vec!["execute"],
                vec![Self::step(
                    "verify",
                    third,
                    ActionKind::Verify,
                    vec!["execute"],
                )],
            ),
        ];
        Ok(PlanDraft {
            tasks,
            metadata: BTreeMap::from([("goal_title".into(), json!(goal.title))]),
        })
    }
}

pub struct StructuralPlanReviewer;

#[async_trait]
impl PlanReviewer for StructuralPlanReviewer {
    fn key(&self) -> &str {
        "structural"
    }

    async fn review(&self, plan: &Plan) -> PlanResult<PlanReview> {
        let decision = match plan.validate() {
            Ok(()) => ReviewDecision::Approved,
            Err(_) => ReviewDecision::Rejected,
        };
        Ok(PlanReview {
            decision,
            findings: if decision == ReviewDecision::Approved {
                Vec::new()
            } else {
                vec!["plan structure is invalid".into()]
            },
            reviewer_key: self.key().into(),
            reviewed_at: Utc::now(),
        })
    }
}

/// LLM-driven plan builder.
///
/// Accepts a PlanDraft (generated externally by an LLM) and validates it.
/// The LLM is expected to produce JSON matching the PlanDraft schema.
pub struct LLMPlanBuilder {
    pub draft: PlanDraft,
}

impl LLMPlanBuilder {
    pub fn new(draft: PlanDraft) -> Self {
        Self { draft }
    }

    /// Build from a JSON string (LLM output).
    pub fn from_json(json_str: &str) -> PlanResult<Self> {
        let draft: PlanDraft = serde_json::from_str(json_str)
            .map_err(|e| PlanError::Validation(format!("LLM PlanDraft JSON parse error: {e}")))?;
        if draft.tasks.is_empty() {
            return Err(PlanError::Validation(
                "LLM PlanDraft must contain at least 1 task".into(),
            ));
        }
        Ok(Self { draft })
    }
}

#[async_trait]
impl PlanBuilder for LLMPlanBuilder {
    fn key(&self) -> &str {
        "llm"
    }

    async fn build(&self, _goal: &Goal, _context: &PlanningContext) -> PlanResult<PlanDraft> {
        Ok(self.draft.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CreateGoalRequest;
    use crate::manager::GoalManager;
    use std::sync::Arc;

    #[tokio::test]
    async fn rule_builder_returns_three_dependent_tasks() {
        let catalog = Arc::new(InMemoryPlanningCatalog::default());
        let goal = GoalManager::new(catalog)
            .create(CreateGoalRequest::new("ship", "ship a change"))
            .await
            .unwrap();
        let draft = RulePlanBuilder
            .build(&goal, &PlanningContext::default())
            .await
            .unwrap();
        assert_eq!(draft.tasks.len(), 3);
        assert_eq!(draft.tasks[1].depends_on, vec!["analyze"]);
        assert_eq!(draft.tasks[2].depends_on, vec!["execute"]);
    }
}
