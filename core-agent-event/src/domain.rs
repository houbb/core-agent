use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{EventError, EventResult};

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 1024;

pub type EventMetadata = BTreeMap<String, Value>;

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
pub enum EventCategory {
    System,
    Domain,
}
string_enum!(EventCategory {
    System => "SYSTEM",
    Domain => "DOMAIN",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventSourceKind {
    Agent,
    Execution,
    Tool,
    Workspace,
    Planner,
    Memory,
    Session,
    Context,
    Model,
    Plugin,
    Workflow,
    System,
}
string_enum!(EventSourceKind {
    Agent => "AGENT",
    Execution => "EXECUTION",
    Tool => "TOOL",
    Workspace => "WORKSPACE",
    Planner => "PLANNER",
    Memory => "MEMORY",
    Session => "SESSION",
    Context => "CONTEXT",
    Model => "MODEL",
    Plugin => "PLUGIN",
    Workflow => "WORKFLOW",
    System => "SYSTEM",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventPriority {
    Low,
    Normal,
    High,
    Critical,
}
string_enum!(EventPriority {
    Low => "LOW",
    Normal => "NORMAL",
    High => "HIGH",
    Critical => "CRITICAL",
});

impl EventPriority {
    pub fn rank(self) -> i64 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventVisibility {
    Internal,
    External,
}
string_enum!(EventVisibility {
    Internal => "INTERNAL",
    External => "EXTERNAL",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventState {
    Created,
    Published,
    Dispatched,
    Delivered,
    Handled,
    Archived,
}
string_enum!(EventState {
    Created => "CREATED",
    Published => "PUBLISHED",
    Dispatched => "DISPATCHED",
    Delivered => "DELIVERED",
    Handled => "HANDLED",
    Archived => "ARCHIVED",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryState {
    Pending,
    Delivered,
    Handled,
    Failed,
    DeadLettered,
}
string_enum!(DeliveryState {
    Pending => "PENDING",
    Delivered => "DELIVERED",
    Handled => "HANDLED",
    Failed => "FAILED",
    DeadLettered => "DEAD_LETTERED",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReplayState {
    Requested,
    Running,
    Completed,
    Failed,
}
string_enum!(ReplayState {
    Requested => "REQUESTED",
    Running => "RUNNING",
    Completed => "COMPLETED",
    Failed => "FAILED",
});

pub trait TypedEventPayload: Serialize + DeserializeOwned + Send + Sync + 'static {
    const EVENT_TYPE: &'static str;
    const CATEGORY: EventCategory;
    const SCHEMA_VERSION: u32 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSource {
    pub kind: EventSourceKind,
    pub id: Option<Uuid>,
}

impl EventSource {
    pub fn new(kind: EventSourceKind) -> Self {
        Self { kind, id: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDefinition {
    pub id: Uuid,
    pub key: String,
    pub category: EventCategory,
    pub payload_type: String,
    pub schema_version: u32,
    pub description: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl EventDefinition {
    pub fn for_payload<T: TypedEventPayload>(description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: T::EVENT_TYPE.into(),
            category: T::CATEGORY,
            payload_type: type_name::<T>().into(),
            schema_version: T::SCHEMA_VERSION,
            description: description.into(),
            active: true,
            created_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_key("event type", &self.key)?;
        validate_text("event payload type", &self.payload_type, 512)?;
        validate_text("event description", &self.description, 2048)?;
        if self.schema_version == 0 {
            return Err(EventError::Validation(
                "event schema version must be positive".into(),
            ));
        }
        validate_size(self, "event definition")
    }

    pub fn validate_event(&self, event: &EventEnvelope) -> EventResult<()> {
        self.validate()?;
        if !self.active
            || self.key != event.event_type
            || self.category != event.category
            || self.payload_type != event.payload_type
            || self.schema_version != event.schema_version
        {
            return Err(EventError::Validation(
                "event does not match its registered type definition".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDelivery {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub replay_id: Option<Uuid>,
    pub state: DeliveryState,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub handled_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

impl EventDelivery {
    pub fn new(subscription_id: Uuid, replay_id: Option<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            subscription_id,
            replay_id,
            state: DeliveryState::Pending,
            attempts: 0,
            last_error: None,
            delivered_at: None,
            handled_at: None,
            updated_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        if self.attempts > 10
            || self
                .last_error
                .as_ref()
                .is_some_and(|value| value.len() > 4096)
            || matches!(
                self.state,
                DeliveryState::Delivered | DeliveryState::Handled
            ) && self.delivered_at.is_none()
            || self.state == DeliveryState::Handled && self.handled_at.is_none()
            || self.state == DeliveryState::Pending && self.attempts != 0
            || matches!(
                self.state,
                DeliveryState::Failed | DeliveryState::DeadLettered
            ) && self.last_error.is_none()
        {
            return Err(EventError::Validation(
                "event delivery state, attempt or timestamps are inconsistent".into(),
            ));
        }
        validate_size(self, "event delivery")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: Uuid,
    pub event_type: String,
    pub category: EventCategory,
    pub namespace: String,
    pub source: EventSource,
    pub target: Option<String>,
    pub payload: Value,
    pub payload_type: String,
    pub metadata: EventMetadata,
    pub priority: EventPriority,
    pub visibility: EventVisibility,
    pub sensitive: bool,
    pub schema_version: u32,
    pub policy_id: Option<Uuid>,
    pub state: EventState,
    pub deliveries: Vec<EventDelivery>,
    pub occurred_at: DateTime<Utc>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventEnvelope {
    pub fn from_typed<T: TypedEventPayload>(
        namespace: impl Into<String>,
        source: EventSourceKind,
        payload: T,
        actor: impl Into<String>,
    ) -> EventResult<Self> {
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            event_type: T::EVENT_TYPE.into(),
            category: T::CATEGORY,
            namespace: namespace.into(),
            source: EventSource::new(source),
            target: None,
            payload: serde_json::to_value(payload)?,
            payload_type: type_name::<T>().into(),
            metadata: BTreeMap::new(),
            priority: EventPriority::Normal,
            visibility: EventVisibility::Internal,
            sensitive: false,
            schema_version: T::SCHEMA_VERSION,
            policy_id: None,
            state: EventState::Created,
            deliveries: Vec::new(),
            occurred_at: now,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn decode<T: TypedEventPayload>(&self) -> EventResult<T> {
        if self.event_type != T::EVENT_TYPE
            || self.category != T::CATEGORY
            || self.payload_type != type_name::<T>()
            || self.schema_version != T::SCHEMA_VERSION
        {
            return Err(EventError::Validation(
                "event payload type or schema version mismatch".into(),
            ));
        }
        Ok(serde_json::from_value(self.payload.clone())?)
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_key("event type", &self.event_type)?;
        validate_key("event namespace", &self.namespace)?;
        if let Some(target) = &self.target {
            validate_key("event target", target)?;
        }
        validate_text("event payload type", &self.payload_type, 512)?;
        validate_metadata(&self.metadata)?;
        validate_actor(&self.actor)?;
        reject_sensitive_keys(&self.payload, "event payload", 0)?;
        if serde_json::to_vec(&self.payload)?.len() > MAX_PAYLOAD_BYTES {
            return Err(EventError::Validation(
                "event payload exceeds 256 KiB".into(),
            ));
        }
        if self.schema_version == 0
            || self.version == 0
            || self.updated_at < self.created_at
            || self.deliveries.len() > MAX_ITEMS
        {
            return Err(EventError::Validation(
                "event version, timestamps or delivery count are invalid".into(),
            ));
        }
        let mut subscriptions = BTreeSet::new();
        for delivery in &self.deliveries {
            delivery.validate()?;
            if delivery.replay_id.is_none() && !subscriptions.insert(delivery.subscription_id) {
                return Err(EventError::Validation(
                    "event contains duplicate subscription delivery".into(),
                ));
            }
        }
        let lifecycle_is_consistent = match self.state {
            EventState::Created | EventState::Published => self.deliveries.is_empty(),
            EventState::Dispatched => self
                .deliveries
                .iter()
                .all(|delivery| delivery.state == DeliveryState::Pending),
            EventState::Delivered => {
                !self.deliveries.is_empty()
                    && self
                        .deliveries
                        .iter()
                        .any(|delivery| delivery.state != DeliveryState::Pending)
            }
            EventState::Handled => {
                !self.deliveries.is_empty()
                    && self
                        .deliveries
                        .iter()
                        .all(|delivery| delivery.state == DeliveryState::Handled)
            }
            EventState::Archived => self.deliveries.iter().all(|delivery| {
                matches!(
                    delivery.state,
                    DeliveryState::Handled | DeliveryState::DeadLettered
                )
            }),
        };
        if !lifecycle_is_consistent {
            return Err(EventError::Validation(
                "event lifecycle and delivery states are inconsistent".into(),
            ));
        }
        validate_size(self, "event")
    }

    pub fn payload_hash(&self) -> EventResult<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&self.payload)?)
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSubscription {
    pub id: Uuid,
    pub key: String,
    pub namespace: String,
    pub event_types: BTreeSet<String>,
    pub categories: BTreeSet<EventCategory>,
    pub sources: BTreeSet<EventSourceKind>,
    pub target: Option<String>,
    pub priority: i32,
    pub max_attempts: u32,
    pub policy_id: Option<Uuid>,
    pub enabled: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventSubscription {
    pub fn for_type(
        key: impl Into<String>,
        namespace: impl Into<String>,
        event_type: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            namespace: namespace.into(),
            event_types: BTreeSet::from([event_type.into()]),
            categories: BTreeSet::new(),
            sources: BTreeSet::new(),
            target: None,
            priority: 0,
            max_attempts: 3,
            policy_id: None,
            enabled: true,
            version: 1,
            actor: "system".into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_key("event subscription key", &self.key)?;
        validate_key("event subscription namespace", &self.namespace)?;
        for event_type in &self.event_types {
            validate_key("subscribed event type", event_type)?;
        }
        if let Some(target) = &self.target {
            validate_key("event subscription target", target)?;
        }
        validate_actor(&self.actor)?;
        if self.priority.abs() > 10_000
            || self.max_attempts == 0
            || self.max_attempts > 10
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(EventError::Validation(
                "event subscription priority, attempts, version or time is invalid".into(),
            ));
        }
        validate_size(self, "event subscription")
    }

    pub fn matches(&self, event: &EventEnvelope) -> bool {
        self.enabled
            && self.namespace == event.namespace
            && (self.event_types.is_empty() || self.event_types.contains(&event.event_type))
            && (self.categories.is_empty() || self.categories.contains(&event.category))
            && (self.sources.is_empty() || self.sources.contains(&event.source.kind))
            && self
                .target
                .as_ref()
                .is_none_or(|target| event.target.as_ref() == Some(target))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventPolicyDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub allow_sensitive_external: bool,
    pub allow_replay: bool,
    pub allowed_categories: BTreeSet<EventCategory>,
    pub allowed_sources: BTreeSet<EventSourceKind>,
    pub max_attempts: u32,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventPolicyDefinition {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            allow_sensitive_external: false,
            allow_replay: true,
            allowed_categories: BTreeSet::new(),
            allowed_sources: BTreeSet::new(),
            max_attempts: 3,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_key("event policy key", &self.key)?;
        validate_text("event policy name", &self.name, 256)?;
        if self.max_attempts == 0
            || self.max_attempts > 10
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(EventError::Validation(
                "event policy attempts, version or time is invalid".into(),
            ));
        }
        validate_size(self, "event policy")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventReplayRecord {
    pub id: Uuid,
    pub event_id: Uuid,
    pub subscription_ids: BTreeSet<Uuid>,
    pub state: ReplayState,
    pub deliveries: Vec<EventDelivery>,
    pub reason: String,
    pub actor: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventReplayRecord {
    pub fn new(event_id: Uuid, request: &ReplayRequest) -> EventResult<Self> {
        request.validate()?;
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            event_id,
            subscription_ids: request.subscription_ids.clone(),
            state: ReplayState::Requested,
            deliveries: Vec::new(),
            reason: request.reason.clone(),
            actor: request.actor.clone(),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_text("event replay reason", &self.reason, 2048)?;
        validate_actor(&self.actor)?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.deliveries.len() > MAX_ITEMS
        {
            return Err(EventError::Validation(
                "event replay version, time or delivery count is invalid".into(),
            ));
        }
        let mut subscriptions = BTreeSet::new();
        for delivery in &self.deliveries {
            delivery.validate()?;
            if delivery.replay_id != Some(self.id)
                || !subscriptions.insert(delivery.subscription_id)
            {
                return Err(EventError::Validation(
                    "event replay delivery ownership is invalid".into(),
                ));
            }
        }
        let lifecycle_is_consistent = match self.state {
            ReplayState::Requested => self.deliveries.is_empty(),
            ReplayState::Running => true,
            ReplayState::Completed => self
                .deliveries
                .iter()
                .all(|delivery| delivery.state == DeliveryState::Handled),
            ReplayState::Failed => {
                self.deliveries
                    .iter()
                    .any(|delivery| delivery.state == DeliveryState::DeadLettered)
                    && self.deliveries.iter().all(|delivery| {
                        matches!(
                            delivery.state,
                            DeliveryState::Handled | DeliveryState::DeadLettered
                        )
                    })
            }
        };
        if !lifecycle_is_consistent {
            return Err(EventError::Validation(
                "event replay lifecycle and delivery states are inconsistent".into(),
            ));
        }
        validate_size(self, "event replay")
    }
}

#[derive(Debug, Clone)]
pub struct ReplayRequest {
    pub event_id: Uuid,
    pub subscription_ids: BTreeSet<Uuid>,
    pub reason: String,
    pub actor: String,
}

impl ReplayRequest {
    pub fn new(event_id: Uuid, actor: impl Into<String>) -> Self {
        Self {
            event_id,
            subscription_ids: BTreeSet::new(),
            reason: "explicit event replay".into(),
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_text("event replay reason", &self.reason, 2048)?;
        validate_actor(&self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDeadLetter {
    pub id: Uuid,
    pub event_id: Uuid,
    pub subscription_id: Uuid,
    pub replay_id: Option<Uuid>,
    pub attempts: u32,
    pub error: String,
    pub payload_hash: String,
    pub resolved: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EventDeadLetter {
    pub fn new(
        event: &EventEnvelope,
        delivery: &EventDelivery,
        error: impl Into<String>,
        actor: impl Into<String>,
    ) -> EventResult<Self> {
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            event_id: event.id,
            subscription_id: delivery.subscription_id,
            replay_id: delivery.replay_id,
            attempts: delivery.attempts,
            error: error.into(),
            payload_hash: event.payload_hash()?,
            resolved: false,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> EventResult<()> {
        validate_text("event dead-letter error", &self.error, 4096)?;
        validate_text("event payload hash", &self.payload_hash, 128)?;
        validate_actor(&self.actor)?;
        if self.attempts == 0
            || self.attempts > 10
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(EventError::Validation(
                "event dead-letter attempts, version or time is invalid".into(),
            ));
        }
        validate_size(self, "event dead letter")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublishOutcome {
    pub event: EventEnvelope,
    pub handled: usize,
    pub dead_letters: Vec<EventDeadLetter>,
    pub idempotent: bool,
}

pub(crate) fn validate_actor(value: &str) -> EventResult<()> {
    validate_text("event actor", value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> EventResult<()> {
    validate_text(label, value, 386)?;
    if value.trim() != value || value.chars().any(char::is_whitespace) {
        return Err(EventError::Validation(format!(
            "{label} must be normalized and contain no whitespace"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> EventResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(EventError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_metadata(metadata: &EventMetadata) -> EventResult<()> {
    if metadata.len() > 64 {
        return Err(EventError::Validation(
            "event metadata has more than 64 entries".into(),
        ));
    }
    for key in metadata.keys() {
        validate_key("event metadata key", key)?;
    }
    let value = Value::Object(metadata.clone().into_iter().collect());
    reject_sensitive_keys(&value, "event metadata", 0)?;
    if serde_json::to_vec(metadata)?.len() > 64 * 1024 {
        return Err(EventError::Validation(
            "event metadata exceeds 64 KiB".into(),
        ));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> EventResult<()> {
    if depth > 64 {
        return Err(EventError::Validation(format!(
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
                    return Err(EventError::Validation(format!(
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

fn validate_size<T: Serialize>(value: &T, label: &str) -> EventResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(EventError::Validation(format!(
            "serialized {label} exceeds 1 MiB"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct BuildSucceeded {
        build_id: String,
    }

    impl TypedEventPayload for BuildSucceeded {
        const EVENT_TYPE: &'static str = "domain.build.succeeded";
        const CATEGORY: EventCategory = EventCategory::Domain;
    }

    #[test]
    fn typed_event_round_trips_and_matches_definition() {
        let event = EventEnvelope::from_typed(
            "tenant-a",
            EventSourceKind::Execution,
            BuildSucceeded {
                build_id: "build-1".into(),
            },
            "tester",
        )
        .unwrap();
        EventDefinition::for_payload::<BuildSucceeded>("build completed")
            .validate_event(&event)
            .unwrap();
        assert_eq!(
            event.decode::<BuildSucceeded>().unwrap().build_id,
            "build-1"
        );
    }

    #[test]
    fn subscription_never_crosses_namespace() {
        let event = EventEnvelope::from_typed(
            "tenant-a",
            EventSourceKind::Execution,
            BuildSucceeded {
                build_id: "build-1".into(),
            },
            "tester",
        )
        .unwrap();
        let subscription =
            EventSubscription::for_type("build-handler", "tenant-b", BuildSucceeded::EVENT_TYPE);
        assert!(!subscription.matches(&event));
    }

    #[test]
    fn nested_secret_keys_are_rejected() {
        let mut event = EventEnvelope::from_typed(
            "tenant-a",
            EventSourceKind::Execution,
            BuildSucceeded {
                build_id: "build-1".into(),
            },
            "tester",
        )
        .unwrap();
        event.payload = serde_json::json!({"nested": {"api_key": "secret"}});
        assert!(matches!(event.validate(), Err(EventError::Validation(_))));
    }
}
