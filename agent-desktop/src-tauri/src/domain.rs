use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{DesktopError, DesktopResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreferenceKind {
    Window,
    Layout,
    RecentProject,
    Theme,
    Shortcut,
}

impl PreferenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Window => "WINDOW",
            Self::Layout => "LAYOUT",
            Self::RecentProject => "RECENT_PROJECT",
            Self::Theme => "THEME",
            Self::Shortcut => "SHORTCUT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPreference {
    pub id: Uuid,
    pub key: String,
    pub kind: PreferenceKind,
    pub value: Value,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl UiPreference {
    pub fn new(
        key: impl Into<String>,
        kind: PreferenceKind,
        value: Value,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            kind,
            value,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> DesktopResult<()> {
        validate_key("preference key", &self.key)?;
        validate_key("preference actor", &self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(DesktopError::Validation(
                "preference version or timestamps are invalid".into(),
            ));
        }
        reject_sensitive(&self.value, 0)?;
        if serde_json::to_vec(&self.value)?.len() > 64 * 1024 {
            return Err(DesktopError::Validation(
                "preference value exceeds 64 KiB".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePreferenceRequest {
    pub key: String,
    pub kind: PreferenceKind,
    pub value: Value,
    pub expected_version: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNode {
    pub id: String,
    pub name: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSuggestion {
    pub name: String,
    pub usage: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeItem {
    pub path: String,
    pub status: String,
    pub additions: u64,
    pub deletions: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceStep {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub state: String,
    pub duration_ms: Option<u64>,
    pub tokens: Option<u64>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub key: String,
    pub name: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionItem {
    pub session_id: Uuid,
    pub title: String,
    pub state: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWorkspaceSnapshot {
    pub project_name: String,
    pub profile: String,
    pub model: String,
    pub project_tree: Vec<ProjectNode>,
    pub commands: Vec<CommandSuggestion>,
    pub changes: Vec<ChangeItem>,
    pub trace: Vec<TraceStep>,
    pub memory: Vec<MemoryItem>,
    pub tools: Vec<ToolStatus>,
    pub sessions: Vec<SessionItem>,
    pub resume_session: bool,
    pub permission_mode: String,
    pub config_sources: Vec<core_agent::ConfigSourceInfo>,
    pub effective_config: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessageRequest {
    pub message: String,
    pub session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSubmission {
    pub session_id: Option<Uuid>,
    pub response: Option<String>,
    pub action: core_agent::EnterpriseCommandAction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionRequest {
    pub approval_id: Uuid,
    pub decision: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeRequest {
    pub path: String,
    pub method: String,
    pub body: Option<Value>,
}

fn validate_key(label: &str, value: &str) -> DesktopResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(DesktopError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}

fn reject_sensitive(value: &Value, depth: usize) -> DesktopResult<()> {
    if depth > 16 {
        return Err(DesktopError::Validation(
            "preference nesting exceeds 16".into(),
        ));
    }
    match value {
        Value::Object(values) => {
            if values.len() > 256 {
                return Err(DesktopError::Validation(
                    "preference object exceeds 256 entries".into(),
                ));
            }
            for (key, value) in values {
                let key = key.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    key.as_str(),
                    "password" | "secret" | "api_key" | "access_token" | "refresh_token"
                ) || key.ends_with("_secret")
                    || key.ends_with("_password")
                {
                    return Err(DesktopError::Validation(
                        "preference cannot contain secrets".into(),
                    ));
                }
                reject_sensitive(value, depth + 1)?;
            }
        }
        Value::Array(values) => {
            if values.len() > 256 {
                return Err(DesktopError::Validation(
                    "preference array exceeds 256 entries".into(),
                ));
            }
            for value in values {
                reject_sensitive(value, depth + 1)?;
            }
        }
        Value::String(value) if value.chars().any(char::is_control) => {
            return Err(DesktopError::Validation(
                "preference contains control characters".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}
