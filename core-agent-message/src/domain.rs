use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{MessageError, MessageResult};

const MAX_TEXT: usize = 4096;
const MAX_INTENT: usize = 128;
const MAX_JSON_BYTES: usize = 256 * 1024;
const MAX_METADATA_ENTRIES: usize = 64;

pub type MessageMetadata = BTreeMap<String, Value>;

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

// ── MessageType ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageType {
    Request,
    Response,
    Event,
    Broadcast,
}
string_enum!(MessageType {
    Request => "REQUEST",
    Response => "RESPONSE",
    Event => "EVENT",
    Broadcast => "BROADCAST",
});

// ── MessagePriority ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

// ── MessageStatus ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageStatus {
    Pending,
    Delivered,
    Read,
    Failed,
}
string_enum!(MessageStatus {
    Pending => "PENDING",
    Delivered => "DELIVERED",
    Read => "READ",
    Failed => "FAILED",
});

// ── AgentMessage ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: Uuid,
    pub from_agent_id: Uuid,
    pub to_agent_id: Uuid,
    pub correlation_id: Option<Uuid>,
    pub message_type: MessageType,
    pub intent: String,
    pub payload: Value,
    pub priority: MessagePriority,
    pub status: MessageStatus,
    pub metadata: MessageMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentMessage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        from_agent_id: Uuid,
        to_agent_id: Uuid,
        message_type: MessageType,
        intent: String,
        payload: Value,
        priority: MessagePriority,
        actor: String,
    ) -> MessageResult<Self> {
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            from_agent_id,
            to_agent_id,
            correlation_id: None,
            message_type,
            intent,
            payload,
            priority,
            status: MessageStatus::Pending,
            metadata: BTreeMap::new(),
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> MessageResult<()> {
        validate_text("message actor", &self.actor, 256)?;
        if self.intent.is_empty()
            || self.intent.len() > MAX_INTENT
            || !self.intent.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
        {
            return Err(MessageError::Validation(
                "message intent must be 1..=128 safe characters".into(),
            ));
        }
        if serde_json::to_vec(&self.payload)?.len() > MAX_JSON_BYTES {
            return Err(MessageError::Validation(
                "message payload exceeds 256 KiB".into(),
            ));
        }
        reject_sensitive_keys(&self.payload, "message payload", 0)?;
        if self.metadata.len() > MAX_METADATA_ENTRIES {
            return Err(MessageError::Validation(
                "message metadata has more than 64 entries".into(),
            ));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(MessageError::Validation(
                "message version or timestamps are invalid".into(),
            ));
        }
        Ok(())
    }
}

// ── Validation helpers ──

fn validate_text(label: &str, value: &str, max: usize) -> MessageResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(MessageError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

fn reject_sensitive_keys(value: &Value, label: &str, depth: usize) -> MessageResult<()> {
    if depth > 32 {
        return Err(MessageError::Validation(format!("{label} nesting exceeds 32")));
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
                    "apikey", "accesstoken", "token", "password", "passwd",
                    "privatekey", "secret",
                ]
                .iter()
                .any(|needle| normalized == *needle || normalized.ends_with(needle))
                {
                    return Err(MessageError::Validation(format!(
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
    fn create_valid_message() {
        let msg = AgentMessage::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MessageType::Request,
            "ANALYSIS_REQUEST".into(),
            json!({"goal": "analyze"}),
            MessagePriority::Normal,
            "system".into(),
        )
        .unwrap();
        assert_eq!(msg.message_type, MessageType::Request);
        assert_eq!(msg.status, MessageStatus::Pending);
    }

    #[test]
    fn empty_intent_rejected() {
        let result = AgentMessage::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MessageType::Request,
            "".into(),
            json!({}),
            MessagePriority::Normal,
            "system".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn sensitive_payload_rejected() {
        let result = AgentMessage::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            MessageType::Request,
            "TEST".into(),
            json!({"password": "secret"}),
            MessagePriority::Normal,
            "system".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn message_type_roundtrip() {
        assert_eq!(MessageType::parse("REQUEST"), Some(MessageType::Request));
        assert_eq!(MessageType::parse("RESPONSE"), Some(MessageType::Response));
        assert_eq!(MessageType::parse("BROADCAST"), Some(MessageType::Broadcast));
    }

    #[test]
    fn priority_ordering() {
        assert!(MessagePriority::Critical > MessagePriority::High);
        assert!(MessagePriority::High > MessagePriority::Normal);
        assert!(MessagePriority::Normal > MessagePriority::Low);
    }

    #[test]
    fn status_roundtrip() {
        assert_eq!(MessageStatus::parse("PENDING"), Some(MessageStatus::Pending));
        assert_eq!(MessageStatus::parse("DELIVERED"), Some(MessageStatus::Delivered));
        assert_eq!(MessageStatus::parse("READ"), Some(MessageStatus::Read));
    }
}