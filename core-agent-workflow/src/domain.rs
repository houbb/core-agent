use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{WorkflowError, WorkflowResult};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 256 * 1024;
const MAX_ACTIONS: usize = 1024;

pub type WorkflowMetadata = BTreeMap<String, Value>;
pub type WorkflowVariables = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowState {
    Created,
    Scheduled,
    Running,
    Waiting,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Archived,
}
string_enum!(WorkflowState {
    Created => "CREATED",
    Scheduled => "SCHEDULED",
    Running => "RUNNING",
    Waiting => "WAITING",
    Paused => "PAUSED",
    Completed => "COMPLETED",
    Failed => "FAILED",
    Cancelled => "CANCELLED",
    Archived => "ARCHIVED",
});

impl WorkflowState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Archived
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkItemState {
    Pending,
    Prepared,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}
string_enum!(WorkItemState {
    Pending => "PENDING",
    Prepared => "PREPARED",
    Running => "RUNNING",
    Waiting => "WAITING",
    Completed => "COMPLETED",
    Failed => "FAILED",
    Cancelled => "CANCELLED",
});

pub(crate) fn aggregate_state(values: impl Iterator<Item = WorkItemState>) -> WorkItemState {
    let values = values.collect::<Vec<_>>();
    if values.contains(&WorkItemState::Failed) {
        WorkItemState::Failed
    } else if values.contains(&WorkItemState::Cancelled) {
        WorkItemState::Cancelled
    } else if values
        .iter()
        .all(|value| *value == WorkItemState::Completed)
    {
        WorkItemState::Completed
    } else if values.contains(&WorkItemState::Waiting) {
        WorkItemState::Waiting
    } else if values
        .iter()
        .any(|value| matches!(value, WorkItemState::Prepared | WorkItemState::Running))
    {
        WorkItemState::Running
    } else {
        WorkItemState::Pending
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowPolicyDefinition {
    pub action_timeout_ms: u64,
    pub allow_pause: bool,
    pub allow_resume: bool,
    pub allow_cancel: bool,
}

impl Default for WorkflowPolicyDefinition {
    fn default() -> Self {
        Self {
            action_timeout_ms: 300_000,
            allow_pause: true,
            allow_resume: true,
            allow_cancel: true,
        }
    }
}

impl WorkflowPolicyDefinition {
    pub fn validate(&self) -> WorkflowResult<()> {
        if self.action_timeout_ms == 0 || self.action_timeout_ms > 86_400_000 {
            return Err(WorkflowError::Validation(
                "workflow action timeout must be within 1ms..=24h".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowAction {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub input: Value,
    pub timeout_ms: Option<u64>,
    pub metadata: WorkflowMetadata,
}

impl WorkflowAction {
    pub fn new(key: impl Into<String>, name: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            kind: kind.into(),
            input: Value::Null,
            timeout_ms: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow action key", &self.key)?;
        validate_text("workflow action name", &self.name, 256)?;
        validate_key("workflow action kind", &self.kind)?;
        validate_json("workflow action input", &self.input, MAX_JSON_BYTES)?;
        validate_metadata(&self.metadata)?;
        if self
            .timeout_ms
            .is_some_and(|value| value == 0 || value > 86_400_000)
        {
            return Err(WorkflowError::Validation(
                "workflow action timeout must be within 1ms..=24h".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowActivity {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub actions: Vec<WorkflowAction>,
    pub metadata: WorkflowMetadata,
}

impl WorkflowActivity {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        actions: Vec<WorkflowAction>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            actions,
            metadata: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow activity key", &self.key)?;
        validate_text("workflow activity name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        if self.actions.is_empty() {
            return Err(WorkflowError::Validation(
                "workflow activity must contain at least one Action".into(),
            ));
        }
        for action in &self.actions {
            action.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStageDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub activities: Vec<WorkflowActivity>,
    pub metadata: WorkflowMetadata,
}

impl WorkflowStageDefinition {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        activities: Vec<WorkflowActivity>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            activities,
            metadata: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow stage key", &self.key)?;
        validate_text("workflow stage name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        if self.activities.is_empty() {
            return Err(WorkflowError::Validation(
                "workflow Stage must contain at least one Activity".into(),
            ));
        }
        for activity in &self.activities {
            activity.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub version: u64,
    pub stages: Vec<WorkflowStageDefinition>,
    pub policy: WorkflowPolicyDefinition,
    pub metadata: WorkflowMetadata,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl WorkflowDefinition {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        stages: Vec<WorkflowStageDefinition>,
        actor: impl Into<String>,
    ) -> WorkflowResult<Self> {
        let value = Self {
            id: Uuid::new_v4(),
            workflow_id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            description: String::new(),
            version: 1,
            stages,
            policy: WorkflowPolicyDefinition::default(),
            metadata: BTreeMap::new(),
            actor: actor.into(),
            created_at: Utc::now(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn revise(
        &self,
        stages: Vec<WorkflowStageDefinition>,
        actor: impl Into<String>,
    ) -> WorkflowResult<Self> {
        let value = Self {
            id: Uuid::new_v4(),
            workflow_id: self.workflow_id,
            key: self.key.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            version: self
                .version
                .checked_add(1)
                .ok_or_else(|| WorkflowError::Validation("definition version exhausted".into()))?,
            stages,
            policy: self.policy.clone(),
            metadata: self.metadata.clone(),
            actor: actor.into(),
            created_at: Utc::now(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow key", &self.key)?;
        validate_text("workflow name", &self.name, 256)?;
        if self.description.len() > 4096 || self.description.chars().any(char::is_control) {
            return Err(WorkflowError::Validation(
                "workflow description exceeds 4096 safe bytes".into(),
            ));
        }
        validate_actor(&self.actor)?;
        validate_metadata(&self.metadata)?;
        self.policy.validate()?;
        if self.version == 0 || self.stages.is_empty() {
            return Err(WorkflowError::Validation(
                "workflow definition needs a positive version and at least one Stage".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut stage_keys = BTreeSet::new();
        let mut activity_keys = BTreeSet::new();
        let mut action_keys = BTreeSet::new();
        let mut action_count = 0usize;
        for stage in &self.stages {
            stage.validate()?;
            if !ids.insert(stage.id) || !stage_keys.insert(stage.key.clone()) {
                return Err(WorkflowError::Validation(
                    "workflow contains duplicate Stage identity or key".into(),
                ));
            }
            for activity in &stage.activities {
                if !ids.insert(activity.id) || !activity_keys.insert(activity.key.clone()) {
                    return Err(WorkflowError::Validation(
                        "workflow contains duplicate Activity identity or key".into(),
                    ));
                }
                for action in &activity.actions {
                    action_count += 1;
                    if !ids.insert(action.id) || !action_keys.insert(action.key.clone()) {
                        return Err(WorkflowError::Validation(
                            "workflow contains duplicate Action identity or key".into(),
                        ));
                    }
                }
            }
        }
        if action_count > MAX_ACTIONS {
            return Err(WorkflowError::Validation(format!(
                "workflow exceeds {MAX_ACTIONS} Actions"
            )));
        }
        validate_size(self, "workflow definition")
    }

    pub fn action(&self, id: Uuid) -> Option<&WorkflowAction> {
        self.stages
            .iter()
            .flat_map(|stage| &stage.activities)
            .flat_map(|activity| &activity.actions)
            .find(|action| action.id == id)
    }

    pub fn action_count(&self) -> usize {
        self.stages
            .iter()
            .flat_map(|stage| &stage.activities)
            .map(|activity| activity.actions.len())
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowIdentity {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub current_definition_id: Uuid,
    pub current_definition_version: u64,
    pub enabled: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowIdentity {
    pub fn from_definition(value: &WorkflowDefinition) -> Self {
        Self {
            id: value.workflow_id,
            key: value.key.clone(),
            name: value.name.clone(),
            current_definition_id: value.id,
            current_definition_version: value.version,
            enabled: true,
            version: 1,
            actor: value.actor.clone(),
            created_at: value.created_at,
            updated_at: value.created_at,
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow key", &self.key)?;
        validate_text("workflow name", &self.name, 256)?;
        validate_actor(&self.actor)?;
        if self.current_definition_version == 0
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(WorkflowError::Validation(
                "workflow identity version or timestamps are invalid".into(),
            ));
        }
        validate_size(self, "workflow identity")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBinding {
    pub dispatch_id: Uuid,
    pub external_id: Uuid,
    pub external_kind: String,
    pub prepared_at: DateTime<Utc>,
}

impl WorkflowBinding {
    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow binding kind", &self.external_kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowActionResult {
    pub summary: String,
    pub external_state: String,
    pub completed_at: DateTime<Utc>,
}

impl WorkflowActionResult {
    pub fn validate(&self) -> WorkflowResult<()> {
        validate_text("workflow action result", &self.summary, 1024)?;
        validate_key("workflow external state", &self.external_state)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionProgress {
    pub action_id: Uuid,
    pub dispatch_id: Uuid,
    pub state: WorkItemState,
    pub attempts: u32,
    pub binding: Option<WorkflowBinding>,
    pub result: Option<WorkflowActionResult>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl ActionProgress {
    pub fn validate(&self) -> WorkflowResult<()> {
        if self.attempts > 100
            || self.error.as_ref().is_some_and(|value| value.len() > 4096)
            || matches!(
                self.state,
                WorkItemState::Prepared
                    | WorkItemState::Running
                    | WorkItemState::Waiting
                    | WorkItemState::Completed
            ) && self.binding.is_none()
            || self.state == WorkItemState::Pending
                && (self.attempts != 0 || self.binding.is_some())
            || self.state == WorkItemState::Completed
                && (self.result.is_none() || self.completed_at.is_none())
            || matches!(self.state, WorkItemState::Failed | WorkItemState::Cancelled)
                && self.error.is_none()
        {
            return Err(WorkflowError::Validation(
                "workflow Action progress is inconsistent".into(),
            ));
        }
        if let Some(binding) = &self.binding {
            binding.validate()?;
            if binding.dispatch_id != self.dispatch_id {
                return Err(WorkflowError::Validation(
                    "workflow binding does not own the Action dispatch".into(),
                ));
            }
        }
        if let Some(result) = &self.result {
            result.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityProgress {
    pub activity_id: Uuid,
    pub state: WorkItemState,
    pub actions: Vec<ActionProgress>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageProgress {
    pub stage_id: Uuid,
    pub state: WorkItemState,
    pub activities: Vec<ActivityProgress>,
}

#[derive(Debug, Clone)]
pub struct StartWorkflowRequest {
    pub workflow_key: String,
    pub definition_version: Option<u64>,
    pub variables: WorkflowVariables,
    pub actor: String,
}

impl StartWorkflowRequest {
    pub fn new(workflow_key: impl Into<String>, actor: impl Into<String>) -> Self {
        Self {
            workflow_key: workflow_key.into(),
            definition_version: None,
            variables: BTreeMap::new(),
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_key("workflow key", &self.workflow_key)?;
        validate_actor(&self.actor)?;
        validate_variables(&self.variables)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowInstance {
    pub id: Uuid,
    pub workflow_id: Uuid,
    pub definition_id: Uuid,
    pub definition_version: u64,
    pub definition: WorkflowDefinition,
    pub variables: WorkflowVariables,
    pub state: WorkflowState,
    pub progress: Vec<StageProgress>,
    pub version: u64,
    pub actor: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl WorkflowInstance {
    pub fn new(
        definition: WorkflowDefinition,
        variables: WorkflowVariables,
        actor: impl Into<String>,
    ) -> WorkflowResult<Self> {
        definition.validate()?;
        validate_variables(&variables)?;
        let id = Uuid::new_v4();
        let now = Utc::now();
        let progress = definition
            .stages
            .iter()
            .map(|stage| StageProgress {
                stage_id: stage.id,
                state: WorkItemState::Pending,
                activities: stage
                    .activities
                    .iter()
                    .map(|activity| ActivityProgress {
                        activity_id: activity.id,
                        state: WorkItemState::Pending,
                        actions: activity
                            .actions
                            .iter()
                            .map(|action| ActionProgress {
                                action_id: action.id,
                                dispatch_id: Uuid::new_v5(&id, action.id.as_bytes()),
                                state: WorkItemState::Pending,
                                attempts: 0,
                                binding: None,
                                result: None,
                                error: None,
                                started_at: None,
                                completed_at: None,
                                updated_at: now,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
        let value = Self {
            id,
            workflow_id: definition.workflow_id,
            definition_id: definition.id,
            definition_version: definition.version,
            definition,
            variables,
            state: WorkflowState::Created,
            progress,
            version: 1,
            actor: actor.into(),
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        self.definition.validate()?;
        validate_variables(&self.variables)?;
        validate_actor(&self.actor)?;
        if self.workflow_id != self.definition.workflow_id
            || self.definition_id != self.definition.id
            || self.definition_version != self.definition.version
            || self.version == 0
            || self.updated_at < self.created_at
            || self.progress.len() != self.definition.stages.len()
        {
            return Err(WorkflowError::Validation(
                "workflow Instance identity, version or progress is invalid".into(),
            ));
        }
        let mut active = 0usize;
        for (stage, stage_progress) in self.definition.stages.iter().zip(&self.progress) {
            if stage.id != stage_progress.stage_id
                || stage.activities.len() != stage_progress.activities.len()
            {
                return Err(WorkflowError::Validation(
                    "workflow Stage progress does not match Definition".into(),
                ));
            }
            for (activity, activity_progress) in
                stage.activities.iter().zip(&stage_progress.activities)
            {
                if activity.id != activity_progress.activity_id
                    || activity.actions.len() != activity_progress.actions.len()
                {
                    return Err(WorkflowError::Validation(
                        "workflow Activity progress does not match Definition".into(),
                    ));
                }
                for (action, action_progress) in
                    activity.actions.iter().zip(&activity_progress.actions)
                {
                    if action.id != action_progress.action_id {
                        return Err(WorkflowError::Validation(
                            "workflow Action progress does not match Definition".into(),
                        ));
                    }
                    action_progress.validate()?;
                    if matches!(
                        action_progress.state,
                        WorkItemState::Prepared | WorkItemState::Running | WorkItemState::Waiting
                    ) {
                        active += 1;
                    }
                }
                if activity_progress.state
                    != aggregate_state(activity_progress.actions.iter().map(|value| value.state))
                {
                    return Err(WorkflowError::Validation(
                        "workflow Activity aggregate progress is inconsistent".into(),
                    ));
                }
            }
            if stage_progress.state
                != aggregate_state(stage_progress.activities.iter().map(|value| value.state))
            {
                return Err(WorkflowError::Validation(
                    "workflow Stage aggregate progress is inconsistent".into(),
                ));
            }
        }
        if active > 1
            || self.state == WorkflowState::Completed
                && self
                    .action_progress()
                    .any(|value| value.state != WorkItemState::Completed)
            || self.state == WorkflowState::Failed
                && !self
                    .action_progress()
                    .any(|value| value.state == WorkItemState::Failed)
            || self.state == WorkflowState::Waiting
                && !self
                    .action_progress()
                    .any(|value| value.state == WorkItemState::Waiting)
            || self.state == WorkflowState::Running && self.started_at.is_none()
            || self.state.is_terminal() != self.completed_at.is_some()
        {
            return Err(WorkflowError::Validation(
                "workflow Instance lifecycle and Action progress are inconsistent".into(),
            ));
        }
        validate_size(self, "workflow instance")
    }

    pub fn action_progress(&self) -> impl Iterator<Item = &ActionProgress> {
        self.progress
            .iter()
            .flat_map(|stage| &stage.activities)
            .flat_map(|activity| &activity.actions)
    }

    pub fn current_ids(&self) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
        for stage in &self.progress {
            for activity in &stage.activities {
                for action in &activity.actions {
                    if matches!(
                        action.state,
                        WorkItemState::Prepared | WorkItemState::Running | WorkItemState::Waiting
                    ) {
                        return (
                            Some(stage.stage_id),
                            Some(activity.activity_id),
                            Some(action.action_id),
                        );
                    }
                }
            }
        }
        (None, None, None)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSnapshot {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub sequence: u64,
    pub label: String,
    pub content: WorkflowInstance,
    pub hash: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl WorkflowSnapshot {
    pub fn capture(
        instance: &WorkflowInstance,
        label: impl Into<String>,
        actor: impl Into<String>,
    ) -> WorkflowResult<Self> {
        instance.validate()?;
        let value = Self {
            id: Uuid::new_v4(),
            instance_id: instance.id,
            sequence: instance.version,
            label: label.into(),
            content: instance.clone(),
            hash: semantic_hash(instance)?,
            actor: actor.into(),
            created_at: Utc::now(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        validate_text("workflow snapshot label", &self.label, 256)?;
        validate_actor(&self.actor)?;
        self.content.validate()?;
        if self.instance_id != self.content.id
            || self.sequence != self.content.version
            || self.hash != semantic_hash(&self.content)?
        {
            return Err(WorkflowError::Validation(
                "workflow Snapshot identity or hash is invalid".into(),
            ));
        }
        validate_size(self, "workflow snapshot")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStateRecord {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub sequence: u64,
    pub from_state: Option<WorkflowState>,
    pub to_state: WorkflowState,
    pub reason: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl WorkflowStateRecord {
    pub fn validate(&self) -> WorkflowResult<()> {
        validate_text("workflow state reason", &self.reason, 1024)?;
        validate_actor(&self.actor)?;
        if self.sequence == 0 || self.from_state == Some(self.to_state) {
            return Err(WorkflowError::Validation(
                "workflow state sequence or transition is invalid".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_timeline(
    instance: &WorkflowInstance,
    values: &[WorkflowStateRecord],
) -> WorkflowResult<()> {
    let Some(first) = values.first() else {
        return Err(WorkflowError::Validation(
            "workflow Instance has no lifecycle timeline".into(),
        ));
    };
    for value in values {
        value.validate()?;
    }
    if first.instance_id != instance.id
        || first.sequence != 1
        || first.from_state.is_some()
        || first.to_state != WorkflowState::Created
        || values.windows(2).any(|pair| {
            pair[0].instance_id != instance.id
                || pair[1].instance_id != instance.id
                || pair[0].sequence >= pair[1].sequence
                || pair[1].from_state != Some(pair[0].to_state)
        })
        || values.last().is_none_or(|value| {
            value.to_state != instance.state || value.sequence > instance.version
        })
    {
        return Err(WorkflowError::Validation(
            "workflow lifecycle timeline is inconsistent".into(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkflowActionOutcome {
    Completed(WorkflowActionResult),
    Waiting(String),
    Paused(String),
    Failed(String),
    Cancelled(String),
}

pub fn semantic_hash<T: Serialize>(value: &T) -> WorkflowResult<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

pub(crate) fn validate_actor(value: &str) -> WorkflowResult<()> {
    validate_text("workflow actor", value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> WorkflowResult<()> {
    validate_text(label, value, 386)?;
    if value.trim() != value || value.chars().any(char::is_whitespace) {
        return Err(WorkflowError::Validation(format!(
            "{label} must be normalized and contain no whitespace"
        )));
    }
    Ok(())
}

pub(crate) fn validate_variables(value: &WorkflowVariables) -> WorkflowResult<()> {
    validate_json(
        "workflow variables",
        &Value::Object(value.clone().into_iter().collect()),
        MAX_JSON_BYTES,
    )
}

fn validate_metadata(value: &WorkflowMetadata) -> WorkflowResult<()> {
    validate_json(
        "workflow metadata",
        &Value::Object(value.clone().into_iter().collect()),
        MAX_JSON_BYTES,
    )
}

fn validate_json(label: &str, value: &Value, max: usize) -> WorkflowResult<()> {
    if serde_json::to_vec(value)?.len() > max {
        return Err(WorkflowError::Validation(format!(
            "{label} exceeds {max} bytes"
        )));
    }
    reject_sensitive_keys(value, label, 0)
}

fn reject_sensitive_keys(value: &Value, path: &str, depth: usize) -> WorkflowResult<()> {
    if depth > 64 {
        return Err(WorkflowError::Validation(format!(
            "{path} exceeds 64 levels of nesting"
        )));
    }
    match value {
        Value::Object(values) => {
            for (key, child) in values {
                if is_sensitive_key(key) {
                    return Err(WorkflowError::Validation(format!(
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
    .any(|value| normalized == *value || normalized.ends_with(value))
}

fn validate_text(label: &str, value: &str, max: usize) -> WorkflowResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(WorkflowError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> WorkflowResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(WorkflowError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> WorkflowDefinition {
        WorkflowDefinition::new(
            "rca",
            "RCA",
            vec![WorkflowStageDefinition::new(
                "collect",
                "Collect",
                vec![WorkflowActivity::new(
                    "logs",
                    "Collect logs",
                    vec![WorkflowAction::new("read", "Read logs", "execution.plan")],
                )],
            )],
            "designer",
        )
        .unwrap()
    }

    #[test]
    fn four_level_definition_builds_stable_progress() {
        let definition = definition();
        let instance = WorkflowInstance::new(definition, BTreeMap::new(), "operator").unwrap();
        assert_eq!(instance.state, WorkflowState::Created);
        assert_eq!(instance.action_progress().count(), 1);
        assert_eq!(
            instance.action_progress().next().unwrap().state,
            WorkItemState::Pending
        );
    }

    #[test]
    fn duplicate_action_keys_are_rejected() {
        let action = WorkflowAction::new("same", "One", "execution.plan");
        let mut duplicate = action.clone();
        duplicate.id = Uuid::new_v4();
        let result = WorkflowDefinition::new(
            "bad",
            "Bad",
            vec![WorkflowStageDefinition::new(
                "stage",
                "Stage",
                vec![WorkflowActivity::new(
                    "activity",
                    "Activity",
                    vec![action, duplicate],
                )],
            )],
            "designer",
        );
        assert!(matches!(result, Err(WorkflowError::Validation(_))));
    }

    #[test]
    fn nested_secrets_are_rejected() {
        let mut request = StartWorkflowRequest::new("rca", "operator");
        request.variables.insert(
            "config".into(),
            serde_json::json!({"nested": {"access_token": "secret"}}),
        );
        assert!(matches!(
            request.validate(),
            Err(WorkflowError::Validation(_))
        ));
    }
}
