use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::Utc;
use futures_util::FutureExt;
use uuid::Uuid;

use crate::defaults::{
    AllowAllPlanningPolicy, DefaultPlanningLifecycle, DefaultPlanningStrategy,
    InMemoryPlanningCatalog, RulePlanBuilder, StructuralPlanReviewer,
};
use crate::domain::{
    validate_actor, validate_metadata, Action, CreateGoalRequest, CreatePlanRequest, Goal,
    GoalStatus, Plan, PlanDraft, PlanSnapshot, PlanStatus, PlanningContext, PlanningEdge,
    PlanningGraph, PlanningNode, PlanningNodeKind, PlanningRelation, ReviewDecision, Step, Task,
    UpdateGoalRequest, UpdatePlanRequest, WorkStatus,
};
use crate::error::{PlanError, PlanResult};
use crate::infrastructure::{
    DynPlanBuilder, PlanBuilder, PlanReviewer, PlanningCatalog, PlanningInterceptor,
    PlanningLifecycle, PlanningObservation, PlanningObserver, PlanningOperation, PlanningPolicy,
    PlanningStage, PlanningStrategy,
};

pub struct GoalManager {
    catalog: Arc<dyn PlanningCatalog>,
}

impl GoalManager {
    pub fn new(catalog: Arc<dyn PlanningCatalog>) -> Self {
        Self { catalog }
    }

    pub async fn create(&self, request: CreateGoalRequest) -> PlanResult<Goal> {
        validate_actor(&request.actor)?;
        let now = Utc::now();
        let goal = Goal {
            id: Uuid::new_v4(),
            intent: request.intent,
            title: request.title,
            description: request.description,
            priority: request.priority,
            status: GoalStatus::Proposed,
            constraints: request.constraints,
            session_id: request.session_id,
            workspace_id: request.workspace_id,
            metadata: request.metadata,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        goal.validate()?;
        self.catalog.save_goal(&goal, &request.actor).await?;
        Ok(goal)
    }

    pub async fn update(&self, request: UpdateGoalRequest) -> PlanResult<Goal> {
        validate_actor(&request.actor)?;
        let mut goal = self.required(request.goal_id).await?;
        if goal.version != request.expected_version {
            return Err(PlanError::Conflict(format!(
                "goal {} expected version {}, actual {}",
                goal.id, request.expected_version, goal.version
            )));
        }
        goal.title = request.title;
        goal.description = request.description;
        goal.priority = request.priority;
        goal.constraints = request.constraints;
        goal.status = request.status;
        goal.metadata = request.metadata;
        goal.version = next_version(goal.version, "goal")?;
        goal.updated_at = Utc::now();
        goal.validate()?;
        self.catalog.save_goal(&goal, &request.actor).await?;
        Ok(goal)
    }

    pub async fn find(&self, id: Uuid) -> PlanResult<Option<Goal>> {
        self.catalog.find_goal(id).await
    }

    pub async fn list(&self) -> PlanResult<Vec<Goal>> {
        self.catalog.list_goals().await
    }

    async fn required(&self, id: Uuid) -> PlanResult<Goal> {
        self.find(id)
            .await?
            .ok_or_else(|| PlanError::NotFound(id.to_string()))
    }
}

#[derive(Default)]
pub struct TaskManager;

impl TaskManager {
    pub fn list<'a>(&self, plan: &'a Plan) -> Vec<&'a Task> {
        plan.tasks.values().collect()
    }

    pub fn find<'a>(&self, plan: &'a Plan, task_id: Uuid) -> Option<&'a Task> {
        plan.tasks.get(&task_id)
    }
}

#[derive(Default)]
pub struct StepManager;

impl StepManager {
    pub fn list<'a>(&self, plan: &'a Plan) -> Vec<&'a Step> {
        plan.tasks
            .values()
            .flat_map(|task| task.steps.values())
            .collect()
    }

    pub fn find<'a>(&self, plan: &'a Plan, step_id: Uuid) -> Option<&'a Step> {
        plan.tasks
            .values()
            .find_map(|task| task.steps.get(&step_id))
    }
}

pub struct PlanningManagerBuilder {
    catalog: Arc<dyn PlanningCatalog>,
    builders: Vec<DynPlanBuilder>,
    strategy: Arc<dyn PlanningStrategy>,
    reviewer: Arc<dyn PlanReviewer>,
    lifecycle: Arc<dyn PlanningLifecycle>,
    policy: Arc<dyn PlanningPolicy>,
    interceptors: Vec<Arc<dyn PlanningInterceptor>>,
    observers: Vec<Arc<dyn PlanningObserver>>,
}

