use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{MemoryError, MemoryResult};

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_CONTENT_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 256;

pub type MemoryMetadata = BTreeMap<String, Value>;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryKind {
    Episodic,
    Semantic,
}
string_enum!(MemoryKind {
    Episodic => "EPISODIC",
    Semantic => "SEMANTIC",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryType {
    Experience,
    Knowledge,
    Preference,
    Fact,
    Workspace,
    Skill,
    Rule,
    Observation,
}
string_enum!(MemoryType {
    Experience => "EXPERIENCE",
    Knowledge => "KNOWLEDGE",
    Preference => "PREFERENCE",
    Fact => "FACT",
    Workspace => "WORKSPACE",
    Skill => "SKILL",
    Rule => "RULE",
    Observation => "OBSERVATION",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemorySourceKind {
    Conversation,
    Workspace,
    Tool,
    Execution,
    Agent,
    User,
    Plugin,
}
string_enum!(MemorySourceKind {
    Conversation => "CONVERSATION",
    Workspace => "WORKSPACE",
    Tool => "TOOL",
    Execution => "EXECUTION",
    Agent => "AGENT",
    User => "USER",
    Plugin => "PLUGIN",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryImportance {
    Temporary,
    Low,
    Medium,
    High,
    Critical,
}
string_enum!(MemoryImportance {
    Temporary => "TEMPORARY",
    Low => "LOW",
    Medium => "MEDIUM",
    High => "HIGH",
    Critical => "CRITICAL",
});

impl MemoryImportance {
    pub fn rank(self) -> i64 {
        match self {
            Self::Temporary => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryState {
    Created,
    Verified,
    Indexed,
    Recalled,
    Updated,
    Archived,
    Forgotten,
}
string_enum!(MemoryState {
    Created => "CREATED",
    Verified => "VERIFIED",
    Indexed => "INDEXED",
    Recalled => "RECALLED",
    Updated => "UPDATED",
    Archived => "ARCHIVED",
    Forgotten => "FORGOTTEN",
});

impl MemoryState {
    pub fn is_recallable(self) -> bool {
        matches!(self, Self::Indexed | Self::Recalled | Self::Updated)
    }

    pub fn is_snapshot_safe(self) -> bool {
        !matches!(self, Self::Created | Self::Verified | Self::Forgotten)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MemoryEventKind {
    Outcome,
    Preference,
    Knowledge,
    Fact,
    Observation,
    TemporaryLog,
}
string_enum!(MemoryEventKind {
    Outcome => "OUTCOME",
    Preference => "PREFERENCE",
    Knowledge => "KNOWLEDGE",
    Fact => "FACT",
    Observation => "OBSERVATION",
    TemporaryLog => "TEMPORARY_LOG",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryContent {
    pub title: String,
    pub body: String,
    pub data: Value,
}

impl MemoryContent {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            data: Value::Null,
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_text("memory title", &self.title, 1024)?;
        validate_optional_text("memory body", &self.body, MAX_CONTENT_BYTES)?;
        reject_sensitive_keys(&self.data, "memory content", 0)?;
        if serde_json::to_vec(self)?.len() > MAX_CONTENT_BYTES {
            return Err(MemoryError::Validation(
                "memory content exceeds 256 KiB".into(),
            ));
        }
        Ok(())
    }

    pub fn forgotten() -> Self {
        Self {
            title: "[forgotten]".into(),
            body: String::new(),
            data: Value::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySource {
    pub kind: MemorySourceKind,
    pub source_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
}

impl MemorySource {
    pub fn new(kind: MemorySourceKind) -> Self {
        Self {
            kind,
            source_id: None,
            session_id: None,
            workspace_id: None,
            agent_id: None,
            goal_id: None,
            execution_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryPolicyDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub allow_sensitive: bool,
    pub min_confidence: f64,
    pub retention_days: Option<u32>,
    pub temporary_retention_days: u32,
    pub allowed_types: BTreeSet<MemoryType>,
    pub allowed_sources: BTreeSet<MemorySourceKind>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryPolicyDefinition {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            allow_sensitive: false,
            min_confidence: 0.0,
            retention_days: Some(90),
            temporary_retention_days: 7,
            allowed_types: BTreeSet::new(),
            allowed_sources: BTreeSet::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_key("memory policy key", &self.key)?;
        validate_text("memory policy name", &self.name, 256)?;
        validate_confidence(self.min_confidence)?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.temporary_retention_days == 0
        {
            return Err(MemoryError::Validation(
                "memory policy version, time or retention is invalid".into(),
            ));
        }
        validate_size(self, "memory policy")
    }

    pub fn retention_for(&self, importance: MemoryImportance) -> Option<u32> {
        match importance {
            MemoryImportance::Critical => None,
            MemoryImportance::Temporary => Some(self.temporary_retention_days),
            _ => self.retention_days,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryEvent {
    pub id: Uuid,
    pub namespace: String,
    pub kind: MemoryEventKind,
    pub content: MemoryContent,
    pub source: MemorySource,
    pub suggested_kind: Option<MemoryKind>,
    pub suggested_type: Option<MemoryType>,
    pub suggested_importance: Option<MemoryImportance>,
    pub tags: BTreeSet<String>,
    pub metadata: MemoryMetadata,
    pub policy_id: Option<Uuid>,
    pub sensitive: bool,
    pub occurred_at: DateTime<Utc>,
    pub actor: String,
}

impl MemoryEvent {
    pub fn new(
        namespace: impl Into<String>,
        source: MemorySourceKind,
        content: MemoryContent,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            namespace: namespace.into(),
            kind: MemoryEventKind::Observation,
            content,
            source: MemorySource::new(source),
            suggested_kind: None,
            suggested_type: None,
            suggested_importance: None,
            tags: BTreeSet::new(),
            metadata: BTreeMap::new(),
            policy_id: None,
            sensitive: false,
            occurred_at: Utc::now(),
            actor: "system".into(),
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_key("memory namespace", &self.namespace)?;
        self.content.validate()?;
        validate_tags(&self.tags)?;
        validate_metadata(&self.metadata)?;
        validate_actor(&self.actor)?;
        validate_size(self, "memory event")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryClassification {
    pub remember: bool,
    pub kind: MemoryKind,
    pub memory_type: MemoryType,
    pub importance: MemoryImportance,
    pub confidence: f64,
    pub tags: BTreeSet<String>,
    pub reason: String,
}

impl MemoryClassification {
    pub fn validate(&self) -> MemoryResult<()> {
        validate_confidence(self.confidence)?;
        validate_tags(&self.tags)?;
        validate_text("classification reason", &self.reason, 2048)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Memory {
    pub id: Uuid,
    pub event_id: Uuid,
    pub namespace: String,
    pub kind: MemoryKind,
    pub memory_type: MemoryType,
    pub content: MemoryContent,
    pub metadata: MemoryMetadata,
    pub source: MemorySource,
    pub importance: MemoryImportance,
    pub confidence: f64,
    pub state: MemoryState,
    pub tags: BTreeSet<String>,
    pub policy: Option<MemoryPolicyDefinition>,
    pub reason: String,
    pub recall_count: u64,
    pub last_recalled_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Memory {
    pub fn from_event(
        event: MemoryEvent,
        classification: MemoryClassification,
        policy: Option<MemoryPolicyDefinition>,
        expires_at: Option<DateTime<Utc>>,
    ) -> MemoryResult<Self> {
        event.validate()?;
        classification.validate()?;
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            event_id: event.id,
            namespace: event.namespace,
            kind: classification.kind,
            memory_type: classification.memory_type,
            content: event.content,
            metadata: event.metadata,
            source: event.source,
            importance: classification.importance,
            confidence: classification.confidence,
            state: MemoryState::Created,
            tags: classification.tags,
            policy,
            reason: classification.reason,
            recall_count: 0,
            last_recalled_at: None,
            expires_at,
            version: 1,
            actor: event.actor,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_key("memory namespace", &self.namespace)?;
        self.content.validate()?;
        validate_metadata(&self.metadata)?;
        validate_tags(&self.tags)?;
        validate_confidence(self.confidence)?;
        validate_text("memory reason", &self.reason, 2048)?;
        validate_actor(&self.actor)?;
        if let Some(policy) = &self.policy {
            policy.validate()?;
        }
        if self.version == 0
            || self.updated_at < self.created_at
            || self
                .last_recalled_at
                .is_some_and(|time| time < self.created_at)
        {
            return Err(MemoryError::Validation(
                "memory version or timestamps are inconsistent".into(),
            ));
        }
        if self.state == MemoryState::Forgotten
            && (self.content != MemoryContent::forgotten()
                || !self.tags.is_empty()
                || !self.metadata.is_empty())
        {
            return Err(MemoryError::Validation(
                "forgotten memory must be a content-free tombstone".into(),
            ));
        }
        validate_size(self, "memory")
    }

    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now)
    }
}

#[derive(Debug, Clone)]
pub struct MemoryUpdate {
    pub expected_version: u64,
    pub content: Option<MemoryContent>,
    pub memory_type: Option<MemoryType>,
    pub kind: Option<MemoryKind>,
    pub importance: Option<MemoryImportance>,
    pub confidence: Option<f64>,
    pub tags: Option<BTreeSet<String>>,
    pub metadata: Option<MemoryMetadata>,
    pub reason: String,
    pub actor: String,
}

impl MemoryUpdate {
    pub fn new(expected_version: u64, actor: impl Into<String>) -> Self {
        Self {
            expected_version,
            content: None,
            memory_type: None,
            kind: None,
            importance: None,
            confidence: None,
            tags: None,
            metadata: None,
            reason: "memory updated".into(),
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        if self.expected_version == 0 {
            return Err(MemoryError::Validation(
                "expected memory version must be positive".into(),
            ));
        }
        if let Some(content) = &self.content {
            content.validate()?;
        }
        if let Some(confidence) = self.confidence {
            validate_confidence(confidence)?;
        }
        if let Some(tags) = &self.tags {
            validate_tags(tags)?;
        }
        if let Some(metadata) = &self.metadata {
            validate_metadata(metadata)?;
        }
        validate_text("memory update reason", &self.reason, 2048)?;
        validate_actor(&self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub namespace: String,
    pub text: Option<String>,
    pub kinds: BTreeSet<MemoryKind>,
    pub types: BTreeSet<MemoryType>,
    pub sources: BTreeSet<MemorySourceKind>,
    pub minimum_importance: Option<MemoryImportance>,
    pub tags: BTreeSet<String>,
    pub workspace_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub include_archived: bool,
    pub limit: usize,
    pub actor: String,
}

impl MemoryQuery {
    pub fn new(namespace: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            text: None,
            kinds: BTreeSet::new(),
            types: BTreeSet::new(),
            sources: BTreeSet::new(),
            minimum_importance: None,
            tags: BTreeSet::new(),
            workspace_id: None,
            agent_id: None,
            goal_id: None,
            created_after: None,
            created_before: None,
            include_archived: false,
            limit: 20,
            actor: "system".into(),
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_key("memory namespace", &self.namespace)?;
        if let Some(text) = &self.text {
            validate_text("memory query", text, 4096)?;
        }
        validate_tags(&self.tags)?;
        validate_actor(&self.actor)?;
        if self.limit == 0 || self.limit > 100 {
            return Err(MemoryError::Validation(
                "memory query limit must be within 1..=100".into(),
            ));
        }
        if self
            .created_after
            .zip(self.created_before)
            .is_some_and(|(after, before)| after > before)
        {
            return Err(MemoryError::Validation(
                "memory query time range is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryRecallHit {
    pub memory: Memory,
    pub score: i64,
    pub matched_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryIndexEntry {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub namespace: String,
    pub normalized_text: String,
    pub kind: MemoryKind,
    pub memory_type: MemoryType,
    pub source: MemorySourceKind,
    pub importance: MemoryImportance,
    pub state: MemoryState,
    pub workspace_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub goal_id: Option<Uuid>,
    pub memory_version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MemoryIndexEntry {
    pub fn validate_for(&self, memory: &Memory) -> MemoryResult<()> {
        if self.memory_id != memory.id
            || self.namespace != memory.namespace
            || self.kind != memory.kind
            || self.memory_type != memory.memory_type
            || self.source != memory.source.kind
            || self.importance != memory.importance
            || self.state != memory.state
            || self.workspace_id != memory.source.workspace_id
            || self.agent_id != memory.source.agent_id
            || self.goal_id != memory.source.goal_id
            || self.memory_version != memory.version
            || self.updated_at != memory.updated_at
            || self.created_at < memory.created_at
            || self.normalized_text.len() > MAX_CONTENT_BYTES
        {
            return Err(MemoryError::Validation(
                "memory index does not match aggregate".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySnapshot {
    pub id: Uuid,
    pub memory_id: Uuid,
    pub memory_version: u64,
    pub label: String,
    pub hash: String,
    pub content: Memory,
    pub created_at: DateTime<Utc>,
}

impl MemorySnapshot {
    pub fn new(memory: &Memory, label: impl Into<String>) -> MemoryResult<Self> {
        if !memory.state.is_snapshot_safe() {
            return Err(MemoryError::InvalidState(format!(
                "cannot snapshot {} memory",
                memory.state.as_str()
            )));
        }
        let value = Self {
            id: Uuid::new_v4(),
            memory_id: memory.id,
            memory_version: memory.version,
            label: label.into(),
            hash: semantic_hash(memory)?,
            content: memory.clone(),
            created_at: Utc::now(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> MemoryResult<()> {
        validate_text("memory snapshot label", &self.label, 256)?;
        self.content.validate()?;
        if self.memory_id != self.content.id
            || self.memory_version != self.content.version
            || !self.content.state.is_snapshot_safe()
            || self.hash != semantic_hash(&self.content)?
            || self.created_at < self.content.created_at
        {
            return Err(MemoryError::Validation(
                "memory snapshot identity or hash mismatch".into(),
            ));
        }
        validate_size(self, "memory snapshot")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RememberResult {
    pub event_id: Uuid,
    pub memory: Option<Memory>,
    pub reason: String,
}

pub fn normalize_tag(value: &str) -> MemoryResult<String> {
    let normalized = value.trim().to_lowercase();
    validate_key("memory tag", &normalized)?;
    Ok(normalized)
}

pub(crate) fn validate_actor(value: &str) -> MemoryResult<()> {
    validate_text("memory actor", value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> MemoryResult<()> {
    validate_text(label, value, 386)?;
    if value.trim() != value || value.chars().any(char::is_whitespace) {
        return Err(MemoryError::Validation(format!(
            "{label} must be normalized and contain no whitespace"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> MemoryResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(MemoryError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: &str, max: usize) -> MemoryResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}

fn validate_confidence(value: f64) -> MemoryResult<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(MemoryError::Validation(
            "memory confidence must be finite within 0..=1".into(),
        ));
    }
    Ok(())
}

fn validate_tags(tags: &BTreeSet<String>) -> MemoryResult<()> {
    if tags.len() > MAX_ITEMS {
        return Err(MemoryError::Validation(
            "memory has more than 256 tags".into(),
        ));
    }
    for tag in tags {
        if normalize_tag(tag)? != *tag {
            return Err(MemoryError::Validation(
                "memory tags must be normalized".into(),
            ));
        }
    }
    Ok(())
}

fn validate_metadata(metadata: &MemoryMetadata) -> MemoryResult<()> {
    if metadata.len() > 64 {
        return Err(MemoryError::Validation(
            "memory metadata has more than 64 entries".into(),
        ));
    }
    for key in metadata.keys() {
        validate_key("memory metadata key", key)?;
    }
    let value = Value::Object(metadata.clone().into_iter().collect());
    reject_sensitive_keys(&value, "memory metadata", 0)?;
    if serde_json::to_vec(metadata)?.len() > 64 * 1024 {
        return Err(MemoryError::Validation(
            "memory metadata exceeds 64 KiB".into(),
        ));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> MemoryResult<()> {
    if depth > 64 {
        return Err(MemoryError::Validation(format!(
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
                    return Err(MemoryError::Validation(format!(
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

fn validate_size<T: Serialize>(value: &T, label: &str) -> MemoryResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(MemoryError::Validation(format!(
            "serialized {label} exceeds 1 MiB"
        )));
    }
    Ok(())
}

pub(crate) fn semantic_hash<T: Serialize>(value: &T) -> MemoryResult<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use serde_json::json;

    use super::*;

    #[test]
    fn lifecycle_state_helpers_are_conservative() {
        assert!(MemoryState::Indexed.is_recallable());
        assert!(MemoryState::Recalled.is_recallable());
        assert!(MemoryState::Updated.is_recallable());
        assert!(!MemoryState::Archived.is_recallable());
        assert!(!MemoryState::Forgotten.is_snapshot_safe());
    }

    #[test]
    fn nested_sensitive_content_is_rejected() {
        let content = MemoryContent {
            title: "unsafe".into(),
            body: "content".into(),
            data: json!({"nested": {"api_key": "secret"}}),
        };

        assert!(matches!(
            content.validate(),
            Err(MemoryError::Validation(_))
        ));
    }

    #[test]
    fn query_rejects_invalid_limit_and_time_range() {
        let mut query = MemoryQuery::new("tenant-a");
        query.limit = 101;
        assert!(matches!(query.validate(), Err(MemoryError::Validation(_))));

        query.limit = 20;
        query.created_after = Some(Utc::now());
        query.created_before = query.created_after.map(|time| time - Duration::seconds(1));
        assert!(matches!(query.validate(), Err(MemoryError::Validation(_))));
    }
}
