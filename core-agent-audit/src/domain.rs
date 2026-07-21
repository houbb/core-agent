use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{AuditError, AuditResult};

const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditEventType {
    AgentCreated,
    AgentRun,
    ToolCall,
    ToolResult,
    Decision,
    PermissionGrant,
    PermissionDeny,
    WorkflowState,
    WorkflowAction,
    Approval,
    CostRecord,
    DataAccess,
    System,
}

impl AuditEventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentCreated => "AGENT_CREATED",
            Self::AgentRun => "AGENT_RUN",
            Self::ToolCall => "TOOL_CALL",
            Self::ToolResult => "TOOL_RESULT",
            Self::Decision => "DECISION",
            Self::PermissionGrant => "PERMISSION_GRANT",
            Self::PermissionDeny => "PERMISSION_DENY",
            Self::WorkflowState => "WORKFLOW_STATE",
            Self::WorkflowAction => "WORKFLOW_ACTION",
            Self::Approval => "APPROVAL",
            Self::CostRecord => "COST_RECORD",
            Self::DataAccess => "DATA_ACCESS",
            Self::System => "SYSTEM",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "AGENT_CREATED" => Some(Self::AgentCreated),
            "AGENT_RUN" => Some(Self::AgentRun),
            "TOOL_CALL" => Some(Self::ToolCall),
            "TOOL_RESULT" => Some(Self::ToolResult),
            "DECISION" => Some(Self::Decision),
            "PERMISSION_GRANT" => Some(Self::PermissionGrant),
            "PERMISSION_DENY" => Some(Self::PermissionDeny),
            "WORKFLOW_STATE" => Some(Self::WorkflowState),
            "WORKFLOW_ACTION" => Some(Self::WorkflowAction),
            "APPROVAL" => Some(Self::Approval),
            "COST_RECORD" => Some(Self::CostRecord),
            "DATA_ACCESS" => Some(Self::DataAccess),
            "SYSTEM" => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl AuditSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARNING" => Some(Self::Warning),
            "ERROR" => Some(Self::Error),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub actor: String,
    pub event_type: AuditEventType,
    pub action: String,
    pub resource: String,
    pub payload: Value,
    pub severity: AuditSeverity,
    pub result: String,
    pub request_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub trace_id: Option<String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub occurred_at: DateTime<Utc>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

