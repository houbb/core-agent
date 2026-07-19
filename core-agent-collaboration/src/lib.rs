use std::collections::{BTreeMap, BTreeSet};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type CollaborationPlatformResult<T> = Result<T, CollaborationPlatformError>;

#[derive(Debug, thiserror::Error)]
pub enum CollaborationPlatformError {
    #[error("collaboration validation failed: {0}")]
    Validation(String),
    #[error("collaboration resource not found: {0}")]
    NotFound(String),
    #[error("collaboration state conflict: {0}")]
    Conflict(String),
    #[error("collaboration permission denied: {0}")]
    Denied(String),
    #[error("collaboration internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectRole {
    Owner,
    Maintainer,
    Reviewer,
    Member,
    Viewer,
}

impl ProjectRole {
    fn can_write(self) -> bool {
        self != Self::Viewer
    }
    fn can_review(self) -> bool {
        matches!(self, Self::Owner | Self::Maintainer | Self::Reviewer)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectState {
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamProject {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub state: ProjectState,
    pub members: BTreeMap<String, ProjectRole>,
    pub agent_ids: BTreeSet<Uuid>,
    pub workflow_ids: BTreeSet<Uuid>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TeamProject {
    pub fn new(key: impl Into<String>, name: impl Into<String>, owner: impl Into<String>) -> Self {
        let now = Utc::now();
        let owner = owner.into();
        let mut members = BTreeMap::new();
        members.insert(owner.clone(), ProjectRole::Owner);
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            state: ProjectState::Active,
            members,
            agent_ids: BTreeSet::new(),
            workflow_ids: BTreeSet::new(),
            version: 1,
            actor: owner,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> CollaborationPlatformResult<()> {
        validate_key("project key", &self.key)?;
        validate_text("project name", &self.name, 256)?;
        validate_actor(&self.actor)?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.members.is_empty()
            || self.members.len() > 512
            || !self
                .members
                .values()
                .any(|role| *role == ProjectRole::Owner)
            || self.agent_ids.len() > 512
            || self.workflow_ids.len() > 512
        {
            return Err(CollaborationPlatformError::Validation(
                "project bounds are invalid".into(),
            ));
        }
        for member in self.members.keys() {
            validate_actor(member)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    Open,
    Running,
    Paused,
    Review,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: Uuid,
    pub project_id: Uuid,
    pub number: u64,
    pub title: String,
    pub state: TaskState,
    pub owner_agent_id: Option<Uuid>,
    pub assignee: String,
    pub reviewer: Option<String>,
    pub progress: u8,
    pub created_by: String,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TeamTask {
    fn validate(&self) -> CollaborationPlatformResult<()> {
        validate_text("task title", &self.title, 512)?;
        validate_actor(&self.assignee)?;
        validate_actor(&self.created_by)?;
        validate_actor(&self.actor)?;
        if let Some(reviewer) = &self.reviewer {
            validate_actor(reviewer)?;
        }
        if self.number == 0
            || self.version == 0
            || self.updated_at < self.created_at
            || self.progress > 100
        {
            return Err(CollaborationPlatformError::Validation(
                "task bounds are invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewState {
    Pending,
    Approved,
    ChangesRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub id: Uuid,
    pub reviewer: String,
    pub decision: ReviewDecision,
    pub comment: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamReview {
    pub id: Uuid,
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub state: ReviewState,
    pub risk: String,
    pub summary: String,
    pub approvals: Vec<ApprovalRecord>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TeamReview {
    fn validate(&self) -> CollaborationPlatformResult<()> {
        validate_key("review risk", &self.risk)?;
        validate_text("review summary", &self.summary, 4096)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at || self.approvals.len() > 64 {
            return Err(CollaborationPlatformError::Validation(
                "review bounds are invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeState {
    Draft,
    Published,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeAsset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub title: String,
    pub kind: String,
    pub summary: String,
    pub state: KnowledgeState,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl KnowledgeAsset {
    fn validate(&self) -> CollaborationPlatformResult<()> {
        validate_key("knowledge key", &self.key)?;
        validate_key("knowledge kind", &self.kind)?;
        validate_text("knowledge title", &self.title, 256)?;
        validate_text("knowledge summary", &self.summary, 4096)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(CollaborationPlatformError::Validation(
                "knowledge bounds are invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub event_key: String,
    pub kind: String,
    pub subject: String,
    pub summary: String,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub audience: BTreeSet<String>,
    pub occurred_at: DateTime<Utc>,
}

impl ActivityRecord {
    fn validate(&self) -> CollaborationPlatformResult<()> {
        validate_key("activity event key", &self.event_key)?;
        validate_key("activity kind", &self.kind)?;
        validate_actor(&self.subject)?;
        validate_text("activity summary", &self.summary, 1024)?;
        validate_key("activity entity type", &self.entity_type)?;
        if self.audience.len() > 512 {
            return Err(CollaborationPlatformError::Validation(
                "activity audience is too large".into(),
            ));
        }
        for actor in &self.audience {
            validate_actor(actor)?;
        }
        Ok(())
    }
}

#[derive(Default, Clone)]
struct State {
    projects: BTreeMap<Uuid, TeamProject>,
    tasks: BTreeMap<Uuid, TeamTask>,
    reviews: BTreeMap<Uuid, TeamReview>,
    knowledge: BTreeMap<Uuid, KnowledgeAsset>,
    activities: BTreeMap<String, ActivityRecord>,
}

#[derive(Default)]
pub struct CollaborationPlatformManager {
    state: RwLock<State>,
}

impl CollaborationPlatformManager {
    pub fn create_project(&self, project: TeamProject) -> CollaborationPlatformResult<TeamProject> {
        project.validate()?;
        let mut state = self.write()?;
        if state
            .projects
            .values()
            .any(|value| value.key == project.key)
        {
            return Err(CollaborationPlatformError::Conflict(
                "project key already exists".into(),
            ));
        }
        state.projects.insert(project.id, project.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("project.created:{}", project.id),
                "project.created",
                &project.actor,
                format!("{} created project {}", project.actor, project.name),
                "project",
                project.id,
            ),
        )?;
        Ok(project)
    }

    pub fn add_member(
        &self,
        project_id: Uuid,
        member: &str,
        role: ProjectRole,
        actor: &str,
    ) -> CollaborationPlatformResult<TeamProject> {
        validate_actor(member)?;
        validate_actor(actor)?;
        let mut state = self.write()?;
        let mut project = required_project(&state, project_id)?.clone();
        require_role(&project, actor, |role| {
            matches!(role, ProjectRole::Owner | ProjectRole::Maintainer)
        })?;
        project.members.insert(member.into(), role);
        advance_project(&mut project, actor);
        project.validate()?;
        state.projects.insert(project_id, project.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("project.member:{project_id}:{member}:{}", project.version),
                "project.member_added",
                actor,
                format!("{actor} added {member} to {}", project.name),
                "project",
                project_id,
            ),
        )?;
        Ok(project)
    }

    pub fn create_task(
        &self,
        project_id: Uuid,
        title: &str,
        assignee: &str,
        owner_agent_id: Option<Uuid>,
        actor: &str,
    ) -> CollaborationPlatformResult<TeamTask> {
        validate_text("task title", title, 512)?;
        validate_actor(assignee)?;
        validate_actor(actor)?;
        let mut state = self.write()?;
        let project = required_project(&state, project_id)?.clone();
        require_role(&project, actor, ProjectRole::can_write)?;
        if !project.members.contains_key(assignee) {
            return Err(CollaborationPlatformError::Denied(
                "task assignee is not a project member".into(),
            ));
        }
        let number = state
            .tasks
            .values()
            .filter(|task| task.project_id == project_id)
            .map(|task| task.number)
            .max()
            .unwrap_or(0)
            + 1;
        let now = Utc::now();
        let task = TeamTask {
            id: Uuid::new_v4(),
            project_id,
            number,
            title: title.into(),
            state: TaskState::Open,
            owner_agent_id,
            assignee: assignee.into(),
            reviewer: None,
            progress: 0,
            created_by: actor.into(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        task.validate()?;
        state.tasks.insert(task.id, task.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("task.created:{}", task.id),
                "task.created",
                actor,
                format!("{actor} created Task #{number}: {title}"),
                "task",
                task.id,
            ),
        )?;
        Ok(task)
    }

    pub fn update_task(
        &self,
        task_id: Uuid,
        state_value: TaskState,
        progress: u8,
        assignee: Option<&str>,
        actor: &str,
    ) -> CollaborationPlatformResult<TeamTask> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let mut task = required_task(&state, task_id)?.clone();
        let project = required_project(&state, task.project_id)?.clone();
        require_role(&project, actor, ProjectRole::can_write)?;
        if !valid_task_transition(task.state, state_value) {
            return Err(CollaborationPlatformError::Conflict(
                "invalid task state transition".into(),
            ));
        }
        if let Some(assignee) = assignee {
            validate_actor(assignee)?;
            if !project.members.contains_key(assignee) {
                return Err(CollaborationPlatformError::Denied(
                    "task assignee is not a member".into(),
                ));
            }
            task.assignee = assignee.into();
        }
        task.state = state_value;
        task.progress = progress;
        advance_task(&mut task, actor);
        task.validate()?;
        state.tasks.insert(task_id, task.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("task.updated:{task_id}:{}", task.version),
                "task.updated",
                actor,
                format!(
                    "{actor} moved Task #{} to {:?} ({}%)",
                    task.number, task.state, task.progress
                ),
                "task",
                task_id,
            ),
        )?;
        Ok(task)
    }

    pub fn request_review(
        &self,
        task_id: Uuid,
        reviewer: &str,
        risk: &str,
        summary: &str,
        actor: &str,
    ) -> CollaborationPlatformResult<TeamReview> {
        validate_actor(reviewer)?;
        validate_actor(actor)?;
        validate_key("review risk", risk)?;
        validate_text("review summary", summary, 4096)?;
        let mut state = self.write()?;
        let mut task = required_task(&state, task_id)?.clone();
        let project = required_project(&state, task.project_id)?.clone();
        require_role(&project, actor, ProjectRole::can_write)?;
        require_role(&project, reviewer, ProjectRole::can_review)?;
        if !matches!(task.state, TaskState::Running | TaskState::Paused) {
            return Err(CollaborationPlatformError::Conflict(
                "task is not ready for review".into(),
            ));
        }
        if state
            .reviews
            .values()
            .any(|review| review.task_id == task_id && review.state == ReviewState::Pending)
        {
            return Err(CollaborationPlatformError::Conflict(
                "task already has a pending review".into(),
            ));
        }
        task.state = TaskState::Review;
        task.reviewer = Some(reviewer.into());
        advance_task(&mut task, actor);
        state.tasks.insert(task_id, task.clone());
        let now = Utc::now();
        let review = TeamReview {
            id: Uuid::new_v4(),
            project_id: project.id,
            task_id,
            state: ReviewState::Pending,
            risk: risk.into(),
            summary: summary.into(),
            approvals: Vec::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        review.validate()?;
        state.reviews.insert(review.id, review.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("review.requested:{}", review.id),
                "review.requested",
                actor,
                format!(
                    "{actor} requested review from {reviewer} for Task #{}",
                    task.number
                ),
                "review",
                review.id,
            ),
        )?;
        Ok(review)
    }

    pub fn decide_review(
        &self,
        review_id: Uuid,
        decision: ReviewDecision,
        comment: &str,
        actor: &str,
    ) -> CollaborationPlatformResult<TeamReview> {
        validate_actor(actor)?;
        validate_text("approval comment", comment, 2048)?;
        let mut state = self.write()?;
        let mut review = required_review(&state, review_id)?.clone();
        let mut task = required_task(&state, review.task_id)?.clone();
        let project = required_project(&state, review.project_id)?.clone();
        require_role(&project, actor, ProjectRole::can_review)?;
        if review.state != ReviewState::Pending {
            return Err(CollaborationPlatformError::Conflict(
                "review is already decided".into(),
            ));
        }
        if task.created_by == actor {
            return Err(CollaborationPlatformError::Denied(
                "task creator cannot approve their own review".into(),
            ));
        }
        review.approvals.push(ApprovalRecord {
            id: Uuid::new_v4(),
            reviewer: actor.into(),
            decision,
            comment: comment.into(),
            decided_at: Utc::now(),
        });
        review.state = if decision == ReviewDecision::Approve {
            ReviewState::Approved
        } else {
            ReviewState::ChangesRequested
        };
        review.version += 1;
        review.actor = actor.into();
        review.updated_at = Utc::now().max(review.updated_at);
        review.validate()?;
        task.state = if decision == ReviewDecision::Approve {
            task.progress = 100;
            TaskState::Completed
        } else {
            TaskState::Running
        };
        advance_task(&mut task, actor);
        state.reviews.insert(review_id, review.clone());
        state.tasks.insert(task.id, task.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("review.decided:{review_id}:{}", review.version),
                "review.decided",
                actor,
                format!("{actor} {:?} review for Task #{}", decision, task.number),
                "review",
                review_id,
            ),
        )?;
        Ok(review)
    }

    pub fn add_knowledge(
        &self,
        project_id: Uuid,
        key: &str,
        title: &str,
        kind: &str,
        summary: &str,
        actor: &str,
    ) -> CollaborationPlatformResult<KnowledgeAsset> {
        let mut state = self.write()?;
        let project = required_project(&state, project_id)?.clone();
        require_role(&project, actor, ProjectRole::can_write)?;
        if state
            .knowledge
            .values()
            .any(|value| value.project_id == project_id && value.key == key)
        {
            return Err(CollaborationPlatformError::Conflict(
                "knowledge key already exists".into(),
            ));
        }
        let now = Utc::now();
        let value = KnowledgeAsset {
            id: Uuid::new_v4(),
            project_id,
            key: key.into(),
            title: title.into(),
            kind: kind.into(),
            summary: summary.into(),
            state: KnowledgeState::Draft,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        state.knowledge.insert(value.id, value.clone());
        append_activity(
            &mut state,
            activity(
                &project,
                format!("knowledge.created:{}", value.id),
                "knowledge.created",
                actor,
                format!("{actor} created knowledge {}", value.title),
                "knowledge",
                value.id,
            ),
        )?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_external_activity(
        &self,
        project_id: Uuid,
        event_key: &str,
        kind: &str,
        subject: &str,
        summary: &str,
        entity_type: &str,
        entity_id: Uuid,
    ) -> CollaborationPlatformResult<ActivityRecord> {
        let mut state = self.write()?;
        let project = required_project(&state, project_id)?.clone();
        require_role(&project, subject, ProjectRole::can_write)?;
        let record = activity(
            &project,
            event_key.into(),
            kind,
            subject,
            summary.into(),
            entity_type,
            entity_id,
        );
        append_activity(&mut state, record.clone())?;
        Ok(record)
    }

    pub fn projects(&self) -> CollaborationPlatformResult<Vec<TeamProject>> {
        Ok(self.read()?.projects.values().cloned().collect())
    }
    pub fn tasks(&self, project_id: Uuid) -> CollaborationPlatformResult<Vec<TeamTask>> {
        let state = self.read()?;
        required_project(&state, project_id)?;
        Ok(state
            .tasks
            .values()
            .filter(|value| value.project_id == project_id)
            .cloned()
            .collect())
    }
    pub fn reviews(&self, project_id: Uuid) -> CollaborationPlatformResult<Vec<TeamReview>> {
        let state = self.read()?;
        required_project(&state, project_id)?;
        Ok(state
            .reviews
            .values()
            .filter(|value| value.project_id == project_id)
            .cloned()
            .collect())
    }
    pub fn knowledge(&self, project_id: Uuid) -> CollaborationPlatformResult<Vec<KnowledgeAsset>> {
        let state = self.read()?;
        required_project(&state, project_id)?;
        Ok(state
            .knowledge
            .values()
            .filter(|value| value.project_id == project_id)
            .cloned()
            .collect())
    }
    pub fn activities(&self, project_id: Uuid) -> CollaborationPlatformResult<Vec<ActivityRecord>> {
        let state = self.read()?;
        required_project(&state, project_id)?;
        let mut values = state
            .activities
            .values()
            .filter(|value| value.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.occurred_at), value.id));
        Ok(values)
    }
    pub fn notifications(
        &self,
        project_id: Uuid,
        actor: &str,
    ) -> CollaborationPlatformResult<Vec<ActivityRecord>> {
        let state = self.read()?;
        require_role(required_project(&state, project_id)?, actor, |_| true)?;
        drop(state);
        Ok(self
            .activities(project_id)?
            .into_iter()
            .filter(|value| value.audience.is_empty() || value.audience.contains(actor))
            .collect())
    }

    fn read(&self) -> CollaborationPlatformResult<RwLockReadGuard<'_, State>> {
        self.state
            .read()
            .map_err(|_| CollaborationPlatformError::Internal("collaboration lock poisoned".into()))
    }
    fn write(&self) -> CollaborationPlatformResult<RwLockWriteGuard<'_, State>> {
        self.state
            .write()
            .map_err(|_| CollaborationPlatformError::Internal("collaboration lock poisoned".into()))
    }
}

fn activity(
    project: &TeamProject,
    event_key: String,
    kind: &str,
    subject: &str,
    summary: String,
    entity_type: &str,
    entity_id: Uuid,
) -> ActivityRecord {
    ActivityRecord {
        id: Uuid::new_v4(),
        project_id: project.id,
        event_key,
        kind: kind.into(),
        subject: subject.into(),
        summary,
        entity_type: entity_type.into(),
        entity_id,
        audience: project.members.keys().cloned().collect(),
        occurred_at: Utc::now(),
    }
}
fn append_activity(state: &mut State, record: ActivityRecord) -> CollaborationPlatformResult<()> {
    record.validate()?;
    if state.activities.contains_key(&record.event_key) {
        return Err(CollaborationPlatformError::Conflict(
            "activity event already recorded".into(),
        ));
    }
    state.activities.insert(record.event_key.clone(), record);
    Ok(())
}
fn required_project(state: &State, id: Uuid) -> CollaborationPlatformResult<&TeamProject> {
    state
        .projects
        .get(&id)
        .ok_or_else(|| CollaborationPlatformError::NotFound(id.to_string()))
}
fn required_task(state: &State, id: Uuid) -> CollaborationPlatformResult<&TeamTask> {
    state
        .tasks
        .get(&id)
        .ok_or_else(|| CollaborationPlatformError::NotFound(id.to_string()))
}
fn required_review(state: &State, id: Uuid) -> CollaborationPlatformResult<&TeamReview> {
    state
        .reviews
        .get(&id)
        .ok_or_else(|| CollaborationPlatformError::NotFound(id.to_string()))
}
fn require_role(
    project: &TeamProject,
    actor: &str,
    allowed: impl FnOnce(ProjectRole) -> bool,
) -> CollaborationPlatformResult<()> {
    let role = project.members.get(actor).copied().ok_or_else(|| {
        CollaborationPlatformError::Denied("actor is not a project member".into())
    })?;
    if !allowed(role) {
        return Err(CollaborationPlatformError::Denied(
            "project role cannot perform this operation".into(),
        ));
    }
    Ok(())
}
fn valid_task_transition(from: TaskState, to: TaskState) -> bool {
    from == to
        || matches!(
            (from, to),
            (TaskState::Open, TaskState::Running)
                | (
                    TaskState::Running,
                    TaskState::Paused
                        | TaskState::Review
                        | TaskState::Completed
                        | TaskState::Failed
                )
                | (
                    TaskState::Paused,
                    TaskState::Running | TaskState::Review | TaskState::Failed
                )
                | (TaskState::Review, TaskState::Running | TaskState::Completed)
        )
}
fn advance_project(value: &mut TeamProject, actor: &str) {
    value.version += 1;
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at)
}
fn advance_task(value: &mut TeamTask, actor: &str) {
    value.version += 1;
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at)
}
fn validate_actor(value: &str) -> CollaborationPlatformResult<()> {
    validate_key("actor", value)
}
fn validate_key(label: &str, value: &str) -> CollaborationPlatformResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(CollaborationPlatformError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}
fn validate_text(label: &str, value: &str, max: usize) -> CollaborationPlatformResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(CollaborationPlatformError::Validation(format!(
            "{label} is invalid"
        )));
    }
    Ok(())
}
