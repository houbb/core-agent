use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{ApprovalError, ApprovalResult};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "LOW" => Some(Self::Low),
            "MEDIUM" => Some(Self::Medium),
            "HIGH" => Some(Self::High),
            "CRITICAL" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalType {
    Tool,
    Data,
    Workflow,
}

impl ApprovalType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "TOOL",
            Self::Data => "DATA",
            Self::Workflow => "WORKFLOW",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "TOOL" => Some(Self::Tool),
            "DATA" => Some(Self::Data),
            "WORKFLOW" => Some(Self::Workflow),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Expired,
    Cancelled,
    Executed,
}

impl ApprovalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Expired => "EXPIRED",
            Self::Cancelled => "CANCELLED",
            Self::Executed => "EXECUTED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "PENDING" => Some(Self::Pending),
            "APPROVED" => Some(Self::Approved),
            "REJECTED" => Some(Self::Rejected),
            "EXPIRED" => Some(Self::Expired),
            "CANCELLED" => Some(Self::Cancelled),
            "EXECUTED" => Some(Self::Executed),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected | Self::Expired | Self::Cancelled | Self::Executed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub id: Uuid,
    pub request_id: Uuid,
    pub principal_id: Uuid,
    pub decision: ApprovalState,
    pub comment: String,
    pub decided_at: DateTime<Utc>,
    pub actor: String,
}

