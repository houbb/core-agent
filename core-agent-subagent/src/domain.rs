use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{SubAgentError, SubAgentResult};

const MAX_TEXT: usize = 4096;
const MAX_ITEMS: usize = 256;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

pub type SubAgentMetadata = BTreeMap<String, Value>;

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

// ── InstanceType ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstanceType {
    Manager,
    Worker,
}
string_enum!(InstanceType {
    Manager => "MANAGER",
    Worker => "WORKER",
});

// ── AgentRole ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentRole {
    Planner,
    Executor,
    Researcher,
    Reviewer,
    Monitor,
    DecisionMaker,
}
string_enum!(AgentRole {
    Planner => "PLANNER",
    Executor => "EXECUTOR",
    Researcher => "RESEARCHER",
    Reviewer => "REVIEWER",
    Monitor => "MONITOR",
    DecisionMaker => "DECISION_MAKER",
});

// ── SubAgentStatus ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SubAgentStatus {
    Created,
    Initialized,
    Running,
    Waiting,
    Completed,
    Failed,
    Destroyed,
}
string_enum!(SubAgentStatus {
    Created => "CREATED",
    Initialized => "INITIALIZED",
    Running => "RUNNING",
    Waiting => "WAITING",
    Completed => "COMPLETED",
    Failed => "FAILED",
    Destroyed => "DESTROYED",
});

impl SubAgentStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Destroyed)
    }
}

// ── AgentInstance ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentInstance {
    pub id: Uuid,
    pub name: String,
    pub instance_type: InstanceType,
    pub role: AgentRole,
    pub parent_agent_id: Option<Uuid>,
    pub supervisor_agent_id: Option<Uuid>,
    pub status: SubAgentStatus,
    pub config: Value,
    pub metadata: SubAgentMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentInstance {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        instance_type: InstanceType,
        role: AgentRole,
        parent_agent_id: Option<Uuid>,
        supervisor_agent_id: Option<Uuid>,
        config: Value,
        actor: String,
    ) -> SubAgentResult<Self> {
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            name,
            instance_type,
            role,
            parent_agent_id,
            supervisor_agent_id,
            status: SubAgentStatus::Created,
            config,
            metadata: BTreeMap::new(),
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> SubAgentResult<()> {
        validate_text("subagent name", &self.name, MAX_TEXT)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(SubAgentError::Validation(
                "subagent version or timestamps are invalid".into(),
            ));
        }
        reject_sensitive_keys(&self.config, "subagent config", 0)?;
        if self.metadata.len() > 64 {
            return Err(SubAgentError::Validation(
                "subagent metadata has more than 64 entries".into(),
            ));
        }
        if serde_json::to_vec(self)?.len() > MAX_DOCUMENT_BYTES {
            return Err(SubAgentError::Validation(
                "serialized AgentInstance exceeds 1 MiB".into(),
            ));
        }
        Ok(())
    }
}

// ── Validation helpers ──

pub(crate) fn validate_actor(value: &str) -> SubAgentResult<()> {
    validate_text("actor", value, 256)
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> SubAgentResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(SubAgentError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

pub(crate) fn validate_key(label: &str, value: &str) -> SubAgentResult<()> {
    validate_text(label, value, 128)?;
    if value.trim() != value || value.contains(char::is_whitespace) {
        return Err(SubAgentError::Validation(format!("{label} is not normalized")));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> SubAgentResult<()> {
    if depth > 64 {
        return Err(SubAgentError::Validation(format!(
            "{label} exceeds 64 levels of nesting"
        )));
    }
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let normalized = key
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if [
                    "apikey", "accesstoken", "refreshtoken", "authtoken", "token",
                    "password", "passwd", "authorization", "privatekey", "clientsecret",
                    "credential", "secret",
                ]
                .iter()
                .any(|needle| normalized == *needle || normalized.ends_with(needle))
                {
                    return Err(SubAgentError::Validation(format!(
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_instance_creation() {
        let instance = AgentInstance::new(
            "log-agent".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({}),
            "system".into(),
        )
        .unwrap();
        assert_eq!(instance.name, "log-agent");
        assert_eq!(instance.instance_type, InstanceType::Worker);
        assert_eq!(instance.role, AgentRole::Researcher);
        assert_eq!(instance.status, SubAgentStatus::Created);
        assert_eq!(instance.version, 1);
    }

    #[test]
    fn empty_name_is_rejected() {
        let result = AgentInstance::new(
            "".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({}),
            "system".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn invalid_actor_is_rejected() {
        let result = AgentInstance::new(
            "test".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({}),
            "\0".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn sensitive_config_is_rejected() {
        let result = AgentInstance::new(
            "test".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({"password": "secret"}),
            "system".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn status_enum_roundtrip() {
        assert_eq!(SubAgentStatus::parse("CREATED"), Some(SubAgentStatus::Created));
        assert_eq!(SubAgentStatus::parse("RUNNING"), Some(SubAgentStatus::Running));
        assert_eq!(SubAgentStatus::parse("UNKNOWN"), None);
        assert!(SubAgentStatus::Completed.is_terminal());
        assert!(SubAgentStatus::Failed.is_terminal());
        assert!(!SubAgentStatus::Running.is_terminal());
    }

    #[test]
    fn role_enum_roundtrip() {
        assert_eq!(AgentRole::parse("PLANNER"), Some(AgentRole::Planner));
        assert_eq!(AgentRole::parse("EXECUTOR"), Some(AgentRole::Executor));
        assert_eq!(AgentRole::parse("UNKNOWN"), None);
    }

    #[test]
    fn instance_type_roundtrip() {
        assert_eq!(InstanceType::parse("MANAGER"), Some(InstanceType::Manager));
        assert_eq!(InstanceType::parse("WORKER"), Some(InstanceType::Worker));
    }
}