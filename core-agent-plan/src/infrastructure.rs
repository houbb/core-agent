use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    Goal, Plan, PlanDraft, PlanSnapshot, PlanStatus, PlanningContext, UpdateGoalRequest,
};
use crate::error::PlanResult;

#[async_trait]
pub trait GoalProvider: Send + Sync {
    async fn provide(&self, context: &PlanningContext) -> PlanResult<Vec<Goal>>;
}

pub trait PlanningStrategy: Send + Sync {
    fn key(&self) -> &str;
    fn select_builder(&self, goal: &Goal, context: &PlanningContext) -> PlanResult<String>;
}

#[async_trait]
pub trait PlanBuilder: Send + Sync {
    fn key(&self) -> &str;
    async fn build(&self, goal: &Goal, context: &PlanningContext) -> PlanResult<PlanDraft>;
}

#[async_trait]
pub trait PlanReviewer: Send + Sync {
    fn key(&self) -> &str;
    async fn review(&self, plan: &Plan) -> PlanResult<crate::domain::PlanReview>;
}

#[async_trait]
pub trait TaskScheduler: Send + Sync {
    async fn schedule(&self, plan: &Plan) -> PlanResult<Vec<Uuid>>;
}

#[async_trait]
pub trait GoalStore: Send + Sync {
    async fn save_goal(&self, goal: &Goal, actor: &str) -> PlanResult<()>;
    async fn find_goal(&self, id: Uuid) -> PlanResult<Option<Goal>>;
    async fn list_goals(&self) -> PlanResult<Vec<Goal>>;
}

#[async_trait]
pub trait PlanStore: Send + Sync {
    /// Atomically saves the current Plan and an optional previous-state snapshot.
    async fn save_plan(
        &self,
        plan: &Plan,
        previous: Option<&PlanSnapshot>,
        actor: &str,
    ) -> PlanResult<()>;
    async fn find_plan(&self, id: Uuid) -> PlanResult<Option<Plan>>;
    async fn list_plans(&self, goal_id: Uuid) -> PlanResult<Vec<Plan>>;
}

#[async_trait]
pub trait PlanSnapshotStore: Send + Sync {
    async fn save_snapshot(&self, snapshot: &PlanSnapshot, actor: &str) -> PlanResult<()>;
    async fn find_snapshot(&self, id: Uuid) -> PlanResult<Option<PlanSnapshot>>;
    async fn list_snapshots(&self, plan_id: Uuid) -> PlanResult<Vec<PlanSnapshot>>;
}

pub trait PlanningCatalog: GoalStore + PlanStore + PlanSnapshotStore {}
impl<T> PlanningCatalog for T where T: GoalStore + PlanStore + PlanSnapshotStore {}

pub trait PlanningLifecycle: Send + Sync {
    fn transition(&self, plan: &mut Plan, next: PlanStatus) -> PlanResult<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningOperation {
    CreateGoal,
    UpdateGoal,
    CreatePlan,
    UpdatePlan,
    CancelPlan,
    ResumePlan,
    SnapshotPlan,
    RestorePlan,
}

pub trait PlanningPolicy: Send + Sync {
    fn evaluate(
        &self,
        operation: PlanningOperation,
        goal: Option<&Goal>,
        plan: Option<&Plan>,
    ) -> PlanResult<()>;
}

pub trait PlanningInterceptor: Send + Sync {
    fn before_build(&self, _goal: &Goal, _context: &mut PlanningContext) -> PlanResult<()> {
        Ok(())
    }

    fn after_build(&self, _goal: &Goal, _draft: &mut PlanDraft) -> PlanResult<()> {
        Ok(())
    }

    fn before_goal_update(&self, _request: &mut UpdateGoalRequest) -> PlanResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningStage {
    Goal,
    Build,
    Review,
    Persist,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct PlanningObservation {
    pub operation: PlanningOperation,
    pub stage: PlanningStage,
    pub success: bool,
    pub goal_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub plan_version: Option<u64>,
    pub message: Option<String>,
}

pub trait PlanningObserver: Send + Sync {
    fn on_observation(&self, observation: &PlanningObservation);
}

pub type DynPlanBuilder = Arc<dyn PlanBuilder>;
