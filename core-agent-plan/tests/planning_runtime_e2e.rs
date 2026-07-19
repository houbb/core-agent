use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use core_agent_plan::{
    ActionDraft, ActionKind, CreateGoalRequest, CreatePlanRequest, Plan, PlanBuilder, PlanDraft,
    PlanError, PlanReview, PlanReviewer, PlanSnapshot, PlanSnapshotStore, PlanStatus, PlanStore,
    PlanningContext, PlanningInterceptor, PlanningManager, PlanningObservation, PlanningObserver,
    PlanningOperation, PlanningPolicy, ReviewDecision, SqlitePlanningStore, StepDraft, TaskDraft,
    UpdateGoalRequest, UpdatePlanRequest,
};

#[tokio::test]
async fn sqlite_plan_can_update_cancel_resume_and_restore() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("planning.db");
    let store = Arc::new(SqlitePlanningStore::new(&database).unwrap());
    let manager = PlanningManager::new(store.clone());
    let goal = manager
        .create_goal(CreateGoalRequest::new("交付功能", "生成可恢复的计划"))
        .await
        .unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    let manual = manager
        .snapshot_plan(plan.id, "manual", "tester")
        .await
        .unwrap();
    assert!(matches!(
        store.save_snapshot(&manual, "tester").await,
        Err(PlanError::Conflict(_))
    ));

    let updated = manager
        .update_plan(UpdatePlanRequest {
            plan_id: plan.id,
            expected_version: plan.version,
            builder_key: None,
            context: PlanningContext::default(),
            actor: "tester".into(),
        })
        .await
        .unwrap();
    assert_eq!(updated.version, 2);
    let cancelled = manager
        .cancel_plan(updated.id, updated.version, "tester")
        .await
        .unwrap();
    assert_eq!(cancelled.status, PlanStatus::Cancelled);
    let resumed = manager
        .resume_plan(cancelled.id, cancelled.version, "tester")
        .await
        .unwrap();
    assert_eq!(resumed.status, PlanStatus::Ready);
    assert_eq!(resumed.version, 4);
    let restored = manager
        .restore_plan(manual.id, resumed.version, "tester")
        .await
        .unwrap();
    assert_eq!(restored.status, PlanStatus::Ready);
    assert_eq!(restored.version, 5);
    assert!(manager.list_snapshots(plan.id).await.unwrap().len() >= 4);

    drop(manager);
    let cold = PlanningManager::new(Arc::new(SqlitePlanningStore::new(&database).unwrap()));
    let loaded = cold.find_plan(plan.id).await.unwrap().unwrap();
    assert_eq!(loaded.version, 5);
    assert_eq!(loaded.tasks.len(), 3);
}

#[tokio::test]
async fn catalog_compare_and_swap_rejects_a_stale_plan_writer() {
    let store = Arc::new(SqlitePlanningStore::new(":memory:").unwrap());
    let manager = PlanningManager::new(store.clone());
    let goal = manager
        .create_goal(CreateGoalRequest::new("并发", "拒绝丢失更新"))
        .await
        .unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    let first_snapshot = PlanSnapshot::capture(&plan, "first-writer").unwrap();
    let second_snapshot = PlanSnapshot::capture(&plan, "second-writer").unwrap();
    let mut first = plan.clone();
    first.version = 2;
    first.updated_at = Utc::now();
    first.metadata.insert("writer".into(), serde_json::json!(1));
    store
        .save_plan(&first, Some(&first_snapshot), "writer-1")
        .await
        .unwrap();
    let mut stale = plan;
    stale.version = 2;
    stale.updated_at = Utc::now();
    stale.metadata.insert("writer".into(), serde_json::json!(2));
    assert!(matches!(
        store
            .save_plan(&stale, Some(&second_snapshot), "writer-2")
            .await,
        Err(PlanError::Conflict(_))
    ));
    assert_eq!(store.find_plan(first.id).await.unwrap().unwrap(), first);
}

#[tokio::test]
async fn in_memory_catalog_uses_the_same_compare_and_swap_contract() {
    let store = Arc::new(core_agent_plan::InMemoryPlanningCatalog::default());
    let manager = PlanningManager::new(store.clone());
    let goal = manager
        .create_goal(CreateGoalRequest::new("memory cas", "reject stale writer"))
        .await
        .unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    let first_snapshot = PlanSnapshot::capture(&plan, "first").unwrap();
    let stale_snapshot = PlanSnapshot::capture(&plan, "stale").unwrap();
    let mut first = plan.clone();
    first.version = 2;
    first.updated_at = Utc::now();
    first.metadata.insert("writer".into(), serde_json::json!(1));
    store
        .save_plan(&first, Some(&first_snapshot), "writer-1")
        .await
        .unwrap();
    let mut stale = plan;
    stale.version = 2;
    stale.updated_at = Utc::now();
    stale.metadata.insert("writer".into(), serde_json::json!(2));
    assert!(matches!(
        store
            .save_plan(&stale, Some(&stale_snapshot), "writer-2")
            .await,
        Err(PlanError::Conflict(_))
    ));
}