impl AuditEvent {
    pub fn new(
        tenant_id: Uuid,
        actor: impl Into<String>,
        event_type: AuditEventType,
        action: impl Into<String>,
        resource: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            actor: actor.into(),
            event_type,
            action: action.into(),
            resource: resource.into(),
            payload: Value::Null,
            severity: AuditSeverity::Info,
            result: "success".into(),
            request_id: None,
            session_id: None,
            trace_id: None,
            client_ip: None,
            user_agent: None,
            occurred_at: now,
            version: 1,
            created_at: now,
        }
    }

    pub fn validate(&self) -> AuditResult<()> {
        validate_actor("audit actor", &self.actor)?;
        validate_key("audit action", &self.action)?;
        validate_key("audit resource", &self.resource)?;
        validate_text("audit result", &self.result, 256)?;
        if self.version == 0 || self.created_at < self.occurred_at {
            return Err(AuditError::Validation(
                "audit event version or timestamps are invalid".into(),
            ));
        }
        let payload_bytes = serde_json::to_vec(&self.payload)?;
        if payload_bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(AuditError::Validation(format!(
                "audit payload exceeds {MAX_PAYLOAD_BYTES} bytes"
            )));
        }
        validate_size(self, "audit event")
    }

    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }

    pub fn with_severity(mut self, severity: AuditSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = result.into();
        self
    }

    pub fn with_request_id(mut self, request_id: Uuid) -> Self {
        self.request_id = Some(request_id);
        self
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct AuditQuery {
    pub tenant_id: Option<Uuid>,
    pub actor: Option<String>,
    pub event_type: Option<AuditEventType>,
    pub action: Option<String>,
    pub resource: Option<String>,
    pub severity: Option<AuditSeverity>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for AuditQuery {
    fn default() -> Self {
        Self {
            tenant_id: None,
            actor: None,
            event_type: None,
            action: None,
            resource: None,
            severity: None,
            from: None,
            to: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl AuditQuery {
    pub fn validate(&self) -> AuditResult<()> {
        if self.limit == 0 || self.limit > 10000 {
            return Err(AuditError::Validation(
                "audit query limit must be within 1..=10000".into(),
            ));
        }
        if let Some(actor) = &self.actor {
            validate_actor("audit query actor", actor)?;
        }
        if let Some(action) = &self.action {
            validate_key("audit query action", action)?;
        }
        if let Some(resource) = &self.resource {
            validate_key("audit query resource", resource)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSnapshot {
    pub tenant_id: Uuid,
    pub total_events: u64,
    pub by_event_type: BTreeMap<String, u64>,
    pub by_severity: BTreeMap<String, u64>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

pub(crate) fn validate_actor(label: &str, value: &str) -> AuditResult<()> {
    validate_text(label, value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> AuditResult<()> {
    if value.is_empty()
        || value.len() > 386
        || value.chars().any(char::is_whitespace)
    {
        return Err(AuditError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> AuditResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(AuditError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> AuditResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(AuditError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> AuditEvent {
        AuditEvent::new(
            Uuid::new_v4(),
            "test-agent",
            AuditEventType::ToolCall,
            "file.read",
            "workspace/config.yaml",
        )
    }

    #[test]
    fn valid_event_passes_validate() {
        let event = sample_event();
        assert!(event.validate().is_ok());
    }

    #[test]
    fn empty_actor_is_rejected() {
        let event = AuditEvent::new(
            Uuid::new_v4(),
            "",
            AuditEventType::System,
            "test",
            "test",
        );
        assert!(matches!(event.validate(), Err(AuditError::Validation(_))));
    }

    #[test]
    fn zero_version_is_rejected() {
        let mut event = sample_event();
        event.version = 0;
        assert!(matches!(event.validate(), Err(AuditError::Validation(_))));
    }

    #[test]
    fn builder_methods_work() {
        let event = sample_event()
            .with_payload(serde_json::json!({"key": "value"}))
            .with_severity(AuditSeverity::Critical)
            .with_result("denied")
            .with_request_id(Uuid::new_v4())
            .with_session_id(Uuid::new_v4())
            .with_trace_id("trace-001");
        assert_eq!(event.severity, AuditSeverity::Critical);
        assert_eq!(event.result, "denied");
        assert!(event.request_id.is_some());
        assert!(event.session_id.is_some());
        assert_eq!(event.trace_id.unwrap(), "trace-001");
    }

    #[test]
    fn audit_query_default_is_valid() {
        let query = AuditQuery::default();
        assert!(query.validate().is_ok());
    }

    #[test]
    fn audit_query_zero_limit_is_rejected() {
        let query = AuditQuery {
            limit: 0,
            offset: 0,
            ..Default::default()
        };
        assert!(matches!(query.validate(), Err(AuditError::Validation(_))));
    }

    #[test]
    fn event_type_roundtrip() {
        for variant in &[
            AuditEventType::AgentCreated,
            AuditEventType::AgentRun,
            AuditEventType::ToolCall,
            AuditEventType::ToolResult,
            AuditEventType::Decision,
            AuditEventType::PermissionGrant,
            AuditEventType::PermissionDeny,
            AuditEventType::WorkflowState,
            AuditEventType::WorkflowAction,
            AuditEventType::Approval,
            AuditEventType::CostRecord,
            AuditEventType::DataAccess,
            AuditEventType::System,
        ] {
            let s = variant.as_str();
            let parsed = AuditEventType::parse(s).unwrap();
            assert_eq!(*variant, parsed);
        }
    }

    #[test]
    fn severity_roundtrip() {
        for variant in &[
            AuditSeverity::Debug,
            AuditSeverity::Info,
            AuditSeverity::Warning,
            AuditSeverity::Error,
            AuditSeverity::Critical,
        ] {
            let s = variant.as_str();
            let parsed = AuditSeverity::parse(s).unwrap();
            assert_eq!(*variant, parsed);
        }
    }
}