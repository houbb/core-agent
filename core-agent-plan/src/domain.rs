use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::error::{PlanError, PlanResult};

pub type PlanningMetadata = BTreeMap<String, Value>;

const MAX_METADATA_BYTES: usize = 64 * 1024;
const MAX_PARAMETERS_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 128 * 1024;
const MAX_ITEMS: usize = 256;
const MAX_TOTAL_STEPS: usize = 1024;
const MAX_PLAN_BYTES: usize = 8 * 1024 * 1024;

pub fn validate_actor(actor: &str) -> PlanResult<()> {
    validate_text("actor", actor, 128)
}

pub fn validate_metadata(metadata: &PlanningMetadata) -> PlanResult<()> {
    validate_json(
        "metadata",
        &Value::Object(metadata.clone().into_iter().collect()),
        MAX_METADATA_BYTES,
    )
}

fn validate_json(name: &str, value: &Value, max_bytes: usize) -> PlanResult<()> {
    let encoded = serde_json::to_vec(value)?;
    if encoded.len() > max_bytes {
        return Err(PlanError::Validation(format!(
            "{name} exceeds {max_bytes} bytes"
        )));
    }
    reject_sensitive_keys(value, name, 0)
}

fn reject_sensitive_keys(value: &Value, path: &str, depth: usize) -> PlanResult<()> {
    if depth > 64 {
        return Err(PlanError::Validation(format!(
            "{path} exceeds 64 levels of nesting"
        )));
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_key(key) {
                    return Err(PlanError::Validation(format!(
                        "{path}.{key} may contain secret material"
                    )));
                }
                reject_sensitive_keys(child, &format!("{path}.{key}"), depth + 1)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                reject_sensitive_keys(child, &format!("{path}[{index}]"), depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    [
        "apikey",
        "accesstoken",
        "refreshtoken",
        "authtoken",
        "token",
        "password",
        "passwd",
        "authorization",
        "privatekey",
        "clientsecret",
        "credential",
        "secret",
    ]
    .iter()
    .any(|sensitive| normalized == *sensitive || normalized.ends_with(sensitive))
}

fn validate_text(name: &str, value: &str, max: usize) -> PlanResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > max || trimmed.chars().any(char::is_control) {
        return Err(PlanError::Validation(format!(
            "{name} must contain 1..={max} printable characters"
        )));
    }
    Ok(())
}

fn validate_uri(name: &str, value: &str) -> PlanResult<()> {
    validate_text(name, value, 2048)?;
    let uri = Url::parse(value)
        .map_err(|error| PlanError::Validation(format!("invalid {name}: {error}")))?;
    if uri.scheme().is_empty() || !uri.username().is_empty() || uri.password().is_some() {
        return Err(PlanError::Validation(format!(
            "{name} must have a scheme and must not contain credentials"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub metadata: PlanningMetadata,
    pub created_at: DateTime<Utc>,
}

impl Intent {
    pub fn new(
        kind: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> PlanResult<Self> {
        let intent = Self {
            id: Uuid::new_v4(),
            kind: kind.into(),
            title: title.into(),
            description: description.into(),
            metadata: BTreeMap::new(),
            created_at: Utc::now(),
        };
        intent.validate()?;
        Ok(intent)
    }

    pub fn validate(&self) -> PlanResult<()> {
        validate_text("intent kind", &self.kind, 64)?;
        validate_text("intent title", &self.title, 256)?;
        if self.description.len() > 4096 {
            return Err(PlanError::Validation(
                "intent description exceeds 4096 bytes".into(),
            ));
        }
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoalStatus {
    Proposed,
    Active,
    Satisfied,
    Cancelled,
}

impl GoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "PROPOSED",
            Self::Active => "ACTIVE",
            Self::Satisfied => "SATISFIED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: Uuid,
    pub intent: Option<Intent>,
    pub title: String,
    pub description: String,
    pub priority: i32,
    pub status: GoalStatus,
    pub constraints: Vec<String>,
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub metadata: PlanningMetadata,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Goal {
    pub fn validate(&self) -> PlanResult<()> {
        validate_text("goal title", &self.title, 256)?;
        if self.description.len() > 16 * 1024 {
            return Err(PlanError::Validation(
                "goal description exceeds 16384 bytes".into(),
            ));
        }
        if self.constraints.len() > 32 {
            return Err(PlanError::Validation(
                "goal has more than 32 constraints".into(),
            ));
        }
        for constraint in &self.constraints {
            validate_text("goal constraint", constraint, 512)?;
        }
        if let Some(intent) = &self.intent {
            intent.validate()?;
        }
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone)]
pub struct CreateGoalRequest {
    pub intent: Option<Intent>,
    pub title: String,
    pub description: String,
    pub priority: i32,
    pub constraints: Vec<String>,
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub metadata: PlanningMetadata,
    pub actor: String,
}

impl CreateGoalRequest {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            intent: None,
            title: title.into(),
            description: description.into(),
            priority: 0,
            constraints: Vec::new(),
            session_id: None,
            workspace_id: None,
            metadata: BTreeMap::new(),
            actor: "system".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdateGoalRequest {
    pub goal_id: Uuid,
    pub expected_version: u64,
    pub title: String,
    pub description: String,
    pub priority: i32,
    pub constraints: Vec<String>,
    pub status: GoalStatus,
    pub metadata: PlanningMetadata,
    pub actor: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStatus {
    Created,
    Planning,
    Reviewing,
    Ready,
    Executing,
    Completed,
    Cancelled,
    Failed,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Planning => "PLANNING",
            Self::Reviewing => "REVIEWING",
            Self::Ready => "READY",
            Self::Executing => "EXECUTING",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
            Self::Failed => "FAILED",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use PlanStatus::*;
        matches!(
            (self, next),
            (Created, Planning | Cancelled)
                | (Planning, Reviewing | Failed | Cancelled)
                | (Reviewing, Ready | Completed | Planning | Failed | Cancelled)
                | (Ready, Planning | Executing | Cancelled)
                | (Executing, Reviewing | Failed | Cancelled)
                | (Failed, Planning | Cancelled)
                | (Cancelled, Created | Planning | Reviewing | Ready)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
    Cancelled,
}

impl WorkStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Skipped => "SKIPPED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionKind {
    Analyze,
    InvokeTool,
    Produce,
    Verify,
}

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Analyze => "ANALYZE",
            Self::InvokeTool => "INVOKE_TOOL",
            Self::Produce => "PRODUCE",
            Self::Verify => "VERIFY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: Uuid,
    pub kind: ActionKind,
    pub tool_key: Option<String>,
    pub capability: Option<String>,
    pub target_uri: Option<String>,
    pub parameters: Value,
}

impl Action {
    pub fn validate(&self) -> PlanResult<()> {
        if let Some(tool_key) = &self.tool_key {
            validate_text("action tool key", tool_key, 386)?;
        }
        if let Some(capability) = &self.capability {
            validate_text("action capability", capability, 64)?;
        }
        if let Some(target) = &self.target_uri {
            validate_uri("action target URI", target)?;
        }
        if self.kind == ActionKind::InvokeTool && self.tool_key.is_none() {
            return Err(PlanError::Validation(
                "tool action requires a tool key".into(),
            ));
        }
        if self.kind != ActionKind::InvokeTool
            && (self.tool_key.is_some() || self.capability.is_some())
        {
            return Err(PlanError::Validation(
                "only tool actions may bind a tool or capability".into(),
            ));
        }
        validate_json("action parameters", &self.parameters, MAX_PARAMETERS_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub task_id: Uuid,
    pub key: String,
    pub name: String,
    pub status: WorkStatus,
    pub dependencies: Vec<Uuid>,
    pub max_attempts: u32,
    pub action: Action,
    pub metadata: PlanningMetadata,
}

impl Step {
    pub fn validate(&self) -> PlanResult<()> {
        validate_text("step key", &self.key, 128)?;
        validate_text("step name", &self.name, 256)?;
        if self.max_attempts == 0 || self.max_attempts > 100 {
            return Err(PlanError::Validation(
                "step max_attempts must be 1..=100".into(),
            ));
        }
        self.action.validate()?;
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub key: String,
    pub name: String,
    pub status: WorkStatus,
    pub priority: i32,
    pub dependencies: Vec<Uuid>,
    pub steps: BTreeMap<Uuid, Step>,
    pub metadata: PlanningMetadata,
}

impl Task {
    pub fn validate(&self) -> PlanResult<()> {
        validate_text("task key", &self.key, 128)?;
        validate_text("task name", &self.name, 256)?;
        if self.steps.is_empty() || self.steps.len() > MAX_ITEMS {
            return Err(PlanError::Validation(
                "task must contain 1..=256 steps".into(),
            ));
        }
        for (id, step) in &self.steps {
            if *id != step.id || step.task_id != self.id || step.plan_id != self.plan_id {
                return Err(PlanError::Validation("task/step identity mismatch".into()));
            }
            step.validate()?;
        }
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewDecision {
    Approved,
    ChangesRequired,
    Rejected,
}

impl ReviewDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::ChangesRequired => "CHANGES_REQUIRED",
            Self::Rejected => "REJECTED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanReview {
    pub decision: ReviewDecision,
    pub findings: Vec<String>,
    pub reviewer_key: String,
    pub reviewed_at: DateTime<Utc>,
}

impl PlanReview {
    pub fn validate(&self) -> PlanResult<()> {
        validate_text("reviewer key", &self.reviewer_key, 128)?;
        if self.findings.len() > 64 {
            return Err(PlanError::Validation(
                "review has more than 64 findings".into(),
            ));
        }
        for finding in &self.findings {
            validate_text("review finding", finding, 1024)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanningNodeKind {
    Intent,
    Goal,
    Plan,
    Task,
    Step,
    Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanningRelation {
    Contains,
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningNode {
    pub id: Uuid,
    pub kind: PlanningNodeKind,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningEdge {
    pub source: Uuid,
    pub target: Uuid,
    pub relation: PlanningRelation,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanningGraph {
    pub nodes: Vec<PlanningNode>,
    pub edges: Vec<PlanningEdge>,
}

impl PlanningGraph {
    pub fn validate(&self) -> PlanResult<()> {
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            validate_text("planning node label", &node.label, 256)?;
            if !ids.insert(node.id) {
                return Err(PlanError::Validation(
                    "planning graph has duplicate nodes".into(),
                ));
            }
        }
        if self
            .edges
            .iter()
            .any(|edge| !ids.contains(&edge.source) || !ids.contains(&edge.target))
        {
            return Err(PlanError::Validation(
                "planning graph edge is dangling".into(),
            ));
        }
        let dependencies = self
            .edges
            .iter()
            .filter(|edge| edge.relation == PlanningRelation::DependsOn)
            .fold(BTreeMap::<Uuid, Vec<Uuid>>::new(), |mut map, edge| {
                map.entry(edge.source).or_default().push(edge.target);
                map
            });
        fn visit(
            id: Uuid,
            dependencies: &BTreeMap<Uuid, Vec<Uuid>>,
            visiting: &mut BTreeSet<Uuid>,
            visited: &mut BTreeSet<Uuid>,
        ) -> bool {
            if visited.contains(&id) {
                return true;
            }
            if !visiting.insert(id) {
                return false;
            }
            if dependencies
                .get(&id)
                .into_iter()
                .flatten()
                .any(|next| !visit(*next, dependencies, visiting, visited))
            {
                return false;
            }
            visiting.remove(&id);
            visited.insert(id);
            true
        }
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        if dependencies
            .keys()
            .any(|id| !visit(*id, &dependencies, &mut visiting, &mut visited))
        {
            return Err(PlanError::Validation(
                "planning dependency graph contains a cycle".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub id: Uuid,
    pub goal_id: Uuid,
    pub strategy_key: String,
    pub status: PlanStatus,
    pub tasks: BTreeMap<Uuid, Task>,
    pub graph: PlanningGraph,
    pub review: Option<PlanReview>,
    pub metadata: PlanningMetadata,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Plan {
    pub fn validate(&self) -> PlanResult<()> {
        validate_text("plan strategy key", &self.strategy_key, 128)?;
        if self.tasks.is_empty() || self.tasks.len() > MAX_ITEMS {
            return Err(PlanError::Validation(
                "plan must contain 1..=256 tasks".into(),
            ));
        }
        let mut step_ids = BTreeSet::new();
        let total_steps = self
            .tasks
            .values()
            .map(|task| task.steps.len())
            .sum::<usize>();
        if total_steps > MAX_TOTAL_STEPS {
            return Err(PlanError::Validation(
                "plan has more than 1024 total steps".into(),
            ));
        }
        let task_ids = self.tasks.keys().copied().collect::<BTreeSet<_>>();
        for (id, task) in &self.tasks {
            if *id != task.id || task.plan_id != self.id {
                return Err(PlanError::Validation("plan/task identity mismatch".into()));
            }
            if task
                .dependencies
                .iter()
                .any(|dependency| !task_ids.contains(dependency))
            {
                return Err(PlanError::Validation(
                    "task dependency is outside its plan".into(),
                ));
            }
            task.validate()?;
            step_ids.extend(task.steps.keys().copied());
        }
        for task in self.tasks.values() {
            for step in task.steps.values() {
                if step
                    .dependencies
                    .iter()
                    .any(|dependency| !step_ids.contains(dependency))
                {
                    return Err(PlanError::Validation(
                        "step dependency is outside its plan".into(),
                    ));
                }
            }
        }
        if self.status == PlanStatus::Ready
            && self.review.as_ref().map(|review| review.decision) != Some(ReviewDecision::Approved)
        {
            return Err(PlanError::Validation(
                "ready plan requires an approved review".into(),
            ));
        }
        if self.status == PlanStatus::Completed
            && (self.review.as_ref().map(|review| review.decision)
                != Some(ReviewDecision::Approved)
                || self.tasks.values().any(|task| {
                    !matches!(task.status, WorkStatus::Completed | WorkStatus::Skipped)
                        || task.steps.values().any(|step| {
                            !matches!(step.status, WorkStatus::Completed | WorkStatus::Skipped)
                        })
                }))
        {
            return Err(PlanError::Validation(
                "completed plan requires approved review and completed work".into(),
            ));
        }
        if let Some(review) = &self.review {
            review.validate()?;
        }
        validate_metadata(&self.metadata)?;
        self.graph.validate()?;
        self.validate_graph_membership()?;
        if serde_json::to_vec(self)?.len() > MAX_PLAN_BYTES {
            return Err(PlanError::Validation(
                "serialized plan exceeds 8 MiB".into(),
            ));
        }
        Ok(())
    }

    fn validate_graph_membership(&self) -> PlanResult<()> {
        let mut expected = BTreeMap::from([
            (self.goal_id, PlanningNodeKind::Goal),
            (self.id, PlanningNodeKind::Plan),
        ]);
        for task in self.tasks.values() {
            expected.insert(task.id, PlanningNodeKind::Task);
            for step in task.steps.values() {
                expected.insert(step.id, PlanningNodeKind::Step);
                expected.insert(step.action.id, PlanningNodeKind::Action);
            }
        }
        let actual = self
            .graph
            .nodes
            .iter()
            .filter(|node| node.kind != PlanningNodeKind::Intent)
            .map(|node| (node.id, node.kind))
            .collect::<BTreeMap<_, _>>();
        if actual != expected {
            return Err(PlanError::Validation(
                "planning graph does not match plan hierarchy".into(),
            ));
        }
        let intent_ids = self
            .graph
            .nodes
            .iter()
            .filter(|node| node.kind == PlanningNodeKind::Intent)
            .map(|node| node.id)
            .collect::<Vec<_>>();
        if intent_ids.len() > 1 {
            return Err(PlanError::Validation(
                "planning graph has more than one embedded intent".into(),
            ));
        }
        let mut expected_edges =
            BTreeSet::from([(self.goal_id, self.id, PlanningRelation::Contains)]);
        if let Some(intent_id) = intent_ids.first() {
            expected_edges.insert((*intent_id, self.goal_id, PlanningRelation::Contains));
        }
        for task in self.tasks.values() {
            expected_edges.insert((self.id, task.id, PlanningRelation::Contains));
            expected_edges.extend(
                task.dependencies
                    .iter()
                    .map(|dependency| (task.id, *dependency, PlanningRelation::DependsOn)),
            );
            for step in task.steps.values() {
                expected_edges.insert((task.id, step.id, PlanningRelation::Contains));
                expected_edges.insert((step.id, step.action.id, PlanningRelation::Contains));
                expected_edges.extend(
                    step.dependencies
                        .iter()
                        .map(|dependency| (step.id, *dependency, PlanningRelation::DependsOn)),
                );
            }
        }
        let actual_edges = self
            .graph
            .edges
            .iter()
            .map(|edge| (edge.source, edge.target, edge.relation))
            .collect::<BTreeSet<_>>();
        if actual_edges.len() != self.graph.edges.len() || actual_edges != expected_edges {
            return Err(PlanError::Validation(
                "planning graph edges do not match plan hierarchy and dependencies".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub plan_version: u64,
    pub label: String,
    pub content: Plan,
    pub hash: String,
    pub created_at: DateTime<Utc>,
}

impl PlanSnapshot {
    pub fn capture(plan: &Plan, label: impl Into<String>) -> PlanResult<Self> {
        let mut snapshot = Self {
            id: Uuid::new_v4(),
            plan_id: plan.id,
            plan_version: plan.version,
            label: label.into(),
            content: plan.clone(),
            hash: String::new(),
            created_at: Utc::now(),
        };
        snapshot.hash = snapshot.semantic_hash()?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn semantic_hash(&self) -> PlanResult<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&self.content)?)
        ))
    }

    pub fn validate(&self) -> PlanResult<()> {
        validate_text("snapshot label", &self.label, 256)?;
        if self.content.id != self.plan_id || self.content.version != self.plan_version {
            return Err(PlanError::Validation(
                "snapshot identity/version mismatch".into(),
            ));
        }
        self.content.validate()?;
        if self.semantic_hash()? != self.hash {
            return Err(PlanError::Validation("snapshot hash mismatch".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanningRequestKind {
    Coding,
    RootCauseAnalysis,
    Report,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningWorkspaceRef {
    pub id: Uuid,
    pub name: String,
    pub uri: String,
    pub state: String,
    pub project_count: usize,
    pub resource_count: usize,
    pub graph_node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolReference {
    pub key: String,
    pub name: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanningContext {
    pub request_kind: PlanningRequestKind,
    pub session_id: Option<Uuid>,
    pub context_id: Option<Uuid>,
    pub workspace: Option<PlanningWorkspaceRef>,
    pub tools: Vec<ToolReference>,
    pub facts: PlanningMetadata,
}

impl Default for PlanningContext {
    fn default() -> Self {
        Self {
            request_kind: PlanningRequestKind::General,
            session_id: None,
            context_id: None,
            workspace: None,
            tools: Vec::new(),
            facts: BTreeMap::new(),
        }
    }
}

impl PlanningContext {
    pub fn validate(&self) -> PlanResult<()> {
        if self.tools.len() > MAX_ITEMS {
            return Err(PlanError::Validation(
                "planning context has more than 256 tools".into(),
            ));
        }
        let mut keys = BTreeSet::new();
        for tool in &self.tools {
            validate_text("tool reference key", &tool.key, 386)?;
            validate_text("tool reference name", &tool.name, 256)?;
            if !keys.insert(&tool.key) {
                return Err(PlanError::Validation(
                    "planning context has duplicate tool keys".into(),
                ));
            }
            if tool.capabilities.len() > 64 {
                return Err(PlanError::Validation(
                    "tool reference has too many capabilities".into(),
                ));
            }
            for capability in &tool.capabilities {
                validate_text("tool capability", capability, 64)?;
            }
        }
        if let Some(workspace) = &self.workspace {
            validate_text("workspace reference name", &workspace.name, 256)?;
            validate_uri("workspace reference URI", &workspace.uri)?;
            validate_text("workspace reference state", &workspace.state, 64)?;
        }
        validate_json(
            "planning context",
            &serde_json::to_value(self)?,
            MAX_CONTEXT_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDraft {
    pub kind: ActionKind,
    pub tool_key: Option<String>,
    pub capability: Option<String>,
    pub target_uri: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepDraft {
    pub key: String,
    pub name: String,
    pub depends_on: Vec<String>,
    pub max_attempts: u32,
    pub action: ActionDraft,
    pub metadata: PlanningMetadata,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskDraft {
    pub key: String,
    pub name: String,
    pub priority: i32,
    pub depends_on: Vec<String>,
    pub steps: Vec<StepDraft>,
    pub metadata: PlanningMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanDraft {
    pub tasks: Vec<TaskDraft>,
    pub metadata: PlanningMetadata,
}

#[derive(Debug, Clone)]
pub struct CreatePlanRequest {
    pub goal_id: Uuid,
    pub builder_key: Option<String>,
    pub context: PlanningContext,
    pub actor: String,
}

impl CreatePlanRequest {
    pub fn new(goal_id: Uuid, context: PlanningContext) -> Self {
        Self {
            goal_id,
            builder_key: None,
            context,
            actor: "system".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UpdatePlanRequest {
    pub plan_id: Uuid,
    pub expected_version: u64,
    pub builder_key: Option<String>,
    pub context: PlanningContext,
    pub actor: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_supports_pre_and_post_execution_review() {
        assert!(PlanStatus::Planning.can_transition_to(PlanStatus::Reviewing));
        assert!(PlanStatus::Reviewing.can_transition_to(PlanStatus::Ready));
        assert!(PlanStatus::Executing.can_transition_to(PlanStatus::Reviewing));
        assert!(PlanStatus::Reviewing.can_transition_to(PlanStatus::Completed));
        assert!(!PlanStatus::Created.can_transition_to(PlanStatus::Executing));
    }

    #[test]
    fn nested_secret_parameters_are_rejected() {
        let action = Action {
            id: Uuid::new_v4(),
            kind: ActionKind::Produce,
            tool_key: None,
            capability: None,
            target_uri: None,
            parameters: serde_json::json!({"nested": {"api-token": "hidden"}}),
        };
        assert!(action.validate().is_err());
    }

    #[test]
    fn token_counts_are_allowed_but_auth_tokens_are_rejected() {
        assert!(validate_metadata(&BTreeMap::from([(
            "token_count".into(),
            serde_json::json!(42)
        )]))
        .is_ok());
        assert!(validate_metadata(&BTreeMap::from([(
            "auth_token".into(),
            serde_json::json!("hidden")
        )]))
        .is_err());
    }

    #[test]
    fn action_uri_rejects_embedded_credentials() {
        let action = Action {
            id: Uuid::new_v4(),
            kind: ActionKind::Produce,
            tool_key: None,
            capability: None,
            target_uri: Some("https://user:secret@example.com/output".into()),
            parameters: serde_json::json!({}),
        };
        assert!(action.validate().is_err());
    }

    #[test]
    fn graph_rejects_dependency_cycles() {
        let one = Uuid::new_v4();
        let two = Uuid::new_v4();
        let graph = PlanningGraph {
            nodes: vec![
                PlanningNode {
                    id: one,
                    kind: PlanningNodeKind::Task,
                    label: "one".into(),
                },
                PlanningNode {
                    id: two,
                    kind: PlanningNodeKind::Task,
                    label: "two".into(),
                },
            ],
            edges: vec![
                PlanningEdge {
                    source: one,
                    target: two,
                    relation: PlanningRelation::DependsOn,
                },
                PlanningEdge {
                    source: two,
                    target: one,
                    relation: PlanningRelation::DependsOn,
                },
            ],
        };
        assert!(graph.validate().is_err());
    }
}
