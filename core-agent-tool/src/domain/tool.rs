use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ToolError, ToolRuntimeResult};

use super::{validate_metadata, PermissionDecision, ToolCapability, ToolLifecycleStatus};

const MAX_SCHEMA_BYTES: usize = 256 * 1024;
const MAX_PARAMETER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolProviderKind {
    Builtin,
    Mcp,
    Plugin,
    Workflow,
    Remote,
    Http,
    Other(String),
}

impl ToolProviderKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Builtin => "BUILTIN",
            Self::Mcp => "MCP",
            Self::Plugin => "PLUGIN",
            Self::Workflow => "WORKFLOW",
            Self::Remote => "REMOTE",
            Self::Http => "HTTP",
            Self::Other(value) => value,
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "BUILTIN" => Self::Builtin,
            "MCP" => Self::Mcp,
            "PLUGIN" => Self::Plugin,
            "WORKFLOW" => Self::Workflow,
            "REMOTE" => Self::Remote,
            "HTTP" => Self::Http,
            value => Self::Other(value.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolProviderDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub kind: ToolProviderKind,
    pub enabled: bool,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ToolProviderDefinition {
    pub fn new(key: impl Into<String>, name: impl Into<String>, kind: ToolProviderKind) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            kind,
            enabled: true,
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> ToolRuntimeResult<()> {
        validate_identity_part(&self.key, "provider key")?;
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(ToolError::InvalidArgument(
                "provider name must not be empty".into(),
            ));
        }
        if self.kind.as_str().trim().is_empty()
            || self.kind.as_str().len() > 64
            || self.kind.as_str().chars().any(char::is_control)
            || self.updated_at < self.created_at
        {
            return Err(ToolError::InvalidArgument(
                "provider kind/timestamps are invalid".into(),
            ));
        }
        validate_metadata(&self.metadata, "provider metadata")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub id: Uuid,
    pub key: String,
    pub provider_key: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub version: String,
    pub category: String,
    pub icon: Option<String>,
    pub tags: BTreeSet<String>,
    pub capabilities: BTreeSet<ToolCapability>,
    pub default_permission: PermissionDecision,
    pub timeout_ms: u64,
    pub enabled: bool,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ToolDefinition {
    pub fn new(
        provider_key: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        let provider_key = provider_key.into();
        let name = name.into();
        let version = version.into();
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: format!("{provider_key}/{name}@{version}"),
            provider_key,
            name,
            description: String::new(),
            input_schema,
            version,
            category: "other".into(),
            icon: None,
            tags: BTreeSet::new(),
            capabilities: BTreeSet::new(),
            default_permission: PermissionDecision::Ask,
            timeout_ms: 30_000,
            enabled: true,
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> ToolRuntimeResult<()> {
        validate_identity_part(&self.provider_key, "provider key")?;
        validate_identity_part(&self.name, "tool name")?;
        validate_identity_part(&self.version, "tool version")?;
        let expected = format!("{}/{}@{}", self.provider_key, self.name, self.version);
        if self.key != expected {
            return Err(ToolError::InvalidArgument(format!(
                "tool key must equal {expected}"
            )));
        }
        if !self.input_schema.is_object() {
            return Err(ToolError::InvalidArgument(
                "tool input_schema must be a JSON object".into(),
            ));
        }
        if serde_json::to_vec(&self.input_schema)
            .map_err(|error| ToolError::InvalidArgument(error.to_string()))?
            .len()
            > MAX_SCHEMA_BYTES
        {
            return Err(ToolError::InvalidArgument(
                "tool input_schema exceeds 256 KiB".into(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(ToolError::InvalidArgument(
                "tool timeout_ms must be greater than zero".into(),
            ));
        }
        if self.timeout_ms > i64::MAX as u64 {
            return Err(ToolError::InvalidArgument(
                "tool timeout_ms exceeds supported range".into(),
            ));
        }
        if self.category.trim().is_empty() || self.category.len() > 128 {
            return Err(ToolError::InvalidArgument(
                "tool category must not be empty".into(),
            ));
        }
        if self.description.len() > 4096
            || self.icon.as_ref().is_some_and(|value| value.len() > 2048)
            || self.tags.len() > 64
            || self
                .tags
                .iter()
                .any(|tag| tag.trim().is_empty() || tag.len() > 64)
            || self.capabilities.len() > 64
            || self.updated_at < self.created_at
        {
            return Err(ToolError::InvalidArgument(
                "tool catalog fields exceed their supported bounds".into(),
            ));
        }
        validate_metadata(&self.metadata, "tool metadata")
    }
}

fn validate_identity_part(value: &str, label: &str) -> ToolRuntimeResult<()> {
    if value.trim().is_empty()
        || value.len() > 128
        || value != value.trim()
        || value
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || ch == '/' || ch == '@')
    {
        return Err(ToolError::InvalidArgument(format!(
            "{label} must be a non-empty identity segment"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolRequest {
    pub id: Uuid,
    pub tool: String,
    pub parameters: serde_json::Value,
    pub session_id: Option<Uuid>,
    pub subject: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub timeout_ms: Option<u64>,
    pub created_at: DateTime<Utc>,
}

impl ToolRequest {
    pub fn new(tool: impl Into<String>, parameters: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            tool: tool.into(),
            parameters,
            session_id: None,
            subject: None,
            metadata: BTreeMap::new(),
            timeout_ms: None,
            created_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> ToolRuntimeResult<()> {
        if self.tool.trim().is_empty()
            || self.tool.len() > 386
            || self.tool.chars().any(char::is_control)
        {
            return Err(ToolError::InvalidArgument(
                "tool request key must not be empty".into(),
            ));
        }
        if self.timeout_ms == Some(0) {
            return Err(ToolError::InvalidArgument(
                "tool request timeout_ms must be greater than zero".into(),
            ));
        }
        if serde_json::to_vec(&self.parameters)
            .map_err(|error| ToolError::InvalidArgument(error.to_string()))?
            .len()
            > MAX_PARAMETER_BYTES
        {
            return Err(ToolError::InvalidArgument(
                "tool parameters exceed 1 MiB".into(),
            ));
        }
        if self.subject.as_ref().is_some_and(|value| {
            value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(ToolError::InvalidArgument(
                "tool request subject must not be empty".into(),
            ));
        }
        validate_metadata(&self.metadata, "tool request metadata")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ToolContent {
    Text(String),
    Json(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolAttachment {
    pub id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub uri: String,
    pub size_bytes: Option<u64>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolUsage {
    pub duration_ms: u64,
    pub output_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolFailure {
    pub kind: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub request_id: Uuid,
    pub tool_key: String,
    pub status: ToolLifecycleStatus,
    pub content: Vec<ToolContent>,
    pub attachments: Vec<ToolAttachment>,
    pub usage: ToolUsage,
    pub error: Option<ToolFailure>,
    pub metadata: BTreeMap<String, String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

impl ToolResult {
    pub fn validate(&self) -> ToolRuntimeResult<()> {
        if self.tool_key.trim().is_empty() || !self.status.is_terminal() {
            return Err(ToolError::Mapping(
                "tool result needs an identity and terminal status".into(),
            ));
        }
        if self.completed_at < self.started_at
            || (self.status == ToolLifecycleStatus::Success) != self.error.is_none()
            || self
                .error
                .as_ref()
                .is_some_and(|error| error.kind.trim().is_empty() || error.kind.len() > 64)
        {
            return Err(ToolError::Mapping(
                "tool result status, error or timestamps are inconsistent".into(),
            ));
        }
        validate_metadata(&self.metadata, "tool result metadata")
    }

    pub fn failed(
        request_id: Uuid,
        tool_key: impl Into<String>,
        error: &ToolError,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            request_id,
            tool_key: tool_key.into(),
            status: ToolLifecycleStatus::Failed,
            content: Vec::new(),
            attachments: Vec::new(),
            usage: ToolUsage {
                duration_ms: elapsed_ms(started_at, completed_at),
                output_bytes: 0,
            },
            error: Some(ToolFailure {
                kind: error.kind().into(),
                message: error.to_string(),
                retryable: error.is_retryable(),
            }),
            metadata: BTreeMap::new(),
            started_at,
            completed_at,
        }
    }

    pub fn cancelled(
        request_id: Uuid,
        tool_key: impl Into<String>,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
    ) -> Self {
        let error = ToolError::Cancelled(request_id.to_string());
        let mut result = Self::failed(request_id, tool_key, &error, started_at, completed_at);
        result.status = ToolLifecycleStatus::Cancelled;
        result
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RawToolOutput {
    pub content: Vec<ToolContent>,
    pub attachments: Vec<ToolAttachment>,
    pub metadata: BTreeMap<String, String>,
}

impl RawToolOutput {
    pub fn text(value: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text(value.into())],
            ..Self::default()
        }
    }
}

pub(crate) fn elapsed_ms(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    end.signed_duration_since(start).num_milliseconds().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_has_stable_key_and_ask_default() {
        let definition = ToolDefinition::new(
            "builtin",
            "echo",
            "1.0.0",
            serde_json::json!({"type":"object"}),
        );
        assert_eq!(definition.key, "builtin/echo@1.0.0");
        assert_eq!(definition.default_permission, PermissionDecision::Ask);
        assert!(definition.validate().is_ok());
    }

    #[test]
    fn request_rejects_zero_timeout() {
        let mut request = ToolRequest::new("builtin/echo@1.0.0", serde_json::json!({}));
        request.timeout_ms = Some(0);
        assert!(request.validate().is_err());
    }
}