impl ApprovalDecision {
    pub fn new(
        request_id: Uuid,
        principal_id: Uuid,
        decision: ApprovalState,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_id,
            principal_id,
            decision,
            comment: String::new(),
            decided_at: Utc::now(),
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> ApprovalResult<()> {
        validate_actor("approval decision actor", &self.actor)?;
        validate_text("approval decision comment", &self.comment, 2048)?;
        if !matches!(self.decision, ApprovalState::Approved | ApprovalState::Rejected) {
            return Err(ApprovalError::Validation(
                "approval decision must be Approved or Rejected".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub request_type: ApprovalType,
    pub requester: String,
    pub action: String,
    pub resource: String,
    pub parameters: Value,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub impact: String,
    pub state: ApprovalState,
    pub approvers: BTreeSet<String>,
    pub decisions: Vec<ApprovalDecision>,
    pub required_approvals: u8,
    pub version: u64,
    pub expires_at: Option<DateTime<Utc>>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApprovalRequest {
    pub fn new(
        tenant_id: Uuid,
        request_type: ApprovalType,
        requester: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        risk_level: RiskLevel,
    ) -> Self {
        let now = Utc::now();
        let requester = requester.into();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            request_type,
            requester: requester.clone(),
            action: action.into(),
            resource: resource.into(),
            parameters: Value::Null,
            risk_level,
            reason: String::new(),
            impact: String::new(),
            state: ApprovalState::Pending,
            approvers: BTreeSet::new(),
            decisions: Vec::new(),
            required_approvals: 1,
            version: 1,
            expires_at: None,
            actor: requester,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> ApprovalResult<()> {
        validate_actor("approval requester", &self.requester)?;
        validate_key("approval action", &self.action)?;
        validate_key("approval resource", &self.resource)?;
        validate_text("approval reason", &self.reason, 4096)?;
        validate_text("approval impact", &self.impact, 4096)?;
        validate_actor("approval actor", &self.actor)?;
        if self.version == 0
            || !(1..=8).contains(&self.required_approvals)
            || self.decisions.len() > 8
            || self.updated_at < self.created_at
        {
            return Err(ApprovalError::Validation(
                "approval request bounds are invalid".into(),
            ));
        }
        for approver in &self.approvers {
            validate_actor("approval approver", approver)?;
        }
        for decision in &self.decisions {
            decision.validate()?;
            if decision.request_id != self.id {
                return Err(ApprovalError::Validation(
                    "approval decision does not belong to this request".into(),
                ));
            }
        }
        if self.state == ApprovalState::Approved
            && self.decisions.len() < usize::from(self.required_approvals)
        {
            return Err(ApprovalError::Validation(
                "approval lacks required approvals".into(),
            ));
        }
        let mut principals = BTreeSet::new();
        for decision in &self.decisions {
            if !principals.insert(decision.principal_id) {
                return Err(ApprovalError::Validation(
                    "duplicate approver decision".into(),
                ));
            }
        }
        validate_size(self, "approval request")
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| Utc::now() > exp)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskRule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub action_pattern: String,
    pub resource_pattern: String,
    pub risk_level: RiskLevel,
    pub enabled: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RiskRule {
    pub fn new(
        tenant_id: Uuid,
        action_pattern: impl Into<String>,
        resource_pattern: impl Into<String>,
        risk_level: RiskLevel,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            action_pattern: action_pattern.into(),
            resource_pattern: resource_pattern.into(),
            risk_level,
            enabled: true,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn matches(&self, action: &str, resource: &str) -> bool {
        if !self.enabled {
            return false;
        }
        let action_match = self.action_pattern == "*"
            || action == self.action_pattern
            || (action.starts_with(&self.action_pattern.replace('*', ""))
                && self.action_pattern.contains('*'));
        let resource_match = self.resource_pattern == "*"
            || resource == self.resource_pattern
            || (resource.starts_with(&self.resource_pattern.replace('*', ""))
                && self.resource_pattern.contains('*'));
        action_match && resource_match
    }

    pub(crate) fn validate(&self) -> ApprovalResult<()> {
        validate_key("risk rule action pattern", &self.action_pattern)?;
        validate_key("risk rule resource pattern", &self.resource_pattern)?;
        validate_actor("risk rule actor", &self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(ApprovalError::Validation(
                "risk rule version or timestamps are invalid".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_actor(label: &str, value: &str) -> ApprovalResult<()> {
    if value.is_empty() {
        return Err(ApprovalError::Validation(format!("{label} must not be empty")));
    }
    validate_text(label, value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> ApprovalResult<()> {
    if value.is_empty() || value.len() > 386 || value.chars().any(char::is_whitespace) {
        return Err(ApprovalError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> ApprovalResult<()> {
    if value.len() > max || value.chars().any(char::is_control) {
        return Err(ApprovalError::Validation(format!(
            "{label} must contain 0..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> ApprovalResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(ApprovalError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_approval_request_passes_validate() {
        let req = ApprovalRequest::new(
            Uuid::new_v4(),
            ApprovalType::Tool,
            "operator",
            "kubectl.delete",
            "production/pod",
            RiskLevel::High,
        );
        req.validate().unwrap();
    }

    #[test]
    fn empty_requester_is_rejected() {
        let req = ApprovalRequest::new(
            Uuid::new_v4(),
            ApprovalType::Tool,
            "",
            "action",
            "resource",
            RiskLevel::Low,
        );
        assert!(matches!(req.validate(), Err(ApprovalError::Validation(_))));
    }

    #[test]
    fn risk_level_roundtrip() {
        for variant in &[RiskLevel::Low, RiskLevel::Medium, RiskLevel::High, RiskLevel::Critical] {
            let s = variant.as_str();
            let parsed = RiskLevel::parse(s).unwrap();
            assert_eq!(*variant, parsed);
        }
    }

    #[test]
    fn approval_state_terminal() {
        assert!(ApprovalState::Approved.is_terminal());
        assert!(ApprovalState::Rejected.is_terminal());
        assert!(ApprovalState::Executed.is_terminal());
        assert!(!ApprovalState::Pending.is_terminal());
    }

    #[test]
    fn risk_rule_pattern_matching() {
        let rule = RiskRule::new(
            Uuid::new_v4(),
            "kubectl.*",
            "production/*",
            RiskLevel::Critical,
            "admin",
        );
        assert!(rule.matches("kubectl.delete", "production/pod"));
        assert!(rule.matches("kubectl.get", "production/deployment"));
        assert!(!rule.matches("kubectl.delete", "staging/pod"));
        assert!(!rule.matches("docker.run", "production/pod"));

        let wildcard = RiskRule::new(
            Uuid::new_v4(),
            "*",
            "*",
            RiskLevel::Medium,
            "admin",
        );
        assert!(wildcard.matches("any", "thing"));
    }

    #[test]
    fn disabled_rule_does_not_match() {
        let mut rule = RiskRule::new(
            Uuid::new_v4(),
            "*",
            "*",
            RiskLevel::High,
            "admin",
        );
        rule.enabled = false;
        assert!(!rule.matches("anything", "anything"));
    }
}