struct ChangesRequiredReviewer;

#[async_trait]
impl PlanReviewer for ChangesRequiredReviewer {
    fn key(&self) -> &str {
        "changes-required"
    }

    async fn review(&self, _plan: &Plan) -> core_agent_plan::PlanResult<PlanReview> {
        Ok(PlanReview {
            decision: ReviewDecision::ChangesRequired,
            findings: vec!["需要人工确认范围".into()],
            reviewer_key: self.key().into(),
            reviewed_at: Utc::now(),
        })
    }
}

#[tokio::test]
async fn unapproved_review_never_marks_plan_ready() {
    let manager = PlanningManager::builder()
        .reviewer(Arc::new(ChangesRequiredReviewer))
        .build();
    let goal = manager
        .create_goal(CreateGoalRequest::new("review", "review first"))
        .await
        .unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    assert_eq!(plan.status, PlanStatus::Reviewing);
    assert_eq!(
        plan.review.unwrap().decision,
        ReviewDecision::ChangesRequired
    );
}

struct DanglingBuilder;

#[async_trait]
impl PlanBuilder for DanglingBuilder {
    fn key(&self) -> &str {
        "dangling"
    }

    async fn build(
        &self,
        _goal: &core_agent_plan::Goal,
        _context: &PlanningContext,
    ) -> core_agent_plan::PlanResult<PlanDraft> {
        Ok(PlanDraft {
            tasks: vec![TaskDraft {
                key: "only".into(),
                name: "Only".into(),
                priority: 0,
                depends_on: vec!["missing".into()],
                steps: vec![StepDraft {
                    key: "only-step".into(),
                    name: "Only step".into(),
                    depends_on: Vec::new(),
                    max_attempts: 1,
                    action: ActionDraft {
                        kind: ActionKind::Produce,
                        tool_key: None,
                        capability: None,
                        target_uri: None,
                        parameters: serde_json::json!({}),
                    },
                    metadata: BTreeMap::new(),
                }],
                metadata: BTreeMap::new(),
            }],
            metadata: BTreeMap::new(),
        })
    }
}

struct UnlistedToolBuilder;

#[async_trait]
impl PlanBuilder for UnlistedToolBuilder {
    fn key(&self) -> &str {
        "unlisted-tool"
    }

