use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use core_agent_execution::ExecutionStatus;
use core_agent_plan::{CreateGoalRequest, PlanningContext};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AgentError, AgentResult};

const MAX_TEXT: usize = 4096;
const MAX_ITEMS: usize = 256;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

pub type AgentMetadata = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentCapability(String);

impl AgentCapability {
    pub fn new(value: impl Into<String>) -> AgentResult<Self> {
        let value = value.into();
        validate_key("agent capability", &value)?;
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentOperation {
    Create,
    Start,
    Run,
    Stop,
    Finish,
    Destroy,
    Snapshot,
    Restore,
    Reconcile,
}

impl AgentOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Create => "CREATE",
            Self::Start => "START",
            Self::Run => "RUN",
            Self::Stop => "STOP",
            Self::Finish => "FINISH",
            Self::Destroy => "DESTROY",
            Self::Snapshot => "SNAPSHOT",
            Self::Restore => "RESTORE",
            Self::Reconcile => "RECONCILE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentPolicyDecision {
    Allow,
    Ask,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPolicyDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub default_decision: AgentPolicyDecision,
    pub rules: BTreeMap<AgentOperation, AgentPolicyDecision>,
    pub metadata: AgentMetadata,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentPolicyDefinition {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            description: String::new(),
            default_decision: AgentPolicyDecision::Allow,
            rules: BTreeMap::new(),
            metadata: BTreeMap::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn decision(&self, operation: AgentOperation) -> AgentPolicyDecision {
        self.rules
            .get(&operation)
            .copied()
            .unwrap_or(self.default_decision)
    }

    pub fn validate(&self) -> AgentResult<()> {
        validate_key("policy key", &self.key)?;
        validate_text("policy name", &self.name, 256)?;
        validate_optional_text("policy description", &self.description, MAX_TEXT)?;
        validate_metadata(&self.metadata)?;
        validate_version_times(self.version, self.created_at, self.updated_at)?;
        validate_size(self, "policy")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub model_key: Option<String>,
    pub planner_key: Option<String>,
    pub workspace_key: Option<String>,
    pub memory_key: Option<String>,
    pub policy_id: Option<Uuid>,
    pub capabilities: BTreeSet<AgentCapability>,
    pub toolset: BTreeSet<String>,
    pub config: Value,
    pub metadata: AgentMetadata,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentProfile {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            description: String::new(),
            model_key: None,
            planner_key: None,
            workspace_key: None,
            memory_key: None,
            policy_id: None,
            capabilities: BTreeSet::new(),
            toolset: BTreeSet::new(),
            config: Value::Object(Default::default()),
            metadata: BTreeMap::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> AgentResult<()> {
        validate_key("profile key", &self.key)?;
        validate_text("profile name", &self.name, 256)?;
        validate_optional_text("profile description", &self.description, MAX_TEXT)?;
        for (label, value) in [
            ("model key", self.model_key.as_deref()),
            ("planner key", self.planner_key.as_deref()),
            ("workspace key", self.workspace_key.as_deref()),
            ("memory key", self.memory_key.as_deref()),
        ] {
            if let Some(value) = value {
                validate_key(label, value)?;
            }
        }
        if self.capabilities.len() > MAX_ITEMS || self.toolset.len() > MAX_ITEMS {
            return Err(AgentError::Validation(
                "profile capability or tool count exceeds 256".into(),
            ));
        }
        for capability in &self.capabilities {
            validate_key("agent capability", capability.as_str())?;
        }
        for tool in &self.toolset {
            validate_key("profile tool", tool)?;
        }
        reject_sensitive_keys(&self.config, "profile config", 0)?;
        validate_metadata(&self.metadata)?;
        validate_version_times(self.version, self.created_at, self.updated_at)?;
        validate_size(self, "profile")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentState {
    Created,
    Ready,
    Running,
    Waiting,
    Paused,
    Completed,
    Failed,
    Destroyed,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Waiting => "WAITING",
            Self::Paused => "PAUSED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Destroyed => "DESTROYED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CREATED" => Some(Self::Created),
            "READY" => Some(Self::Ready),
            "RUNNING" => Some(Self::Running),
            "WAITING" => Some(Self::Waiting),
            "PAUSED" => Some(Self::Paused),
            "COMPLETED" => Some(Self::Completed),
            "FAILED" => Some(Self::Failed),
            "DESTROYED" => Some(Self::Destroyed),
            _ => None,
        }
    }

    pub fn is_snapshot_safe(self) -> bool {
        matches!(
            self,
            Self::Ready | Self::Waiting | Self::Paused | Self::Completed | Self::Failed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub name: String,
    pub profile: AgentProfile,
    pub policy: Option<AgentPolicyDefinition>,
    pub state: AgentState,
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub current_goal_id: Option<Uuid>,
    pub current_plan_id: Option<Uuid>,
    pub current_execution_id: Option<Uuid>,
    pub completed_goals: u64,
    pub failed_goals: u64,
    pub last_error_kind: Option<String>,
    pub last_error_message: Option<String>,
    pub metadata: AgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    pub fn new(profile: AgentProfile, request: CreateAgentRequest) -> AgentResult<Self> {
        profile.validate()?;
        validate_actor(&request.actor)?;
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            name: request.name,
            profile,
            policy: request.policy,
            state: AgentState::Created,
            session_id: request.session_id,
            workspace_id: request.workspace_id,
            current_goal_id: None,
            current_plan_id: None,
            current_execution_id: None,
            completed_goals: 0,
            failed_goals: 0,
            last_error_kind: None,
            last_error_message: None,
            metadata: request.metadata,
            version: 1,
            actor: request.actor,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> AgentResult<()> {
        validate_text("agent name", &self.name, 256)?;
        self.profile.validate()?;
        if let Some(policy) = &self.policy {
            policy.validate()?;
            if self.profile.policy_id != Some(policy.id) {
                return Err(AgentError::Validation(
                    "agent Profile and Policy snapshots do not match".into(),
                ));
            }
        } else if self.profile.policy_id.is_some() {
            return Err(AgentError::Validation(
                "agent is missing its declared Policy snapshot".into(),
            ));
        }
        if self.current_execution_id.is_some() && self.current_plan_id.is_none()
            || self.current_plan_id.is_some() && self.current_goal_id.is_none()
            || self.updated_at < self.created_at
            || self.state == AgentState::Destroyed && self.current_execution_id.is_some()
        {
            return Err(AgentError::Validation(
                "agent runtime references or lifecycle fields are inconsistent".into(),
            ));
        }
        validate_optional_key("last error kind", self.last_error_kind.as_deref())?;
        if let Some(message) = &self.last_error_message {
            validate_optional_text("last error message", message, 1024)?;
        }
        validate_actor(&self.actor)?;
        validate_metadata(&self.metadata)?;
        validate_version_times(self.version, self.created_at, self.updated_at)?;
        validate_size(self, "agent")
    }
}

#[derive(Debug, Clone)]
pub struct CreateAgentRequest {
    pub name: String,
    pub profile_id: Uuid,
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub policy: Option<AgentPolicyDefinition>,
    pub metadata: AgentMetadata,
    pub actor: String,
}

impl CreateAgentRequest {
    pub fn new(name: impl Into<String>, profile_id: Uuid) -> Self {
        Self {
            name: name.into(),
            profile_id,
            session_id: None,
            workspace_id: None,
            policy: None,
            metadata: BTreeMap::new(),
            actor: "system".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentGoalRequest {
    pub goal: CreateGoalRequest,
    pub context: PlanningContext,
}

impl AgentGoalRequest {
    pub fn new(goal: CreateGoalRequest, context: PlanningContext) -> Self {
        Self { goal, context }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunReference {
    pub goal_id: Uuid,
    pub plan_id: Uuid,
    pub execution_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunOutcome {
    pub agent: Agent,
    pub reference: AgentRunReference,
    pub execution_status: ExecutionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStateRecord {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub sequence: u64,
    pub from_state: Option<AgentState>,
    pub to_state: AgentState,
    pub goal_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub reason: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_version: u64,
    pub state: AgentState,
    pub label: String,
    pub hash: String,
    pub content: Agent,
    pub created_at: DateTime<Utc>,
}

impl AgentSnapshot {
    pub fn new(agent: &Agent, label: impl Into<String>) -> AgentResult<Self> {
        if !agent.state.is_snapshot_safe() {
            return Err(AgentError::InvalidState(format!(
                "cannot snapshot {} agent",
                agent.state.as_str()
            )));
        }
        let value = Self {
            id: Uuid::new_v4(),
            agent_id: agent.id,
            agent_version: agent.version,
            state: agent.state,
            label: label.into(),
            hash: semantic_hash(agent)?,
            content: agent.clone(),
            created_at: Utc::now(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> AgentResult<()> {
        validate_text("snapshot label", &self.label, 256)?;
        self.content.validate()?;
        if self.agent_id != self.content.id
            || self.agent_version != self.content.version
            || self.state != self.content.state
            || !self.state.is_snapshot_safe()
            || self.hash != semantic_hash(&self.content)?
            || self.created_at < self.content.created_at
        {
            return Err(AgentError::Validation(
                "agent snapshot identity or hash mismatch".into(),
            ));
        }
        validate_size(self, "snapshot")
    }
}

pub(crate) fn validate_actor(value: &str) -> AgentResult<()> {
    validate_text("actor", value, 256)
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> AgentResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(AgentError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: &str, max: usize) -> AgentResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}

fn validate_key(label: &str, value: &str) -> AgentResult<()> {
    validate_text(label, value, 386)?;
    if value.trim() != value || value.contains(char::is_whitespace) {
        return Err(AgentError::Validation(format!("{label} is not normalized")));
    }
    Ok(())
}

fn validate_optional_key(label: &str, value: Option<&str>) -> AgentResult<()> {
    if let Some(value) = value {
        validate_key(label, value)?;
    }
    Ok(())
}

fn validate_metadata(value: &AgentMetadata) -> AgentResult<()> {
    if value.len() > 64 {
        return Err(AgentError::Validation(
            "agent metadata has more than 64 entries".into(),
        ));
    }
    for key in value.keys() {
        validate_key("metadata key", key)?;
    }
    reject_sensitive_keys(
        &Value::Object(value.clone().into_iter().collect()),
        "metadata",
        0,
    )?;
    if serde_json::to_vec(value)?.len() > 64 * 1024 {
        return Err(AgentError::Validation(
            "agent metadata exceeds 64 KiB".into(),
        ));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> AgentResult<()> {
    if depth > 64 {
        return Err(AgentError::Validation(format!(
            "{label} exceeds 64 levels of nesting"
        )));
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let normalized = key
                    .chars()
                    .filter(|value| value.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if [
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
                .any(|needle| normalized == *needle || normalized.ends_with(needle))
                {
                    return Err(AgentError::Validation(format!(
                        "{label} contains sensitive key {key}"
                    )));
                }
                reject_sensitive_keys(child, label, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for child in values {
                reject_sensitive_keys(child, label, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_version_times(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> AgentResult<()> {
    if version == 0 || updated_at < created_at {
        return Err(AgentError::Validation(
            "entity version or timestamps are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> AgentResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(AgentError::Validation(format!(
            "serialized {label} exceeds 1 MiB"
        )));
    }
    Ok(())
}

fn semantic_hash<T: Serialize>(value: &T) -> AgentResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