impl Default for PlanningManagerBuilder {
    fn default() -> Self {
        Self {
            catalog: Arc::new(InMemoryPlanningCatalog::default()),
            builders: vec![Arc::new(RulePlanBuilder)],
            strategy: Arc::new(DefaultPlanningStrategy),
            reviewer: Arc::new(StructuralPlanReviewer),
            lifecycle: Arc::new(DefaultPlanningLifecycle),
            policy: Arc::new(AllowAllPlanningPolicy),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl PlanningManagerBuilder {
    pub fn catalog(mut self, value: Arc<dyn PlanningCatalog>) -> Self {
        self.catalog = value;
        self
    }

    pub fn builder(mut self, value: Arc<dyn PlanBuilder>) -> Self {
        self.builders.push(value);
        self
    }

    pub fn strategy(mut self, value: Arc<dyn PlanningStrategy>) -> Self {
        self.strategy = value;
        self
    }

    pub fn reviewer(mut self, value: Arc<dyn PlanReviewer>) -> Self {
        self.reviewer = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn PlanningLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn PlanningPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn PlanningInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn PlanningObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> PlanningManager {
        let builders = self
            .builders
            .into_iter()
            .map(|builder| (builder.key().to_string(), builder))
            .collect();
        PlanningManager {
            goal_manager: GoalManager::new(self.catalog.clone()),
            catalog: self.catalog,
            builders,
            strategy: self.strategy,
            reviewer: self.reviewer,
            lifecycle: self.lifecycle,
            policy: self.policy,
            interceptors: self.interceptors,
            observers: self.observers,
            task_manager: TaskManager,
            step_manager: StepManager,
        }
    }
}

pub struct PlanningManager {
    catalog: Arc<dyn PlanningCatalog>,
    goal_manager: GoalManager,
    builders: BTreeMap<String, DynPlanBuilder>,
    strategy: Arc<dyn PlanningStrategy>,
    reviewer: Arc<dyn PlanReviewer>,
    lifecycle: Arc<dyn PlanningLifecycle>,
    policy: Arc<dyn PlanningPolicy>,
    interceptors: Vec<Arc<dyn PlanningInterceptor>>,
    observers: Vec<Arc<dyn PlanningObserver>>,
    task_manager: TaskManager,
    step_manager: StepManager,
}

impl PlanningManager {
    pub fn builder() -> PlanningManagerBuilder {
        PlanningManagerBuilder::default()
    }

    pub fn new(catalog: Arc<dyn PlanningCatalog>) -> Self {
        Self::builder().catalog(catalog).build()
    }

    pub fn tasks(&self) -> &TaskManager {
        &self.task_manager
    }

    pub fn steps(&self) -> &StepManager {
        &self.step_manager
    }

    pub async fn create_goal(&self, request: CreateGoalRequest) -> PlanResult<Goal> {
        let result = async {
            self.evaluate_policy(PlanningOperation::CreateGoal, None, None)?;
            self.goal_manager.create(request).await
        }
        .await;
        self.notify_result(
            PlanningOperation::CreateGoal,
            PlanningStage::Goal,
            result.as_ref().ok().map(|goal| goal.id),
            None,
            &result,
        );
        result
    }

    pub async fn update_goal(&self, mut request: UpdateGoalRequest) -> PlanResult<Goal> {
        let goal_id = request.goal_id;
        let result = async {
            let current = self.required_goal(goal_id).await?;
            self.evaluate_policy(PlanningOperation::UpdateGoal, Some(&current), None)?;
            let immutable = (
                request.goal_id,
                request.expected_version,
                request.actor.clone(),
            );
            for interceptor in &self.interceptors {
                catch_unwind(AssertUnwindSafe(|| {
                    interceptor.before_goal_update(&mut request)
                }))
                .map_err(|_| PlanError::Extension("planning interceptor panicked".into()))??;
            }
            if immutable
                != (
                    request.goal_id,
                    request.expected_version,
                    request.actor.clone(),
                )
            {
                return Err(PlanError::Validation(
                    "goal interceptor changed immutable request identity".into(),
                ));
            }
            validate_actor(&request.actor)?;
            self.goal_manager.update(request).await
        }
        .await;
        self.notify_result(
            PlanningOperation::UpdateGoal,
            PlanningStage::Goal,
            Some(goal_id),
            None,
            &result,
        );
        result
    }

    pub async fn find_goal(&self, id: Uuid) -> PlanResult<Option<Goal>> {
        self.goal_manager.find(id).await
    }

    pub async fn list_goals(&self) -> PlanResult<Vec<Goal>> {
        self.goal_manager.list().await
    }

    pub async fn create_plan(&self, request: CreatePlanRequest) -> PlanResult<Plan> {
        let goal_id = request.goal_id;
        let result = self.create_plan_inner(request).await;
        self.notify_result(
            PlanningOperation::CreatePlan,
            if result.is_ok() {
                PlanningStage::Persist
            } else {
                PlanningStage::Build
            },
            Some(goal_id),
            result.as_ref().ok().map(|plan| plan.id),
            &result,
        );
        result
    }

    async fn create_plan_inner(&self, mut request: CreatePlanRequest) -> PlanResult<Plan> {
        validate_actor(&request.actor)?;
        request.context.validate()?;
        let goal = self.required_goal(request.goal_id).await?;
        self.evaluate_policy(PlanningOperation::CreatePlan, Some(&goal), None)?;
        let plan_id = Uuid::new_v4();
        let (builder_key, draft) = self
            .build_draft(&goal, &mut request.context, request.builder_key.as_deref())
            .await?;
        let now = Utc::now();
        let mut plan = Plan {
            id: plan_id,
            goal_id: goal.id,
            strategy_key: self.strategy.key().to_string(),
            status: PlanStatus::Created,
            tasks: BTreeMap::new(),
            graph: PlanningGraph::default(),
            review: None,
            metadata: BTreeMap::from([("builder_key".into(), serde_json::json!(builder_key))]),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.lifecycle.transition(&mut plan, PlanStatus::Planning)?;
        self.materialize(&goal, &mut plan, draft)?;
        validate_action_context(&plan, &request.context)?;
        self.review(&mut plan).await?;
        self.evaluate_policy(PlanningOperation::CreatePlan, Some(&goal), Some(&plan))?;
        self.catalog.save_plan(&plan, None, &request.actor).await?;
        Ok(plan)
    }

    pub async fn create_plan_from_draft(
        &self,
        goal_id: Uuid,
        draft: PlanDraft,
        context: PlanningContext,
        actor: &str,
    ) -> PlanResult<Plan> {
        let plan_id = Uuid::new_v4();
        let result = self
            .create_plan_from_draft_inner(plan_id, goal_id, draft, context, actor)
            .await;
        self.notify_result(
            PlanningOperation::CreatePlan,
            if result.is_ok() {
                PlanningStage::Persist
            } else {
                PlanningStage::Build
            },
            Some(goal_id),
            result.as_ref().ok().map(|plan| plan.id),
            &result,
        );
        result
    }

    async fn create_plan_from_draft_inner(
        &self,
        plan_id: Uuid,
        goal_id: Uuid,
        draft: PlanDraft,
        context: PlanningContext,
        actor: &str,
    ) -> PlanResult<Plan> {
        validate_actor(actor)?;
        context.validate()?;
        let goal = self.required_goal(goal_id).await?;
        self.evaluate_policy(PlanningOperation::CreatePlan, Some(&goal), None)?;
        let now = Utc::now();
        let mut plan = Plan {
            id: plan_id,
            goal_id: goal.id,
            strategy_key: "external".to_string(),
            status: PlanStatus::Created,
            tasks: BTreeMap::new(),
            graph: PlanningGraph::default(),
            review: None,
            metadata: BTreeMap::from([("builder_key".into(), serde_json::json!("external"))]),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.lifecycle.transition(&mut plan, PlanStatus::Planning)?;
        self.materialize(&goal, &mut plan, draft)?;
        validate_action_context(&plan, &context)?;
        self.review(&mut plan).await?;
        self.evaluate_policy(PlanningOperation::CreatePlan, Some(&goal), Some(&plan))?;
        self.catalog.save_plan(&plan, None, actor).await?;
        Ok(plan)
    }

    pub async fn update_plan(&self, request: UpdatePlanRequest) -> PlanResult<Plan> {
        let plan_id = request.plan_id;
        let result = self.update_plan_inner(request).await;
        self.notify_result(
            PlanningOperation::UpdatePlan,
            PlanningStage::Persist,
            result.as_ref().ok().map(|plan| plan.goal_id),
            Some(plan_id),
            &result,
        );
        result
    }

    async fn update_plan_inner(&self, mut request: UpdatePlanRequest) -> PlanResult<Plan> {
        validate_actor(&request.actor)?;
        request.context.validate()?;
        let mut plan = self.required_plan(request.plan_id).await?;
        self.require_version(&plan, request.expected_version)?;
        let goal = self.required_goal(plan.goal_id).await?;
        self.evaluate_policy(PlanningOperation::UpdatePlan, Some(&goal), Some(&plan))?;
        if matches!(
            plan.status,
            PlanStatus::Executing | PlanStatus::Completed | PlanStatus::Cancelled
        ) {
            return Err(PlanError::InvalidState(format!(
                "cannot update {} plan",
                plan.status.as_str()
            )));
        }
        let previous = PlanSnapshot::capture(&plan, "before-update")?;
        self.lifecycle.transition(&mut plan, PlanStatus::Planning)?;
        let (builder_key, draft) = self
            .build_draft(&goal, &mut request.context, request.builder_key.as_deref())
            .await?;
        plan.review = None;
        plan.version = next_version(plan.version, "plan")?;
        plan.metadata
            .insert("builder_key".into(), serde_json::json!(builder_key));
        self.materialize(&goal, &mut plan, draft)?;
        validate_action_context(&plan, &request.context)?;
        self.review(&mut plan).await?;
        self.evaluate_policy(PlanningOperation::UpdatePlan, Some(&goal), Some(&plan))?;
        self.catalog
            .save_plan(&plan, Some(&previous), &request.actor)
            .await?;
        Ok(plan)
    }

    pub async fn cancel_plan(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        let result = self.cancel_plan_inner(id, expected_version, actor).await;
        self.notify_result(
            PlanningOperation::CancelPlan,
            PlanningStage::Persist,
            result.as_ref().ok().map(|plan| plan.goal_id),
            Some(id),
            &result,
        );
        result
    }

    async fn cancel_plan_inner(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        validate_actor(actor)?;
        let mut plan = self.required_plan(id).await?;
        self.require_version(&plan, expected_version)?;
        let goal = self.required_goal(plan.goal_id).await?;
        self.evaluate_policy(PlanningOperation::CancelPlan, Some(&goal), Some(&plan))?;
        if matches!(plan.status, PlanStatus::Completed | PlanStatus::Cancelled) {
            return Err(PlanError::InvalidState(format!(
                "cannot cancel {} plan",
                plan.status.as_str()
            )));
        }
        let previous = PlanSnapshot::capture(&plan, "before-cancel")?;
        self.lifecycle
            .transition(&mut plan, PlanStatus::Cancelled)?;
        plan.version = next_version(plan.version, "plan")?;
        plan.validate()?;
        self.catalog
            .save_plan(&plan, Some(&previous), actor)
            .await?;
        Ok(plan)
    }

    pub async fn transition_plan(
        &self,
        id: Uuid,
        expected_version: u64,
        status: PlanStatus,
        actor: &str,
    ) -> PlanResult<Plan> {
        let result = self.transition_plan_inner(id, expected_version, status, actor).await;
        self.notify_result(
            PlanningOperation::UpdatePlan,
            PlanningStage::Persist,
            result.as_ref().ok().map(|plan| plan.goal_id),
            Some(id),
            &result,
        );
        result
    }

    async fn transition_plan_inner(
        &self,
        id: Uuid,
        expected_version: u64,
        status: PlanStatus,
        actor: &str,
    ) -> PlanResult<Plan> {
        validate_actor(actor)?;
        let mut plan = self.required_plan(id).await?;
        self.require_version(&plan, expected_version)?;
        let goal = self.required_goal(plan.goal_id).await?;
        self.evaluate_policy(PlanningOperation::UpdatePlan, Some(&goal), Some(&plan))?;
        if matches!(plan.status, PlanStatus::Completed) {
            return Err(PlanError::InvalidState(
                "cannot transition a completed plan".into(),
            ));
        }
        let previous = PlanSnapshot::capture(&plan, "before-transition")?;
        self.lifecycle.transition(&mut plan, status)?;
        plan.version = next_version(plan.version, "plan")?;
        plan.validate()?;
        self.catalog
            .save_plan(&plan, Some(&previous), actor)
            .await?;
        Ok(plan)
    }

    pub async fn resume_plan(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        let result = self.resume_plan_inner(id, expected_version, actor).await;
        self.notify_result(
            PlanningOperation::ResumePlan,
            PlanningStage::Persist,
            result.as_ref().ok().map(|plan| plan.goal_id),
            Some(id),
            &result,
        );
        result
    }

    async fn resume_plan_inner(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        validate_actor(actor)?;
        let current = self.required_plan(id).await?;
        self.require_version(&current, expected_version)?;
        if current.status != PlanStatus::Cancelled {
            return Err(PlanError::InvalidState(
                "only a cancelled plan can resume".into(),
            ));
        }
        let goal = self.required_goal(current.goal_id).await?;
        self.evaluate_policy(PlanningOperation::ResumePlan, Some(&goal), Some(&current))?;
        let source = self
            .catalog
            .list_snapshots(id)
            .await?
            .into_iter()
            .find(|snapshot| snapshot.label == "before-cancel")
            .ok_or_else(|| PlanError::NotFound(format!("resumable snapshot for plan {id}")))?;
        let previous = PlanSnapshot::capture(&current, "before-resume")?;
        let mut restored = source.content;
        restored.version = next_version(current.version, "plan")?;
        restored.updated_at = Utc::now();
        restored.validate()?;
        self.evaluate_policy(PlanningOperation::ResumePlan, Some(&goal), Some(&restored))?;
        self.catalog
            .save_plan(&restored, Some(&previous), actor)
            .await?;
        Ok(restored)
    }

    pub async fn snapshot_plan(
        &self,
        id: Uuid,
        label: &str,
        actor: &str,
    ) -> PlanResult<PlanSnapshot> {
        let result = async {
            validate_actor(actor)?;
            let plan = self.required_plan(id).await?;
            let goal = self.required_goal(plan.goal_id).await?;
            self.evaluate_policy(PlanningOperation::SnapshotPlan, Some(&goal), Some(&plan))?;
            let snapshot = PlanSnapshot::capture(&plan, label)?;
            self.catalog.save_snapshot(&snapshot, actor).await?;
            Ok(snapshot)
        }
        .await;
        self.notify_result(
            PlanningOperation::SnapshotPlan,
            PlanningStage::Snapshot,
            None,
            Some(id),
            &result,
        );
        result
    }

    pub async fn restore_plan(
        &self,
        snapshot_id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        let result = self
            .restore_plan_inner(snapshot_id, expected_version, actor)
            .await;
        self.notify_result(
            PlanningOperation::RestorePlan,
            PlanningStage::Snapshot,
            result.as_ref().ok().map(|plan| plan.goal_id),
            result.as_ref().ok().map(|plan| plan.id),
            &result,
        );
        result
    }

    async fn restore_plan_inner(
        &self,
        snapshot_id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> PlanResult<Plan> {
        validate_actor(actor)?;
        let source = self
            .catalog
            .find_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| PlanError::NotFound(snapshot_id.to_string()))?;
        let current = self.required_plan(source.plan_id).await?;
        self.require_version(&current, expected_version)?;
        let goal = self.required_goal(current.goal_id).await?;
        self.evaluate_policy(PlanningOperation::RestorePlan, Some(&goal), Some(&current))?;
        let previous = PlanSnapshot::capture(&current, "before-restore")?;
        let mut restored = source.content;
        restored.version = next_version(current.version, "plan")?;
        restored.updated_at = Utc::now();
        restored.validate()?;
        self.evaluate_policy(PlanningOperation::RestorePlan, Some(&goal), Some(&restored))?;
        self.catalog
            .save_plan(&restored, Some(&previous), actor)
            .await?;
        Ok(restored)
    }

    pub async fn find_plan(&self, id: Uuid) -> PlanResult<Option<Plan>> {
        self.catalog.find_plan(id).await
    }

    pub async fn list_plans(&self, goal_id: Uuid) -> PlanResult<Vec<Plan>> {
        self.catalog.list_plans(goal_id).await
    }

    pub async fn list_snapshots(&self, plan_id: Uuid) -> PlanResult<Vec<PlanSnapshot>> {
        self.catalog.list_snapshots(plan_id).await
    }

    async fn build_draft(
        &self,
        goal: &Goal,
        context: &mut crate::domain::PlanningContext,
        requested_builder: Option<&str>,
    ) -> PlanResult<(String, PlanDraft)> {
        validate_context_identity(goal, context)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_build(goal, context)))
                .map_err(|_| PlanError::Extension("planning interceptor panicked".into()))??;
        }
        context.validate()?;
        validate_context_identity(goal, context)?;
        let builder_key = if let Some(key) = requested_builder {
            key.to_string()
        } else {
            catch_unwind(AssertUnwindSafe(|| {
                self.strategy.select_builder(goal, context)
            }))
            .map_err(|_| PlanError::Extension("planning strategy panicked".into()))??
        };
        let builder = self
            .builders
            .get(&builder_key)
            .ok_or_else(|| PlanError::BuilderNotFound(builder_key.clone()))?;
        let mut draft = AssertUnwindSafe(builder.build(goal, context))
            .catch_unwind()
            .await
            .map_err(|_| {
                PlanError::Extension(format!("plan builder `{builder_key}` panicked"))
            })??;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.after_build(goal, &mut draft)
            }))
            .map_err(|_| PlanError::Extension("planning interceptor panicked".into()))??;
        }
        Ok((builder_key, draft))
    }

    async fn review(&self, plan: &mut Plan) -> PlanResult<()> {
        self.lifecycle.transition(plan, PlanStatus::Reviewing)?;
        plan.validate()?;
        let review = AssertUnwindSafe(self.reviewer.review(plan))
            .catch_unwind()
            .await
            .map_err(|_| {
                PlanError::Extension(format!("plan reviewer `{}` panicked", self.reviewer.key()))
            })??;
        review.validate()?;
        plan.review = Some(review);
        if plan
            .review
            .as_ref()
            .is_some_and(|review| review.decision == ReviewDecision::Approved)
        {
            self.lifecycle.transition(plan, PlanStatus::Ready)?;
        }
        plan.validate()
    }

    fn materialize(&self, goal: &Goal, plan: &mut Plan, draft: PlanDraft) -> PlanResult<()> {
        validate_metadata(&draft.metadata)?;
        if draft.tasks.is_empty() || draft.tasks.len() > 256 {
            return Err(PlanError::Validation(
                "plan draft must contain 1..=256 tasks".into(),
            ));
        }
        let mut task_keys = BTreeSet::new();
        let mut step_keys = BTreeSet::new();
        for task in &draft.tasks {
            if !task_keys.insert(task.key.clone()) {
                return Err(PlanError::Validation(format!(
                    "duplicate task key `{}`",
                    task.key
                )));
            }
            if task.steps.is_empty() || task.steps.len() > 256 {
                return Err(PlanError::Validation(format!(
                    "task `{}` must contain 1..=256 steps",
                    task.key
                )));
            }
            for step in &task.steps {
                if !step_keys.insert(step.key.clone()) {
                    return Err(PlanError::Validation(format!(
                        "duplicate step key `{}`",
                        step.key
                    )));
                }
            }
        }
        if step_keys.len() > 1024 {
            return Err(PlanError::Validation(
                "plan draft has more than 1024 total steps".into(),
            ));
        }
        let task_ids = task_keys
            .iter()
            .map(|key| {
                (
                    key.clone(),
                    Uuid::new_v5(&plan.id, format!("task:{key}").as_bytes()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let step_ids = step_keys
            .iter()
            .map(|key| {
                (
                    key.clone(),
                    Uuid::new_v5(&plan.id, format!("step:{key}").as_bytes()),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut tasks = BTreeMap::new();
        for task_draft in draft.tasks {
            let task_id = task_ids[&task_draft.key];
            let dependencies = task_draft
                .depends_on
                .iter()
                .map(|key| {
                    task_ids.get(key).copied().ok_or_else(|| {
                        PlanError::Validation(format!("unknown task dependency `{key}`"))
                    })
                })
                .collect::<PlanResult<Vec<_>>>()?;
            let mut steps = BTreeMap::new();
            for step_draft in task_draft.steps {
                let step_id = step_ids[&step_draft.key];
                let step_dependencies = step_draft
                    .depends_on
                    .iter()
                    .map(|key| {
                        step_ids.get(key).copied().ok_or_else(|| {
                            PlanError::Validation(format!("unknown step dependency `{key}`"))
                        })
                    })
                    .collect::<PlanResult<Vec<_>>>()?;
                let action = Action {
                    id: Uuid::new_v5(&plan.id, format!("action:{}", step_draft.key).as_bytes()),
                    kind: step_draft.action.kind,
                    tool_key: step_draft.action.tool_key,
                    capability: step_draft.action.capability,
                    target_uri: step_draft.action.target_uri,
                    parameters: step_draft.action.parameters,
                };
                let step = Step {
                    id: step_id,
                    plan_id: plan.id,
                    task_id,
                    key: step_draft.key,
                    name: step_draft.name,
                    status: WorkStatus::Pending,
                    dependencies: step_dependencies,
                    max_attempts: step_draft.max_attempts,
                    action,
                    metadata: step_draft.metadata,
                };
                step.validate()?;
                steps.insert(step.id, step);
            }
            let task = Task {
                id: task_id,
                plan_id: plan.id,
                key: task_draft.key,
                name: task_draft.name,
                status: WorkStatus::Pending,
                priority: task_draft.priority,
                dependencies,
                steps,
                metadata: task_draft.metadata,
            };
            task.validate()?;
            tasks.insert(task.id, task);
        }
        plan.tasks = tasks;
        plan.metadata.extend(draft.metadata);
        plan.graph = build_graph(goal, plan);
        plan.updated_at = Utc::now();
        plan.validate()
    }

    fn require_version(&self, plan: &Plan, expected: u64) -> PlanResult<()> {
        if plan.version != expected {
            return Err(PlanError::Conflict(format!(
                "plan {} expected version {}, actual {}",
                plan.id, expected, plan.version
            )));
        }
        Ok(())
    }

    async fn required_goal(&self, id: Uuid) -> PlanResult<Goal> {
        self.catalog
            .find_goal(id)
            .await?
            .ok_or_else(|| PlanError::NotFound(id.to_string()))
    }

    async fn required_plan(&self, id: Uuid) -> PlanResult<Plan> {
        self.catalog
            .find_plan(id)
            .await?
            .ok_or_else(|| PlanError::NotFound(id.to_string()))
    }

    fn evaluate_policy(
        &self,
        operation: PlanningOperation,
        goal: Option<&Goal>,
        plan: Option<&Plan>,
    ) -> PlanResult<()> {
        catch_unwind(AssertUnwindSafe(|| {
            self.policy.evaluate(operation, goal, plan)
        }))
        .map_err(|_| PlanError::Extension("planning policy panicked".into()))?
    }

    fn notify_result<T>(
        &self,
        operation: PlanningOperation,
        stage: PlanningStage,
        goal_id: Option<Uuid>,
        plan_id: Option<Uuid>,
        result: &PlanResult<T>,
    ) where
        T: ObservationVersion,
    {
        let observation = PlanningObservation {
            operation,
            stage,
            success: result.is_ok(),
            goal_id,
            plan_id,
            plan_version: result.as_ref().ok().and_then(ObservationVersion::version),
            message: result.as_ref().err().map(ToString::to_string),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

trait ObservationVersion {
    fn version(&self) -> Option<u64>;
}

impl ObservationVersion for Goal {
    fn version(&self) -> Option<u64> {
        Some(self.version)
    }
}

impl ObservationVersion for Plan {
    fn version(&self) -> Option<u64> {
        Some(self.version)
    }
}

impl ObservationVersion for PlanSnapshot {
    fn version(&self) -> Option<u64> {
        Some(self.plan_version)
    }
}

fn build_graph(goal: &Goal, plan: &Plan) -> PlanningGraph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    if let Some(intent) = &goal.intent {
        nodes.push(PlanningNode {
            id: intent.id,
            kind: PlanningNodeKind::Intent,
            label: intent.title.clone(),
        });
        edges.push(PlanningEdge {
            source: intent.id,
            target: goal.id,
            relation: PlanningRelation::Contains,
        });
    }
    nodes.push(PlanningNode {
        id: goal.id,
        kind: PlanningNodeKind::Goal,
        label: goal.title.clone(),
    });
    nodes.push(PlanningNode {
        id: plan.id,
        kind: PlanningNodeKind::Plan,
        label: goal.title.clone(),
    });
    edges.push(PlanningEdge {
        source: goal.id,
        target: plan.id,
        relation: PlanningRelation::Contains,
    });
    for task in plan.tasks.values() {
        nodes.push(PlanningNode {
            id: task.id,
            kind: PlanningNodeKind::Task,
            label: task.name.clone(),
        });
        edges.push(PlanningEdge {
            source: plan.id,
            target: task.id,
            relation: PlanningRelation::Contains,
        });
        edges.extend(task.dependencies.iter().map(|dependency| PlanningEdge {
            source: task.id,
            target: *dependency,
            relation: PlanningRelation::DependsOn,
        }));
        for step in task.steps.values() {
            nodes.push(PlanningNode {
                id: step.id,
                kind: PlanningNodeKind::Step,
                label: step.name.clone(),
            });
            nodes.push(PlanningNode {
                id: step.action.id,
                kind: PlanningNodeKind::Action,
                label: step.action.kind.as_str().into(),
            });
            edges.push(PlanningEdge {
                source: task.id,
                target: step.id,
                relation: PlanningRelation::Contains,
            });
            edges.push(PlanningEdge {
                source: step.id,
                target: step.action.id,
                relation: PlanningRelation::Contains,
            });
            edges.extend(step.dependencies.iter().map(|dependency| PlanningEdge {
                source: step.id,
                target: *dependency,
                relation: PlanningRelation::DependsOn,
            }));
        }
    }
    PlanningGraph { nodes, edges }
}

fn next_version(current: u64, entity: &str) -> PlanResult<u64> {
    current
        .checked_add(1)
        .ok_or_else(|| PlanError::Validation(format!("{entity} version overflow")))
}

fn validate_context_identity(
    goal: &Goal,
    context: &crate::domain::PlanningContext,
) -> PlanResult<()> {
    if goal.session_id.is_some() && goal.session_id != context.session_id {
        return Err(PlanError::Validation(
            "planning context session does not match the goal".into(),
        ));
    }
    if goal.workspace_id.is_some()
        && goal.workspace_id != context.workspace.as_ref().map(|workspace| workspace.id)
    {
        return Err(PlanError::Validation(
            "planning context workspace does not match the goal".into(),
        ));
    }
    Ok(())
}

fn validate_action_context(
    plan: &Plan,
    context: &crate::domain::PlanningContext,
) -> PlanResult<()> {
    for action in plan
        .tasks
        .values()
        .flat_map(|task| task.steps.values().map(|step| &step.action))
        .filter(|action| action.kind == crate::domain::ActionKind::InvokeTool)
    {
        let tool_key = action
            .tool_key
            .as_deref()
            .ok_or_else(|| PlanError::Validation("tool action is missing its tool key".into()))?;
        let tool = context
            .tools
            .iter()
            .find(|tool| tool.key == tool_key)
            .ok_or_else(|| {
                PlanError::Validation(format!(
                    "planned tool `{tool_key}` is not present in planning context"
                ))
            })?;
        let capability = action.capability.as_deref().ok_or_else(|| {
            PlanError::Validation(format!(
                "planned tool `{tool_key}` is missing its capability"
            ))
        })?;
        if !tool
            .capabilities
            .iter()
            .any(|available| available == capability)
        {
            return Err(PlanError::Validation(format!(
                "planned capability `{capability}` is not available on tool `{tool_key}`"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PlanningContext, PlanningRequestKind};

    #[tokio::test]
    async fn manager_creates_reviewed_ready_plan() {
        let manager = PlanningManager::builder().build();
        let goal = manager
            .create_goal(CreateGoalRequest::new("implement", "implement planning"))
            .await
            .unwrap();
        let context = PlanningContext {
            request_kind: PlanningRequestKind::Coding,
            ..PlanningContext::default()
        };
        let plan = manager
            .create_plan(CreatePlanRequest::new(goal.id, context))
            .await
            .unwrap();
        assert_eq!(plan.status, PlanStatus::Ready);
        assert_eq!(plan.tasks.len(), 3);
        assert_eq!(
            plan.review.as_ref().unwrap().decision,
            ReviewDecision::Approved
        );
        assert_eq!(manager.tasks().list(&plan).len(), 3);
        assert_eq!(manager.steps().list(&plan).len(), 3);
        let mut missing_edge = plan.clone();
        missing_edge.graph.edges.pop();
        assert!(missing_edge.validate().is_err());
        let mut incomplete = plan;
        incomplete.status = PlanStatus::Completed;
        assert!(incomplete.validate().is_err());
    }

    #[tokio::test]
    async fn optimistic_version_prevents_lost_update() {
        let manager = PlanningManager::builder().build();
        let goal = manager
            .create_goal(CreateGoalRequest::new("implement", "implement planning"))
            .await
            .unwrap();
        let plan = manager
            .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
            .await
            .unwrap();
        assert!(matches!(
            manager
                .cancel_plan(plan.id, plan.version + 1, "tester")
                .await,
            Err(PlanError::Conflict(_))
        ));
    }
}
