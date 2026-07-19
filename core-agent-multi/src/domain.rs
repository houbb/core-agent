use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{MultiAgentError, MultiAgentResult};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 256;

pub type MultiAgentMetadata = BTreeMap<String, Value>;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub metadata: MultiAgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Organization {
    pub fn new(key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            description: String::new(),
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_key("organization key", &self.key)?;
        validate_text("organization name", &self.name, 256)?;
        validate_optional_text("organization description", &self.description, 4096)?;
        validate_metadata(&self.metadata)?;
        validate_entity_version(self.version, self.created_at, self.updated_at, &self.actor)?;
        validate_size(self, "organization")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub required_capabilities: BTreeSet<String>,
    pub metadata: MultiAgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Role {
    pub fn new(
        organization_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            key: key.into(),
            name: name.into(),
            description: String::new(),
            required_capabilities: BTreeSet::new(),
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_key("role key", &self.key)?;
        validate_text("role name", &self.name, 256)?;
        validate_optional_text("role description", &self.description, 4096)?;
        validate_capabilities(&self.required_capabilities)?;
        validate_metadata(&self.metadata)?;
        validate_entity_version(self.version, self.created_at, self.updated_at, &self.actor)?;
        validate_size(self, "role")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamPolicyDefinition {
    pub max_members: u32,
    pub allow_handover: bool,
}

impl Default for TeamPolicyDefinition {
    fn default() -> Self {
        Self {
            max_members: 32,
            allow_handover: true,
        }
    }
}

impl TeamPolicyDefinition {
    pub fn validate(&self) -> MultiAgentResult<()> {
        if self.max_members == 0 || self.max_members > 256 {
            return Err(MultiAgentError::Validation(
                "team max_members must be within 1..=256".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TeamState {
    Created,
    Ready,
    Active,
    Completed,
    Archived,
}
string_enum!(TeamState {
    Created => "CREATED",
    Ready => "READY",
    Active => "ACTIVE",
    Completed => "COMPLETED",
    Archived => "ARCHIVED",
});

impl TeamState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Archived)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub key: String,
    pub name: String,
    pub goal: String,
    pub workspace_id: Option<Uuid>,
    pub memory_scope: Option<String>,
    pub policy: TeamPolicyDefinition,
    pub state: TeamState,
    pub metadata: MultiAgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Team {
    pub fn new(
        organization_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        goal: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            organization_id,
            key: key.into(),
            name: name.into(),
            goal: goal.into(),
            workspace_id: None,
            memory_scope: None,
            policy: TeamPolicyDefinition::default(),
            state: TeamState::Created,
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_key("team key", &self.key)?;
        validate_text("team name", &self.name, 256)?;
        validate_text("team goal", &self.goal, 4096)?;
        if let Some(scope) = &self.memory_scope {
            validate_key("team memory scope", scope)?;
        }
        self.policy.validate()?;
        validate_metadata(&self.metadata)?;
        validate_entity_version(self.version, self.created_at, self.updated_at, &self.actor)?;
        if self.state.is_terminal() != self.completed_at.is_some() {
            return Err(MultiAgentError::Validation(
                "team terminal state and completion time are inconsistent".into(),
            ));
        }
        validate_size(self, "team")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemberState {
    Joined,
    Available,
    Assigned,
    Working,
    Waiting,
    Completed,
    Left,
}
string_enum!(MemberState {
    Joined => "JOINED",
    Available => "AVAILABLE",
    Assigned => "ASSIGNED",
    Working => "WORKING",
    Waiting => "WAITING",
    Completed => "COMPLETED",
    Left => "LEFT",
});

impl MemberState {
    pub fn is_available(self) -> bool {
        matches!(self, Self::Joined | Self::Available | Self::Completed)
    }

    pub fn owns_collaboration(self) -> bool {
        matches!(self, Self::Assigned | Self::Working | Self::Waiting)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMember {
    pub id: Uuid,
    pub team_id: Uuid,
    pub role_id: Uuid,
    pub agent_id: Uuid,
    pub capabilities: BTreeSet<String>,
    pub state: MemberState,
    pub current_collaboration_id: Option<Uuid>,
    pub metadata: MultiAgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentMember {
    pub fn new(
        team_id: Uuid,
        role_id: Uuid,
        agent_id: Uuid,
        capabilities: BTreeSet<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            team_id,
            role_id,
            agent_id,
            capabilities,
            state: MemberState::Joined,
            current_collaboration_id: None,
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_capabilities(&self.capabilities)?;
        validate_metadata(&self.metadata)?;
        validate_entity_version(self.version, self.created_at, self.updated_at, &self.actor)?;
        if self.state.owns_collaboration() != self.current_collaboration_id.is_some() {
            return Err(MultiAgentError::Validation(
                "member state and current Collaboration are inconsistent".into(),
            ));
        }
        validate_size(self, "agent member")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}
string_enum!(MessagePriority {
    Low => "LOW",
    Normal => "NORMAL",
    High => "HIGH",
    Critical => "CRITICAL",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: Uuid,
    pub correlation_id: Uuid,
    pub source_member_id: Option<Uuid>,
    pub target_member_id: Uuid,
    pub intent: String,
    pub payload: Value,
    pub context_references: BTreeSet<String>,
    pub priority: MessagePriority,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl AgentMessage {
    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_key("agent message intent", &self.intent)?;
        validate_json("agent message payload", &self.payload, MAX_JSON_BYTES)?;
        if self.context_references.len() > MAX_ITEMS {
            return Err(MultiAgentError::Validation(
                "agent message context reference count exceeds 256".into(),
            ));
        }
        for value in &self.context_references {
            validate_text("agent message context reference", value, 1024)?;
        }
        validate_actor(&self.actor)?;
        validate_size(self, "agent message")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CollaborationState {
    Assigned,
    Working,
    Waiting,
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
}
string_enum!(CollaborationState {
    Assigned => "ASSIGNED",
    Working => "WORKING",
    Waiting => "WAITING",
    Completed => "COMPLETED",
    Failed => "FAILED",
    Cancelled => "CANCELLED",
    OutcomeUnknown => "OUTCOME_UNKNOWN",
});

impl CollaborationState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationBinding {
    pub dispatch_id: Uuid,
    pub external_id: Uuid,
    pub external_kind: String,
    pub prepared_at: DateTime<Utc>,
}

impl CollaborationBinding {
    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_key("collaboration binding kind", &self.external_kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollaborationResult {
    pub summary: String,
    pub external_state: String,
    pub completed_at: DateTime<Utc>,
}

impl CollaborationResult {
    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_text("collaboration result summary", &self.summary, 2048)?;
        validate_key("collaboration external state", &self.external_state)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Collaboration {
    pub id: Uuid,
    pub team_id: Uuid,
    pub role_id: Option<Uuid>,
    pub source_member_id: Option<Uuid>,
    pub target_member_id: Uuid,
    pub goal: String,
    pub required_capabilities: BTreeSet<String>,
    pub priority: MessagePriority,
    pub handover_count: u32,
    pub state: CollaborationState,
    pub binding: Option<CollaborationBinding>,
    pub result: Option<CollaborationResult>,
    pub error: Option<String>,
    pub messages: Vec<AgentMessage>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl Collaboration {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        team_id: Uuid,
        role_id: Option<Uuid>,
        source_member_id: Option<Uuid>,
        target_member_id: Uuid,
        goal: impl Into<String>,
        required_capabilities: BTreeSet<String>,
        priority: MessagePriority,
        actor: impl Into<String>,
    ) -> MultiAgentResult<Self> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let actor = actor.into();
        let goal = goal.into();
        let message = AgentMessage {
            id: Uuid::new_v4(),
            correlation_id: id,
            source_member_id,
            target_member_id,
            intent: "team.assignment".into(),
            payload: serde_json::json!({ "goal": goal }),
            context_references: BTreeSet::new(),
            priority,
            actor: actor.clone(),
            created_at: now,
        };
        let value = Self {
            id,
            team_id,
            role_id,
            source_member_id,
            target_member_id,
            goal,
            required_capabilities,
            priority,
            handover_count: 0,
            state: CollaborationState::Assigned,
            binding: None,
            result: None,
            error: None,
            messages: vec![message],
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn dispatch_id(&self) -> Uuid {
        Uuid::new_v5(
            &self.id,
            format!("{}:{}", self.target_member_id, self.handover_count).as_bytes(),
        )
    }

    pub fn assignment_message(&self) -> MultiAgentResult<&AgentMessage> {
        self.messages.last().ok_or_else(|| {
            MultiAgentError::Validation("collaboration assignment message is missing".into())
        })
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_text("collaboration goal", &self.goal, 4096)?;
        validate_capabilities(&self.required_capabilities)?;
        validate_actor(&self.actor)?;
        if self.handover_count > 100
            || self.version == 0
            || self.updated_at < self.created_at
            || self.messages.is_empty()
            || self.messages.len() > MAX_ITEMS
            || self.error.as_ref().is_some_and(|value| value.len() > 4096)
        {
            return Err(MultiAgentError::Validation(
                "collaboration identity, version or bounded content is invalid".into(),
            ));
        }
        let mut message_ids = BTreeSet::new();
        for message in &self.messages {
            message.validate()?;
            if message.correlation_id != self.id || !message_ids.insert(message.id) {
                return Err(MultiAgentError::Validation(
                    "collaboration message correlation or identity is invalid".into(),
                ));
            }
        }
        let assignment = self.messages.last().ok_or_else(|| {
            MultiAgentError::Validation("collaboration assignment message is missing".into())
        })?;
        if assignment.target_member_id != self.target_member_id {
            return Err(MultiAgentError::Validation(
                "collaboration target does not match its latest message".into(),
            ));
        }
        if let Some(binding) = &self.binding {
            binding.validate()?;
            if binding.dispatch_id != self.dispatch_id() {
                return Err(MultiAgentError::Validation(
                    "collaboration binding does not match stable dispatch identity".into(),
                ));
            }
        }
        if let Some(result) = &self.result {
            result.validate()?;
        }
        if matches!(
            self.state,
            CollaborationState::Working | CollaborationState::Waiting
        ) && self.binding.is_none()
            || self.state == CollaborationState::Completed && self.result.is_none()
            || matches!(
                self.state,
                CollaborationState::Failed
                    | CollaborationState::Cancelled
                    | CollaborationState::OutcomeUnknown
            ) && self.error.is_none()
            || self.state.is_terminal() != self.completed_at.is_some()
            || self.state == CollaborationState::Assigned
                && (self.result.is_some() || self.completed_at.is_some())
        {
            return Err(MultiAgentError::Validation(
                "collaboration lifecycle is inconsistent".into(),
            ));
        }
        validate_size(self, "collaboration")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentAvailability {
    Available,
    Busy,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub agent_id: Uuid,
    pub capabilities: BTreeSet<String>,
    pub availability: AgentAvailability,
    pub workspace_id: Option<Uuid>,
}

impl AgentDescriptor {
    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_capabilities(&self.capabilities)
    }
}

#[derive(Debug, Clone)]
pub struct CreateTeamRequest {
    pub organization_id: Uuid,
    pub key: String,
    pub name: String,
    pub goal: String,
    pub workspace_id: Option<Uuid>,
    pub memory_scope: Option<String>,
    pub policy: TeamPolicyDefinition,
    pub metadata: MultiAgentMetadata,
    pub actor: String,
}

impl CreateTeamRequest {
    pub fn new(
        organization_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        goal: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            organization_id,
            key: key.into(),
            name: name.into(),
            goal: goal.into(),
            workspace_id: None,
            memory_scope: None,
            policy: TeamPolicyDefinition::default(),
            metadata: BTreeMap::new(),
            actor: actor.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AssignmentRequest {
    pub team_id: Uuid,
    pub role_id: Option<Uuid>,
    pub source_member_id: Option<Uuid>,
    pub goal: String,
    pub required_capabilities: BTreeSet<String>,
    pub priority: MessagePriority,
    pub actor: String,
}

impl AssignmentRequest {
    pub fn new(team_id: Uuid, goal: impl Into<String>, actor: impl Into<String>) -> Self {
        Self {
            team_id,
            role_id: None,
            source_member_id: None,
            goal: goal.into(),
            required_capabilities: BTreeSet::new(),
            priority: MessagePriority::Normal,
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> MultiAgentResult<()> {
        validate_text("assignment goal", &self.goal, 4096)?;
        validate_capabilities(&self.required_capabilities)?;
        validate_actor(&self.actor)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CollaborationOutcome {
    Completed(CollaborationResult),
    Waiting(String),
    Failed(String),
    OutcomeUnknown(String),
}

pub(crate) fn validate_actor(value: &str) -> MultiAgentResult<()> {
    validate_text("actor", value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> MultiAgentResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(MultiAgentError::Validation(format!(
            "{label} must be a safe 1..=128 character key"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> MultiAgentResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(MultiAgentError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: &str, max: usize) -> MultiAgentResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}

fn validate_entity_version(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    actor: &str,
) -> MultiAgentResult<()> {
    validate_actor(actor)?;
    if version == 0 || updated_at < created_at {
        return Err(MultiAgentError::Validation(
            "entity version or timestamps are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_capabilities(values: &BTreeSet<String>) -> MultiAgentResult<()> {
    if values.len() > MAX_ITEMS {
        return Err(MultiAgentError::Validation(
            "capability count exceeds 256".into(),
        ));
    }
    for value in values {
        validate_key("agent capability", value)?;
    }
    Ok(())
}

fn validate_metadata(value: &MultiAgentMetadata) -> MultiAgentResult<()> {
    if value.len() > MAX_ITEMS {
        return Err(MultiAgentError::Validation(
            "metadata entry count exceeds 256".into(),
        ));
    }
    validate_json("metadata", &serde_json::to_value(value)?, MAX_JSON_BYTES)
}

fn validate_json(label: &str, value: &Value, max: usize) -> MultiAgentResult<()> {
    reject_sensitive_keys(value, label, 0)?;
    if serde_json::to_vec(value)?.len() > max {
        return Err(MultiAgentError::Validation(format!(
            "{label} exceeds {max} bytes"
        )));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> MultiAgentResult<()> {
    if depth > 32 {
        return Err(MultiAgentError::Validation(format!(
            "{label} nesting exceeds 32"
        )));
    }
    match value {
        Value::Object(values) => {
            for (key, nested) in values {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    normalized.as_str(),
                    "password" | "secret" | "api_key" | "access_token" | "refresh_token"
                ) || normalized.ends_with("_password")
                    || normalized.ends_with("_secret")
                    || normalized.ends_with("_api_key")
                {
                    return Err(MultiAgentError::Validation(format!(
                        "{label} contains sensitive key {key}"
                    )));
                }
                reject_sensitive_keys(nested, label, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for nested in values {
                reject_sensitive_keys(nested, label, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> MultiAgentResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(MultiAgentError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collaboration_dispatch_is_stable_and_handover_changes_it() {
        let target = Uuid::new_v4();
        let mut value = Collaboration::new(
            Uuid::new_v4(),
            None,
            None,
            target,
            "implement feature",
            BTreeSet::new(),
            MessagePriority::Normal,
            "lead",
        )
        .unwrap();
        let first = value.dispatch_id();
        assert_eq!(first, value.dispatch_id());
        value.handover_count += 1;
        assert_ne!(first, value.dispatch_id());
    }

    #[test]
    fn nested_secrets_are_rejected() {
        let mut organization = Organization::new("engineering", "Engineering", "operator");
        organization.metadata.insert(
            "nested".into(),
            serde_json::json!({"credentials": {"api_key": "secret"}}),
        );
        assert!(matches!(
            organization.validate(),
            Err(MultiAgentError::Validation(_))
        ));
    }

    #[test]
    fn active_member_requires_current_collaboration() {
        let mut member = AgentMember::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            BTreeSet::new(),
            "operator",
        );
        member.state = MemberState::Working;
        assert!(matches!(
            member.validate(),
            Err(MultiAgentError::Validation(_))
        ));
    }
}