    async fn build(
        &self,
        _goal: &core_agent_plan::Goal,
        _context: &PlanningContext,
    ) -> core_agent_plan::PlanResult<PlanDraft> {
        Ok(PlanDraft {
            tasks: vec![TaskDraft {
                key: "execute".into(),
                name: "Execute".into(),
                priority: 0,
                depends_on: Vec::new(),
                steps: vec![StepDraft {
                    key: "execute".into(),
                    name: "Execute".into(),
                    depends_on: Vec::new(),
                    max_attempts: 1,
                    action: ActionDraft {
                        kind: ActionKind::InvokeTool,
                        tool_key: Some("unknown/tool@1".into()),
                        capability: Some("filesystem.write".into()),
                        target_uri: Some("file:///workspace/output".into()),
                        parameters: serde_json::json!({}),
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
async fn builder_cannot_plan_a_tool_outside_the_context() {
    let manager = PlanningManager::builder()
        .builder(Arc::new(UnlistedToolBuilder))
        .build();
    let goal = manager
        .create_goal(CreateGoalRequest::new(
            "tool boundary",
            "only declared tools",
        ))
        .await
        .unwrap();
    let mut request = CreatePlanRequest::new(goal.id, PlanningContext::default());
    request.builder_key = Some("unlisted-tool".into());
    assert!(matches!(
        manager.create_plan(request).await,
        Err(PlanError::Validation(_))
    ));
    assert!(manager.list_plans(goal.id).await.unwrap().is_empty());
}

struct RejectGeneratedPlan;

impl PlanningPolicy for RejectGeneratedPlan {
    fn evaluate(
        &self,
        operation: PlanningOperation,
        _goal: Option<&core_agent_plan::Goal>,
        plan: Option<&Plan>,
    ) -> core_agent_plan::PlanResult<()> {
        if operation == PlanningOperation::CreatePlan && plan.is_some() {
            return Err(PlanError::PolicyDenied("generated plan denied".into()));
        }
        Ok(())
    }
}

#[tokio::test]
async fn policy_evaluates_the_generated_plan_before_persistence() {
    let manager = PlanningManager::builder()
        .policy(Arc::new(RejectGeneratedPlan))
        .build();
    let goal = manager
        .create_goal(CreateGoalRequest::new("policy", "inspect generated plan"))
        .await
        .unwrap();
    assert!(matches!(
        manager
            .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
            .await,
        Err(PlanError::PolicyDenied(_))
    ));
    assert!(manager.list_plans(goal.id).await.unwrap().is_empty());
}

struct RedirectGoalInterceptor {
    target: uuid::Uuid,
}

impl PlanningInterceptor for RedirectGoalInterceptor {
    fn before_goal_update(
        &self,
        request: &mut UpdateGoalRequest,
    ) -> core_agent_plan::PlanResult<()> {
        request.goal_id = self.target;
        Ok(())
    }
}

#[tokio::test]
async fn goal_interceptor_cannot_redirect_update_identity() {
    let catalog = Arc::new(core_agent_plan::InMemoryPlanningCatalog::default());
    let setup = PlanningManager::new(catalog.clone());
    let first = setup
        .create_goal(CreateGoalRequest::new("first", "first"))
        .await
        .unwrap();
    let second = setup
        .create_goal(CreateGoalRequest::new("second", "second"))
        .await
        .unwrap();
    let manager = PlanningManager::builder()
        .catalog(catalog)
        .interceptor(Arc::new(RedirectGoalInterceptor { target: second.id }))
        .build();
    let request = UpdateGoalRequest {
        goal_id: first.id,
        expected_version: first.version,
        title: "changed".into(),
        description: "changed".into(),
        priority: 0,
        constraints: Vec::new(),
        status: core_agent_plan::GoalStatus::Active,
        metadata: BTreeMap::new(),
        actor: "tester".into(),
    };
    assert!(matches!(
        manager.update_goal(request).await,
        Err(PlanError::Validation(_))
    ));
    assert_eq!(manager.find_goal(first.id).await.unwrap().unwrap(), first);
    assert_eq!(manager.find_goal(second.id).await.unwrap().unwrap(), second);
}

#[tokio::test]
async fn bound_goal_rejects_missing_workspace_and_session_context() {
    let manager = PlanningManager::builder().build();
    let mut request = CreateGoalRequest::new("bound", "identity must match");
    request.workspace_id = Some(uuid::Uuid::new_v4());
    request.session_id = Some(uuid::Uuid::new_v4());
    let goal = manager.create_goal(request).await.unwrap();
    assert!(matches!(
        manager
            .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
            .await,
        Err(PlanError::Validation(_))
    ));
}

#[derive(Default)]
struct RecordingObserver {
    observations: Mutex<Vec<PlanningObservation>>,
}

impl PlanningObserver for RecordingObserver {
    fn on_observation(&self, observation: &PlanningObservation) {
        self.observations.lock().unwrap().push(observation.clone());
    }
}

#[tokio::test]
async fn invalid_builder_output_is_not_persisted_and_failure_is_observed() {
    let observer = Arc::new(RecordingObserver::default());
    let manager = PlanningManager::builder()
        .builder(Arc::new(DanglingBuilder))
        .observer(observer.clone())
        .build();
    let goal = manager
        .create_goal(CreateGoalRequest::new("invalid", "invalid dependency"))
        .await
        .unwrap();
    let mut request = CreatePlanRequest::new(goal.id, PlanningContext::default());
    request.builder_key = Some("dangling".into());
    assert!(matches!(
        manager.create_plan(request).await,
        Err(PlanError::Validation(_))
    ));
    assert!(manager.list_plans(goal.id).await.unwrap().is_empty());
    let observations = observer.observations.lock().unwrap();
    assert!(observations
        .iter()
        .any(|item| !item.success && item.goal_id == Some(goal.id)));
}

#[tokio::test]
async fn sqlite_detects_tampered_child_rows_on_cold_read() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("corrupt.db");
    let store = Arc::new(SqlitePlanningStore::new(&database).unwrap());
    let manager = PlanningManager::new(store);
    let goal = manager
        .create_goal(CreateGoalRequest::new("strict", "strict restore"))
        .await
        .unwrap();
    let plan = manager
        .create_plan(CreatePlanRequest::new(goal.id, PlanningContext::default()))
        .await
        .unwrap();
    rusqlite::Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE step SET name = 'tampered' WHERE plan_id = ?1",
            [plan.id.to_string()],
        )
        .unwrap();
    assert!(manager.find_plan(plan.id).await.is_err());
}
