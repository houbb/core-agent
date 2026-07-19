use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{PlatformError, PlatformResult};

const MAX_ITEMS: usize = 256;
const MAX_JSON_BYTES: usize = 256 * 1024;

pub type PlatformMetadata = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
            pub fn parse(value: &str) -> Option<Self> { match value { $($value => Some(Self::$variant),)+ _ => None } }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TenantState {
    Active,
    Suspended,
    Archived,
}
string_enum!(TenantState { Active=>"ACTIVE", Suspended=>"SUSPENDED", Archived=>"ARCHIVED" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tenant {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub state: TenantState,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Tenant {
    pub fn new(key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            state: TenantState::Active,
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("tenant key", &self.key)?;
        validate_text("tenant name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformOrganization {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PlatformOrganization {
    pub fn new(
        tenant_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            parent_id: None,
            key: key.into(),
            name: name.into(),
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("organization key", &self.key)?;
        validate_text("organization name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        if self.parent_id == Some(self.id) {
            return Err(PlatformError::Validation(
                "Organization cannot parent itself".into(),
            ));
        }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyEffect {
    Allow,
    Deny,
}
string_enum!(PolicyEffect { Allow=>"ALLOW", Deny=>"DENY" });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub id: Uuid,
    pub subjects: BTreeSet<String>,
    pub actions: BTreeSet<String>,
    pub resources: BTreeSet<String>,
    pub attributes: BTreeMap<String, String>,
    pub effect: PolicyEffect,
    pub priority: i32,
}

impl PolicyRule {
    pub fn validate(&self) -> PlatformResult<()> {
        for (label, values) in [
            ("subject", &self.subjects),
            ("action", &self.actions),
            ("resource", &self.resources),
        ] {
            if values.is_empty() || values.len() > MAX_ITEMS {
                return Err(PlatformError::Validation(format!(
                    "Policy {label} set is invalid"
                )));
            }
            for value in values {
                if value != "*" {
                    validate_key(label, value)?;
                }
            }
        }
        if self.attributes.len() > MAX_ITEMS {
            return Err(PlatformError::Validation(
                "Policy attributes exceed 256".into(),
            ));
        }
        for (key, value) in &self.attributes {
            validate_key("attribute key", key)?;
            validate_text("attribute value", value, 256)?;
        }
        Ok(())
    }
    pub fn matches(&self, request: &GovernanceRequest) -> bool {
        matches_set(&self.subjects, &request.subject)
            && matches_set(&self.actions, &request.action)
            && matches_set(&self.resources, &request.resource)
            && self
                .attributes
                .iter()
                .all(|(k, v)| request.attributes.get(k) == Some(v))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlatformPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub enabled: bool,
    pub rules: Vec<PolicyRule>,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl PlatformPolicy {
    pub fn new(
        tenant_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            key: key.into(),
            name: name.into(),
            enabled: true,
            rules: Vec::new(),
            metadata: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("policy key", &self.key)?;
        validate_text("policy name", &self.name, 256)?;
        if self.rules.len() > MAX_ITEMS {
            return Err(PlatformError::Validation("Policy rules exceed 256".into()));
        }
        let mut ids = BTreeSet::new();
        for rule in &self.rules {
            rule.validate()?;
            if !ids.insert(rule.id) {
                return Err(PlatformError::Validation("duplicate Policy rule id".into()));
            }
        }
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub limit: u64,
    pub consumed: u64,
    pub window_seconds: u64,
    pub window_started_at: DateTime<Utc>,
    pub window_ends_at: DateTime<Utc>,
    pub ledger: BTreeMap<Uuid, u64>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Quota {
    pub fn new(
        tenant_id: Uuid,
        key: impl Into<String>,
        limit: u64,
        window_seconds: u64,
        actor: impl Into<String>,
    ) -> PlatformResult<Self> {
        let now = Utc::now();
        let seconds = i64::try_from(window_seconds)
            .map_err(|_| PlatformError::Validation("quota window too large".into()))?;
        let value = Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            key: key.into(),
            limit,
            consumed: 0,
            window_seconds,
            window_started_at: now,
            window_ends_at: now + Duration::seconds(seconds),
            ledger: BTreeMap::new(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("quota key", &self.key)?;
        if self.limit == 0
            || self.window_seconds == 0
            || self.window_seconds > 31_536_000
            || self.consumed > self.limit
            || self.window_ends_at <= self.window_started_at
            || self.ledger.len() > 1024
            || self.ledger.values().copied().sum::<u64>() != self.consumed
        {
            return Err(PlatformError::Validation(
                "Quota bounds or ledger are inconsistent".into(),
            ));
        }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditDecision {
    Allowed,
    Denied,
    QuotaExceeded,
    Error,
}
string_enum!(AuditDecision{Allowed=>"ALLOWED",Denied=>"DENIED",QuotaExceeded=>"QUOTA_EXCEEDED",Error=>"ERROR"});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: Uuid,
    pub request_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub subject: String,
    pub action: String,
    pub resource: String,
    pub decision: AuditDecision,
    pub reason: String,
    pub policy_id: Option<Uuid>,
    pub rule_id: Option<Uuid>,
    pub quota_id: Option<Uuid>,
    pub quota_key: Option<String>,
    pub units: u64,
    pub attributes: BTreeMap<String, String>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}
impl AuditRecord {
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("audit subject", &self.subject)?;
        validate_key("audit action", &self.action)?;
        validate_key("audit resource", &self.resource)?;
        validate_text("audit reason", &self.reason, 1024)?;
        if let Some(key) = &self.quota_key {
            validate_key("audit quota key", key)?;
        }
        if self.attributes.len() > MAX_ITEMS {
            return Err(PlatformError::Validation(
                "audit attributes exceed 256".into(),
            ));
        }
        for (k, v) in &self.attributes {
            validate_key("audit attribute", k)?;
            validate_text("audit attribute value", v, 256)?;
        }
        validate_actor(&self.actor)
    }
}

#[derive(Debug, Clone)]
pub struct GovernanceRequest {
    pub request_id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub subject: String,
    pub action: String,
    pub resource: String,
    pub attributes: BTreeMap<String, String>,
    pub quota_key: Option<String>,
    pub units: u64,
    pub actor: String,
}
impl GovernanceRequest {
    pub fn new(
        tenant_id: Uuid,
        subject: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            subject: subject.into(),
            action: action.into(),
            resource: resource.into(),
            attributes: BTreeMap::new(),
            quota_key: None,
            units: 0,
            actor: actor.into(),
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("governance subject", &self.subject)?;
        validate_key("governance action", &self.action)?;
        validate_key("governance resource", &self.resource)?;
        validate_actor(&self.actor)?;
        if self.attributes.len() > MAX_ITEMS {
            return Err(PlatformError::Validation(
                "governance attributes exceed 256".into(),
            ));
        }
        for (k, v) in &self.attributes {
            validate_key("attribute key", k)?;
            validate_text("attribute value", v, 256)?;
        }
        if self.quota_key.is_some() != (self.units > 0) {
            return Err(PlatformError::Validation(
                "quota key and units must be provided together".into(),
            ));
        }
        if let Some(k) = &self.quota_key {
            validate_key("quota key", k)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceDecision {
    pub request_id: Uuid,
    pub allowed: bool,
    pub policy_id: Option<Uuid>,
    pub rule_id: Option<Uuid>,
    pub quota_id: Option<Uuid>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformState {
    Created,
    Running,
    Stopped,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthStatus {
    pub component: String,
    pub healthy: bool,
    pub message: String,
    pub checked_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MetricPoint {
    pub name: String,
    pub value: f64,
    pub labels: BTreeMap<String, String>,
    pub recorded_at: DateTime<Utc>,
}

pub(crate) fn validate_actor(v: &str) -> PlatformResult<()> {
    validate_key("actor", v)
}
fn matches_set(values: &BTreeSet<String>, candidate: &str) -> bool {
    values.contains("*") || values.contains(candidate)
}
fn validate_key(label: &str, v: &str) -> PlatformResult<()> {
    if v.is_empty()
        || v.len() > 128
        || !v
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'/'))
    {
        return Err(PlatformError::Validation(format!(
            "{label} must be a safe bounded identifier"
        )));
    }
    Ok(())
}
fn validate_text(label: &str, v: &str, max: usize) -> PlatformResult<()> {
    if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(PlatformError::Validation(format!(
            "{label} must contain safe text"
        )));
    }
    Ok(())
}
fn validate_entity(
    version: u64,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
    actor: &str,
) -> PlatformResult<()> {
    validate_actor(actor)?;
    if version == 0 || updated < created {
        return Err(PlatformError::Validation(
            "entity version or timestamps invalid".into(),
        ));
    }
    Ok(())
}
fn validate_metadata(v: &PlatformMetadata) -> PlatformResult<()> {
    if v.len() > MAX_ITEMS {
        return Err(PlatformError::Validation("metadata exceeds 256".into()));
    }
    let value = serde_json::to_value(v)?;
    reject_sensitive(&value, 0)?;
    if serde_json::to_vec(&value)?.len() > MAX_JSON_BYTES {
        return Err(PlatformError::Validation("metadata too large".into()));
    }
    Ok(())
}
fn reject_sensitive(v: &Value, depth: usize) -> PlatformResult<()> {
    if depth > 32 {
        return Err(PlatformError::Validation(
            "metadata nesting too deep".into(),
        ));
    }
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                let k = k.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    k.as_str(),
                    "password" | "secret" | "api_key" | "access_token" | "refresh_token"
                ) || k.ends_with("_secret")
                    || k.ends_with("_password")
                {
                    return Err(PlatformError::Validation("sensitive metadata key".into()));
                }
                reject_sensitive(v, depth + 1)?
            }
        }
        Value::Array(a) => {
            for v in a {
                reject_sensitive(v, depth + 1)?
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_matching_is_explicit() {
        let rule = PolicyRule {
            id: Uuid::new_v4(),
            subjects: ["user".into()].into_iter().collect(),
            actions: ["tool.execute".into()].into_iter().collect(),
            resources: ["git".into()].into_iter().collect(),
            attributes: BTreeMap::new(),
            effect: PolicyEffect::Allow,
            priority: 1,
        };
        let r = GovernanceRequest::new(Uuid::new_v4(), "user", "tool.execute", "git", "user");
        assert!(rule.matches(&r));
    }
    #[test]
    fn quota_rejects_forged_ledger() {
        let mut q = Quota::new(Uuid::new_v4(), "execution", 10, 60, "system").unwrap();
        q.consumed = 2;
        assert!(q.validate().is_err());
    }
    #[test]
    fn metadata_rejects_secrets() {
        let mut t = Tenant::new("t", "Tenant", "system");
        t.metadata
            .insert("nested".into(), serde_json::json!({"api_key":"x"}));
        assert!(t.validate().is_err());
    }
}
