use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ToolError, ToolRuntimeResult};

use super::{audit_metadata, validate_audit_metadata, validate_metadata, ToolCapability};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PermissionDecision {
    Allow,
    Ask,
    Deny,
}

impl PermissionDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "ALLOW",
            Self::Ask => "ASK",
            Self::Deny => "DENY",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ALLOW" => Some(Self::Allow),
            "ASK" => Some(Self::Ask),
            "DENY" => Some(Self::Deny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolLifecycleStatus {
    Created,
    Ready,
    Running,
    Success,
    Failed,
    Cancelled,
}

impl ToolLifecycleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CREATED" => Some(Self::Created),
            "READY" => Some(Self::Ready),
            "RUNNING" => Some(Self::Running),
            "SUCCESS" => Some(Self::Success),
            "FAILED" => Some(Self::Failed),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Created, Self::Ready | Self::Failed | Self::Cancelled)
                | (Self::Ready, Self::Running | Self::Failed | Self::Cancelled)
                | (
                    Self::Running,
                    Self::Success | Self::Failed | Self::Cancelled
                )
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolExecutionRecord {
    pub id: Uuid,
    pub request_id: Uuid,
    pub tool_key: String,
    pub provider_key: String,
    pub session_id: Option<Uuid>,
    pub subject: Option<String>,
    pub status: ToolLifecycleStatus,
    pub latency_ms: u64,
    pub error_kind: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ToolExecutionRecord {
    pub fn new(
        request_id: Uuid,
        tool_key: impl Into<String>,
        provider_key: impl Into<String>,
        session_id: Option<Uuid>,
        subject: Option<String>,
        metadata: &BTreeMap<String, String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            request_id,
            tool_key: tool_key.into(),
            provider_key: provider_key.into(),
            session_id,
            subject,
            status: ToolLifecycleStatus::Created,
            latency_ms: 0,
            error_kind: None,
            metadata: audit_metadata(metadata),
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn transition(&mut self, next: ToolLifecycleStatus) -> ToolRuntimeResult<()> {
        if !self.status.can_transition_to(next) {
            return Err(ToolError::Lifecycle(format!(
                "invalid transition {} -> {}",
                self.status.as_str(),
                next.as_str()
            )));
        }
        self.status = next;
        self.updated_at = Utc::now();
        if next == ToolLifecycleStatus::Running {
            self.started_at = Some(self.updated_at);
        }
        if next.is_terminal() {
            self.completed_at = Some(self.updated_at);
        }
        Ok(())
    }

    pub fn validate(&self) -> ToolRuntimeResult<()> {
        if self.tool_key.trim().is_empty() || self.provider_key.trim().is_empty() {
            return Err(ToolError::InvalidArgument(
                "execution tool/provider key must not be empty".into(),
            ));
        }
        if self.subject.as_ref().is_some_and(|value| {
            value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(ToolError::InvalidArgument(
                "execution subject must not be empty".into(),
            ));
        }
        if self.updated_at < self.created_at
            || matches!(
                self.status,
                ToolLifecycleStatus::Running | ToolLifecycleStatus::Success
            ) && self.started_at.is_none()
            || self.status.is_terminal() && self.completed_at.is_none()
            || !self.status.is_terminal() && self.completed_at.is_some()
        {
            return Err(ToolError::InvalidArgument(
                "execution lifecycle timestamps are inconsistent".into(),
            ));
        }
        validate_audit_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolPermissionRule {
    pub id: Uuid,
    pub tool_key: Option<String>,
    pub capability: Option<ToolCapability>,
    pub subject: Option<String>,
    pub decision: PermissionDecision,
    pub priority: i32,
    pub enabled: bool,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ToolPermissionRule {
    pub fn for_tool(tool_key: impl Into<String>, decision: PermissionDecision) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tool_key: Some(tool_key.into()),
            capability: None,
            subject: None,
            decision,
            priority: 0,
            enabled: true,
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> ToolRuntimeResult<()> {
        if self.tool_key.is_some() == self.capability.is_some() {
            return Err(ToolError::InvalidArgument(
                "permission rule needs exactly one tool key or capability".into(),
            ));
        }
        if self
            .tool_key
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
            || self
                .subject
                .as_ref()
                .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ToolError::InvalidArgument(
                "permission rule key/subject must not be empty".into(),
            ));
        }
        if self.updated_at < self.created_at {
            return Err(ToolError::InvalidArgument(
                "permission updated_at precedes created_at".into(),
            ));
        }
        validate_metadata(&self.metadata, "permission metadata")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_accepts_only_forward_transitions() {
        assert!(ToolLifecycleStatus::Created.can_transition_to(ToolLifecycleStatus::Ready));
        assert!(!ToolLifecycleStatus::Created.can_transition_to(ToolLifecycleStatus::Success));
        assert!(!ToolLifecycleStatus::Success.can_transition_to(ToolLifecycleStatus::Running));
    }

    #[test]
    fn permission_rule_requires_scope() {
        let mut rule = ToolPermissionRule::for_tool("builtin/echo@1", PermissionDecision::Allow);
        rule.tool_key = None;
        assert!(rule.validate().is_err());
        rule.tool_key = Some("builtin/echo@1".into());
        rule.capability = Some(ToolCapability::new("utility.echo").unwrap());
        assert!(rule.validate().is_err());
    }
}
