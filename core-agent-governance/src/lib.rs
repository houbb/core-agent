use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use core_agent_platform::{GovernanceRequest, PlatformManager};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub type EnterpriseResult<T> = Result<T, EnterpriseError>;

#[derive(Debug, thiserror::Error)]
pub enum EnterpriseError {
    #[error("enterprise validation failed: {0}")]
    Validation(String),
    #[error("enterprise resource not found: {0}")]
    NotFound(String),
    #[error("enterprise state conflict: {0}")]
    Conflict(String),
    #[error("enterprise authorization denied: {0}")]
    Denied(String),
    #[error("Platform governance failed: {0}")]
    Platform(String),
    #[error("enterprise internal error: {0}")]
    Internal(String),
}

// ─── Identity ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityProviderKind {
    Oidc,
    Saml,
    Ldap,
    OAuth,
    LocalAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrincipalState {
    Active,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnterprisePrincipal {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub external_subject: String,
    pub provider: IdentityProviderKind,
    pub display_name: String,
    pub organization_ids: BTreeSet<Uuid>,
    pub roles: BTreeSet<String>,
    pub groups: BTreeSet<String>,
    pub state: PrincipalState,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EnterprisePrincipal {
    pub fn new(
        tenant_id: Uuid,
        external_subject: impl Into<String>,
        provider: IdentityProviderKind,
        display_name: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            external_subject: external_subject.into(),
            provider,
            display_name: display_name.into(),
            organization_ids: BTreeSet::new(),
            roles: BTreeSet::new(),
            groups: BTreeSet::new(),
            state: PrincipalState::Active,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("external subject", &self.external_subject)?;
        validate_text("display name", &self.display_name, 256)?;
        validate_actor(&self.actor)?;
        if self.version == 0
            || self.updated_at < self.created_at
            || self.roles.len() > 256
            || self.groups.len() > 256
            || self.organization_ids.len() > 256
        {
            return Err(EnterpriseError::Validation(
                "principal bounds are invalid".into(),
            ));
        }
        for role in &self.roles {
            validate_key("role", role)?;
        }
        for group in &self.groups {
            validate_key("group", group)?;
        }
        Ok(())
    }
}

// ─── RBAC ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub description: String,
    pub permissions: BTreeSet<String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Role {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, key: key.into(), name: name.into(),
            description: String::new(), permissions: BTreeSet::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("role key", &self.key)?;
        validate_text("role name", &self.name, 256)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("role version or timestamps are invalid".into()));
        }
        for p in &self.permissions { validate_key("role permission", p)?; }
        Ok(())
    }
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains("*") || self.permissions.contains(permission)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Permission {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub action: String,
    pub resource: String,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Permission {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, action: impl Into<String>, resource: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, key: key.into(), name: name.into(),
            action: action.into(), resource: resource.into(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("permission key", &self.key)?;
        validate_text("permission name", &self.name, 256)?;
        validate_key("permission action", &self.action)?;
        validate_key("permission resource", &self.resource)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("permission version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleBinding {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub principal_id: Uuid,
    pub role_id: Uuid,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl RoleBinding {
    pub fn new(tenant_id: Uuid, principal_id: Uuid, role_id: Uuid, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, principal_id, role_id,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("role binding version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

// ─── Secret Management ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Secret {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub encrypted_value: Vec<u8>,
    pub owner: String,
    pub algorithm: String,
    pub rotation_days: u32,
    pub last_rotated_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Secret {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, encrypted_value: Vec<u8>, owner: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, key: key.into(), name: name.into(),
            encrypted_value, owner: owner.into(), algorithm: "AES-256-GCM".into(),
            rotation_days: 90, last_rotated_at: None, expires_at: None,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("secret key", &self.key)?;
        validate_text("secret name", &self.name, 256)?;
        validate_actor(&self.owner)?;
        validate_actor(&self.actor)?;
        if self.encrypted_value.is_empty() {
            return Err(EnterpriseError::Validation("secret value is empty".into()));
        }
        if self.rotation_days == 0 || self.rotation_days > 3650 {
            return Err(EnterpriseError::Validation("secret rotation days must be 1..=3650".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("secret version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

// ─── Agent Identity ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentIdentityCredential {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub tenant_id: Uuid,
    pub public_key: String,
    pub key_algorithm: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl AgentIdentityCredential {
    pub fn new(agent_id: Uuid, tenant_id: Uuid, public_key: impl Into<String>, key_algorithm: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), agent_id, tenant_id, public_key: public_key.into(),
            key_algorithm: key_algorithm.into(), issued_at: now, expires_at: None,
            revoked: false, version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_actor(&self.actor)?;
        if self.public_key.is_empty() || self.key_algorithm.is_empty() {
            return Err(EnterpriseError::Validation("agent identity credential has empty fields".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("credential version or timestamps are invalid".into()));
        }
        Ok(())
    }
    pub fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |exp| Utc::now() > exp)
    }
    pub fn is_valid(&self) -> bool {
        !self.revoked && !self.is_expired()
    }
}

// ─── Resource Security ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceProtection {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub resource_type: String,
    pub resource_pattern: String,
    pub required_permissions: BTreeSet<String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ResourceProtection {
    pub fn new(tenant_id: Uuid, resource_type: impl Into<String>, resource_pattern: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, resource_type: resource_type.into(),
            resource_pattern: resource_pattern.into(), required_permissions: BTreeSet::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("resource protection type", &self.resource_type)?;
        validate_actor(&self.actor)?;
        if self.required_permissions.is_empty() {
            return Err(EnterpriseError::Validation("resource protection must have required permissions".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("resource protection version or timestamps are invalid".into()));
        }
        Ok(())
    }
    pub fn matches(&self, resource_type: &str, resource_id: &str) -> bool {
        if self.resource_type != resource_type { return false; }
        self.resource_pattern == "*" || self.resource_pattern == resource_id
            || (resource_id.starts_with(&self.resource_pattern.replace('*', "")) && self.resource_pattern.contains('*'))
    }
    pub fn is_action_allowed(&self, permissions: &BTreeSet<String>, action: &str) -> bool {
        self.required_permissions.contains("*") || permissions.iter().any(|p| {
            p == action || (p.ends_with('*') && action.starts_with(&p[..p.len() - 1]))
        })
    }
}

// ─── Asset Governance (existing) ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AiAssetType {
    Agent,
    Model,
    Prompt,
    Workflow,
    Knowledge,
    Policy,
    Capability,
}
impl AiAssetType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Model => "model",
            Self::Prompt => "prompt",
            Self::Workflow => "workflow",
            Self::Knowledge => "knowledge",
            Self::Policy => "policy",
            Self::Capability => "capability",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetEnvironment {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GovernanceAssetState {
    Draft,
    Reviewed,
    Approved,
    Production,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetApproval {
    pub id: Uuid,
    pub principal_id: Uuid,
    pub subject: String,
    pub comment: String,
    pub approved_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceAsset {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub asset_type: AiAssetType,
    pub key: String,
    pub name: String,
    pub asset_version: String,
    pub owner_subject: String,
    pub classification: DataClassification,
    pub environment: AssetEnvironment,
    pub state: GovernanceAssetState,
    pub risk_score: u8,
    pub required_approvals: u8,
    pub approvals: Vec<AssetApproval>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl GovernanceAsset {
    pub fn new(
        tenant_id: Uuid,
        asset_type: AiAssetType,
        key: impl Into<String>,
        name: impl Into<String>,
        asset_version: impl Into<String>,
        owner: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let owner = owner.into();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            asset_type,
            key: key.into(),
            name: name.into(),
            asset_version: asset_version.into(),
            owner_subject: owner.clone(),
            classification: DataClassification::Internal,
            environment: AssetEnvironment::Development,
            state: GovernanceAssetState::Draft,
            risk_score: 0,
            required_approvals: 1,
            approvals: Vec::new(),
            version: 1,
            actor: owner,
            created_at: now,
            updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("asset key", &self.key)?;
        validate_text("asset name", &self.name, 256)?;
        validate_key("asset version", &self.asset_version)?;
        validate_actor(&self.owner_subject)?;
        validate_actor(&self.actor)?;
        if self.risk_score > 100
            || !(1..=8).contains(&self.required_approvals)
            || self.approvals.len() > 8
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(EnterpriseError::Validation(
                "asset bounds are invalid".into(),
            ));
        }
        let mut principals = BTreeSet::new();
        for approval in &self.approvals {
            validate_actor(&approval.subject)?;
            validate_text("approval comment", &approval.comment, 2048)?;
            if !principals.insert(approval.principal_id) {
                return Err(EnterpriseError::Validation(
                    "duplicate asset approval".into(),
                ));
            }
        }
        if matches!(
            self.state,
            GovernanceAssetState::Approved | GovernanceAssetState::Production
        ) && self.approvals.len() < usize::from(self.required_approvals)
        {
            return Err(EnterpriseError::Validation(
                "asset lacks required approvals".into(),
            ));
        }
        Ok(())
    }
}

// ─── Cost Record (existing) ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub event_key: String,
    pub project_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub model_key: Option<String>,
    pub currency: String,
    pub amount_micros: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
}

impl CostRecord {
    pub fn new(
        tenant_id: Uuid,
        event_key: impl Into<String>,
        currency: impl Into<String>,
        amount_micros: u64,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            event_key: event_key.into(),
            project_id: None,
            agent_id: None,
            model_key: None,
            currency: currency.into(),
            amount_micros,
            input_tokens: 0,
            output_tokens: 0,
            actor: actor.into(),
            occurred_at: Utc::now(),
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("cost event key", &self.event_key)?;
        validate_actor(&self.actor)?;
        if self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || (self.amount_micros == 0 && self.input_tokens == 0 && self.output_tokens == 0)
        {
            return Err(EnterpriseError::Validation(
                "cost currency or usage is invalid".into(),
            ));
        }
        if let Some(model) = &self.model_key {
            validate_key("model key", model)?;
        }
        Ok(())
    }
}

// ─── Evidence Chain ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceChain {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub audit_event_id: Uuid,
    pub approval_request_id: Option<Uuid>,
    pub governance_request_id: Option<Uuid>,
    pub compliance_record_id: Option<Uuid>,
    pub chain_hash: String,
    pub previous_chain_id: Option<Uuid>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl EvidenceChain {
    pub fn new(tenant_id: Uuid, audit_event_id: Uuid, actor: impl Into<String>, previous: Option<Uuid>) -> Self {
        let now = Utc::now();
        let mut chain = Self {
            id: Uuid::new_v4(), tenant_id, audit_event_id,
            approval_request_id: None, governance_request_id: None, compliance_record_id: None,
            chain_hash: String::new(), previous_chain_id: previous,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        };
        chain.chain_hash = chain.compute_hash();
        chain
    }
    fn compute_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.id.as_bytes());
        hasher.update(self.tenant_id.as_bytes());
        hasher.update(self.audit_event_id.as_bytes());
        if let Some(prev) = self.previous_chain_id {
            hasher.update(prev.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_actor(&self.actor)?;
        if self.chain_hash.is_empty() {
            return Err(EnterpriseError::Validation("evidence chain hash is empty".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("evidence chain version or timestamps are invalid".into()));
        }
        Ok(())
    }
    pub fn verify_chain(&self) -> bool {
        self.chain_hash == self.compute_hash()
    }
}

// ─── Compliance ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComplianceStatus {
    Compliant,
    NonCompliant,
    NotEvaluated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComplianceStandard {
    Iso27001,
    Soc2,
    Gdpr,
    Hipaa,
    PciDss,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub resource_type: String,
    pub resource_id: String,
    pub rule_id: Option<Uuid>,
    pub rule_name: String,
    pub standard: ComplianceStandard,
    pub status: ComplianceStatus,
    pub evidence_ids: BTreeSet<Uuid>,
    pub evaluated_at: DateTime<Utc>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ComplianceRecord {
    pub fn new(tenant_id: Uuid, resource_type: impl Into<String>, resource_id: impl Into<String>, standard: ComplianceStandard, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, resource_type: resource_type.into(),
            resource_id: resource_id.into(), rule_id: None, rule_name: String::new(),
            standard, status: ComplianceStatus::NotEvaluated, evidence_ids: BTreeSet::new(),
            evaluated_at: now, version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("compliance resource type", &self.resource_type)?;
        validate_key("compliance resource id", &self.resource_id)?;
        validate_text("compliance rule name", &self.rule_name, 256)?;
        validate_actor(&self.actor)?;
        if self.evidence_ids.len() > 256 {
            return Err(EnterpriseError::Validation("compliance evidence ids exceed 256".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("compliance record version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComplianceMapping {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub standard: ComplianceStandard,
    pub control_id: String,
    pub policy_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ComplianceMapping {
    pub fn new(tenant_id: Uuid, standard: ComplianceStandard, control_id: impl Into<String>, policy_id: Uuid, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, standard, control_id: control_id.into(),
            policy_id, rule_id: None,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("compliance mapping control id", &self.control_id)?;
        validate_actor(&self.actor)?;
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("compliance mapping version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplianceSnapshot {
    pub tenant_id: Uuid,
    pub total_resources: usize,
    pub compliant: usize,
    pub non_compliant: usize,
    pub not_evaluated: usize,
    pub by_standard: BTreeMap<String, ComplianceStatusCounts>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplianceStatusCounts {
    pub total: usize,
    pub compliant: usize,
    pub non_compliant: usize,
}

// ─── Model Governance ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelGovernanceRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub agent_id: Uuid,
    pub model_key: String,
    pub model_version: String,
    pub prompt_hash: String,
    pub output_hash: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ModelGovernanceRecord {
    pub fn new(tenant_id: Uuid, agent_id: Uuid, model_key: impl Into<String>, model_version: impl Into<String>, prompt: &str, output: &str, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        let prompt_hash = format!("{:x}", Sha256::digest(prompt.as_bytes()));
        let output_hash = format!("{:x}", Sha256::digest(output.as_bytes()));
        Self {
            id: Uuid::new_v4(), tenant_id, agent_id, model_key: model_key.into(),
            model_version: model_version.into(), prompt_hash, output_hash,
            input_tokens: 0, output_tokens: 0, started_at: now, completed_at: now,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("model governance model key", &self.model_key)?;
        validate_key("model governance model version", &self.model_version)?;
        validate_actor(&self.actor)?;
        if self.prompt_hash.is_empty() || self.output_hash.is_empty() {
            return Err(EnterpriseError::Validation("model governance has empty hash".into()));
        }
        if self.completed_at < self.started_at {
            return Err(EnterpriseError::Validation("model governance completed_at < started_at".into()));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("model governance record version or timestamps are invalid".into()));
        }
        Ok(())
    }
}

// ─── Risk Assessment ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskDimension {
    DataAccess,
    ToolAccess,
    NetworkAccess,
    ModelAccess,
    CostImpact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRiskAssessment {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub agent_id: Uuid,
    pub risk_score: u8,
    pub risk_level: RiskLevel,
    pub dimensions: BTreeMap<RiskDimension, u8>,
    pub assessed_at: DateTime<Utc>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl AgentRiskAssessment {
    pub fn new(tenant_id: Uuid, agent_id: Uuid, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, agent_id,
            risk_score: 0, risk_level: RiskLevel::Low,
            dimensions: BTreeMap::new(), assessed_at: now,
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_actor(&self.actor)?;
        if self.risk_score > 100 {
            return Err(EnterpriseError::Validation("risk score must be 0..=100".into()));
        }
        if self.dimensions.len() > 10 {
            return Err(EnterpriseError::Validation("risk dimensions exceed 10".into()));
        }
        for (_, score) in &self.dimensions {
            if *score > 100 {
                return Err(EnterpriseError::Validation("risk dimension score must be 0..=100".into()));
            }
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("risk assessment version or timestamps are invalid".into()));
        }
        Ok(())
    }
    pub fn compute_risk_level(score: u8) -> RiskLevel {
        if score >= 80 { RiskLevel::Critical }
        else if score >= 60 { RiskLevel::High }
        else if score >= 30 { RiskLevel::Medium }
        else { RiskLevel::Low }
    }
}

// ─── Agent Ownership ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentOwnership {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub agent_id: Uuid,
    pub agent_key: String,
    pub owner_subject: String,
    pub owner_principal_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub department_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub authorized_users: BTreeSet<Uuid>,
    pub authorized_roles: BTreeSet<String>,
    pub allow_self_serve: bool,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl AgentOwnership {
    pub fn new(tenant_id: Uuid, agent_id: Uuid, agent_key: impl Into<String>, owner_subject: impl Into<String>, owner_principal_id: Uuid) -> Self {
        let now = Utc::now();
        let owner = owner_subject.into();
        Self {
            id: Uuid::new_v4(), tenant_id, agent_id,
            agent_key: agent_key.into(),
            owner_subject: owner.clone(),
            owner_principal_id,
            organization_id: None, department_id: None, team_id: None,
            authorized_users: BTreeSet::new(), authorized_roles: BTreeSet::new(),
            allow_self_serve: true,
            version: 1, actor: owner, created_at: now, updated_at: now,
        }
    }
    fn validate(&self) -> EnterpriseResult<()> {
        validate_key("agent ownership agent key", &self.agent_key)?;
        validate_actor(&self.owner_subject)?;
        validate_actor(&self.actor)?;
        if self.authorized_users.len() > 256 || self.authorized_roles.len() > 256 {
            return Err(EnterpriseError::Validation("agent ownership sets exceed 256".into()));
        }
        for role in &self.authorized_roles {
            validate_key("agent ownership authorized role", role)?;
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(EnterpriseError::Validation("agent ownership version or timestamps are invalid".into()));
        }
        Ok(())
    }
    pub fn is_authorized(&self, principal_id: &Uuid, roles: &BTreeSet<String>) -> bool {
        if self.allow_self_serve && self.owner_principal_id == *principal_id { return true; }
        if self.authorized_users.contains(principal_id) { return true; }
        self.authorized_roles.iter().any(|role| roles.contains(role))
    }
}

// ─── Compliance Dashboard ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComplianceDashboard {
    pub tenant_id: Uuid,
    pub total_agents: usize,
    pub total_policies: usize,
    pub total_audit_events: u64,
    pub total_compliance_records: usize,
    pub compliant_count: usize,
    pub non_compliant_count: usize,
    pub by_standard: BTreeMap<String, ComplianceStatusCounts>,
    pub high_risk_agents: usize,
    pub open_approval_requests: usize,
    pub cost_by_currency: BTreeMap<String, u64>,
    pub last_audit_at: Option<DateTime<Utc>>,
}

// ─── Governance Snapshot (existing, extended) ─────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceSnapshot {
    pub tenant_id: Uuid,
    pub principals: usize,
    pub assets: usize,
    pub production_assets: usize,
    pub high_risk_assets: usize,
    pub cost_micros_by_currency: BTreeMap<String, u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub roles: usize,
    pub secrets: usize,
    pub compliance_records: usize,
    pub model_governance_records: usize,
    pub risk_assessments: usize,
    pub agent_ownerships: usize,
}

// ─── State ────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct State {
    principals: BTreeMap<Uuid, EnterprisePrincipal>,
    assets: BTreeMap<Uuid, GovernanceAsset>,
    costs: BTreeMap<String, CostRecord>,
    roles: BTreeMap<Uuid, Role>,
    permissions: BTreeMap<Uuid, Permission>,
    role_bindings: BTreeMap<Uuid, RoleBinding>,
    secrets: BTreeMap<Uuid, Secret>,
    agent_credentials: BTreeMap<Uuid, AgentIdentityCredential>,
    resource_protections: BTreeMap<Uuid, ResourceProtection>,
    evidence_chains: BTreeMap<Uuid, EvidenceChain>,
    compliance_records: BTreeMap<Uuid, ComplianceRecord>,
    compliance_mappings: BTreeMap<Uuid, ComplianceMapping>,
    model_governance_records: BTreeMap<Uuid, ModelGovernanceRecord>,
    risk_assessments: BTreeMap<Uuid, AgentRiskAssessment>,
    agent_ownerships: BTreeMap<Uuid, AgentOwnership>,
}

// ─── EnterpriseGovernanceManager ──────────────────────────────────────────

pub struct EnterpriseGovernanceManager {
    platform: Arc<PlatformManager>,
    state: RwLock<State>,
}

impl EnterpriseGovernanceManager {
    pub fn new(platform: Arc<PlatformManager>) -> Self {
        Self {
            platform,
            state: RwLock::new(State::default()),
        }
    }

    // ─── Principal ──────────────────────────────────────────────────────

    pub async fn bind_principal(
        &self,
        value: EnterprisePrincipal,
    ) -> EnterpriseResult<EnterprisePrincipal> {
        value.validate()?;
        self.authorize(value.tenant_id, &value.actor, "enterprise.identity.bind", "identity").await?;
        let mut state = self.write()?;
        if state.principals.values().any(|item| {
            item.tenant_id == value.tenant_id && item.external_subject == value.external_subject
        }) {
            return Err(EnterpriseError::Conflict("external subject already bound".into()));
        }
        state.principals.insert(value.id, value.clone());
        Ok(value)
    }

    // ─── RBAC ───────────────────────────────────────────────────────────

    pub fn create_role(&self, value: Role) -> EnterpriseResult<Role> {
        value.validate()?;
        let mut state = self.write()?;
        if state.roles.values().any(|r| r.tenant_id == value.tenant_id && r.key == value.key) {
            return Err(EnterpriseError::Conflict("role key exists".into()));
        }
        state.roles.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn create_permission(&self, value: Permission) -> EnterpriseResult<Permission> {
        value.validate()?;
        let mut state = self.write()?;
        if state.permissions.values().any(|p| p.tenant_id == value.tenant_id && p.key == value.key) {
            return Err(EnterpriseError::Conflict("permission key exists".into()));
        }
        state.permissions.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn bind_role(&self, value: RoleBinding) -> EnterpriseResult<RoleBinding> {
        value.validate()?;
        let state = self.read()?;
        if !state.principals.contains_key(&value.principal_id) {
            return Err(EnterpriseError::NotFound("principal not found".into()));
        }
        if !state.roles.contains_key(&value.role_id) {
            return Err(EnterpriseError::NotFound("role not found".into()));
        }
        drop(state);
        let mut state = self.write()?;
        state.role_bindings.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn unbind_role(&self, id: Uuid, actor: &str) -> EnterpriseResult<()> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let binding = state.role_bindings.remove(&id)
            .ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        let _ = binding;
        Ok(())
    }

    pub fn roles(&self, tenant: Uuid) -> EnterpriseResult<Vec<Role>> {
        Ok(self.read()?.roles.values().filter(|r| r.tenant_id == tenant).cloned().collect())
    }

    pub fn permissions(&self, tenant: Uuid) -> EnterpriseResult<Vec<Permission>> {
        Ok(self.read()?.permissions.values().filter(|p| p.tenant_id == tenant).cloned().collect())
    }

    pub fn role_bindings(&self, tenant: Uuid) -> EnterpriseResult<Vec<RoleBinding>> {
        Ok(self.read()?.role_bindings.values().filter(|b| b.tenant_id == tenant).cloned().collect())
    }

    // ─── Secret Management ──────────────────────────────────────────────

    pub fn store_secret(&self, value: Secret) -> EnterpriseResult<Secret> {
        value.validate()?;
        let mut state = self.write()?;
        if state.secrets.values().any(|s| s.tenant_id == value.tenant_id && s.key == value.key) {
            return Err(EnterpriseError::Conflict("secret key exists".into()));
        }
        state.secrets.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn read_secret(&self, tenant_id: Uuid, key: &str, actor: &str) -> EnterpriseResult<Secret> {
        validate_actor(actor)?;
        let state = self.read()?;
        state.secrets.values()
            .find(|s| s.tenant_id == tenant_id && s.key == key)
            .cloned()
            .ok_or_else(|| EnterpriseError::NotFound(format!("secret {key}")))
    }

    pub fn rotate_secret(&self, tenant_id: Uuid, key: &str, new_encrypted: Vec<u8>, actor: &str) -> EnterpriseResult<Secret> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let secret = state.secrets.values_mut()
            .find(|s| s.tenant_id == tenant_id && s.key == key)
            .ok_or_else(|| EnterpriseError::NotFound(format!("secret {key}")))?;
        secret.encrypted_value = new_encrypted;
        secret.last_rotated_at = Some(Utc::now());
        secret.version = secret.version.saturating_add(1);
        secret.actor = actor.into();
        secret.updated_at = Utc::now();
        Ok(secret.clone())
    }

    pub fn list_secrets(&self, tenant: Uuid) -> EnterpriseResult<Vec<Secret>> {
        Ok(self.read()?.secrets.values().filter(|s| s.tenant_id == tenant).cloned().collect())
    }

    pub fn delete_secret(&self, tenant_id: Uuid, key: &str, actor: &str) -> EnterpriseResult<()> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let pos = state.secrets.values().position(|s| s.tenant_id == tenant_id && s.key == key)
            .ok_or_else(|| EnterpriseError::NotFound(format!("secret {key}")))?;
        let id = state.secrets.keys().nth(pos).copied().unwrap();
        state.secrets.remove(&id);
        Ok(())
    }

    // ─── Agent Identity ─────────────────────────────────────────────────

    pub fn issue_agent_credential(&self, value: AgentIdentityCredential) -> EnterpriseResult<AgentIdentityCredential> {
        value.validate()?;
        let mut state = self.write()?;
        state.agent_credentials.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn verify_agent_identity(&self, agent_id: Uuid, credential_id: Uuid) -> EnterpriseResult<bool> {
        let state = self.read()?;
        let cred = state.agent_credentials.get(&credential_id)
            .ok_or_else(|| EnterpriseError::NotFound(credential_id.to_string()))?;
        Ok(cred.agent_id == agent_id && cred.is_valid())
    }

    pub fn revoke_agent_credential(&self, credential_id: Uuid, actor: &str) -> EnterpriseResult<()> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let cred = state.agent_credentials.get_mut(&credential_id)
            .ok_or_else(|| EnterpriseError::NotFound(credential_id.to_string()))?;
        cred.revoked = true;
        cred.updated_at = Utc::now();
        Ok(())
    }

    // ─── Resource Security ──────────────────────────────────────────────

    pub fn define_resource_protection(&self, value: ResourceProtection) -> EnterpriseResult<ResourceProtection> {
        value.validate()?;
        let mut state = self.write()?;
        state.resource_protections.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn check_resource_access(&self, tenant_id: Uuid, resource_type: &str, resource_id: &str, action: &str, principal_permissions: &BTreeSet<String>) -> EnterpriseResult<bool> {
        let state = self.read()?;
        let protections: Vec<_> = state.resource_protections.values()
            .filter(|p| p.tenant_id == tenant_id && p.matches(resource_type, resource_id))
            .collect();
        if protections.is_empty() {
            return Ok(true); // No protection defined = allowed
        }
        Ok(protections.iter().all(|p| p.is_action_allowed(principal_permissions, action)))
    }

    // ─── Asset Governance (existing) ────────────────────────────────────

    pub async fn register_asset(&self, value: GovernanceAsset) -> EnterpriseResult<GovernanceAsset> {
        value.validate()?;
        self.authorize(value.tenant_id, &value.actor, "enterprise.asset.register", value.asset_type.as_str()).await?;
        let state = self.read()?;
        require_principal(&state, value.tenant_id, &value.actor)?;
        if state.assets.values().any(|item| {
            item.tenant_id == value.tenant_id && item.asset_type == value.asset_type && item.key == value.key && item.asset_version == value.asset_version
        }) {
            return Err(EnterpriseError::Conflict("asset version already registered".into()));
        }
        drop(state);
        self.write()?.assets.insert(value.id, value.clone());
        Ok(value)
    }

    pub async fn submit_asset(&self, id: Uuid, actor: &str) -> EnterpriseResult<GovernanceAsset> {
        let current = self.required_asset(id)?;
        self.authorize(current.tenant_id, actor, "enterprise.asset.review", current.asset_type.as_str()).await?;
        let mut state = self.write()?;
        require_principal(&state, current.tenant_id, actor)?;
        let value = state.assets.get_mut(&id).ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if value.state != GovernanceAssetState::Draft {
            return Err(EnterpriseError::Conflict("only Draft assets can enter review".into()));
        }
        value.state = GovernanceAssetState::Reviewed;
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }

    pub async fn approve_asset(&self, id: Uuid, comment: &str, actor: &str) -> EnterpriseResult<GovernanceAsset> {
        validate_text("approval comment", comment, 2048)?;
        let current = self.required_asset(id)?;
        self.authorize(current.tenant_id, actor, "enterprise.asset.approve", current.asset_type.as_str()).await?;
        let mut state = self.write()?;
        let principal = require_principal(&state, current.tenant_id, actor)?.clone();
        let value = state.assets.get_mut(&id).ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if value.state != GovernanceAssetState::Reviewed {
            return Err(EnterpriseError::Conflict("asset is not awaiting approval".into()));
        }
        if value.owner_subject == actor {
            return Err(EnterpriseError::Denied("asset owner cannot self-approve".into()));
        }
        if value.approvals.iter().any(|item| item.principal_id == principal.id) {
            return Err(EnterpriseError::Conflict("principal already approved this asset".into()));
        }
        value.approvals.push(AssetApproval {
            id: Uuid::new_v4(), principal_id: principal.id, subject: actor.into(), comment: comment.into(), approved_at: Utc::now(),
        });
        if value.approvals.len() >= usize::from(value.required_approvals) {
            value.state = GovernanceAssetState::Approved;
        }
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }

    pub async fn transition_asset(&self, id: Uuid, target: GovernanceAssetState, actor: &str) -> EnterpriseResult<GovernanceAsset> {
        let current = self.required_asset(id)?;
        self.authorize(current.tenant_id, actor, "enterprise.asset.transition", current.asset_type.as_str()).await?;
        let mut state = self.write()?;
        require_principal(&state, current.tenant_id, actor)?;
        let value = state.assets.get_mut(&id).ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if !valid_asset_transition(value.state, target) {
            return Err(EnterpriseError::Conflict("invalid governed asset transition".into()));
        }
        value.state = target;
        if target == GovernanceAssetState::Production {
            value.environment = AssetEnvironment::Production;
        }
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }

    // ─── Cost Record (existing) ─────────────────────────────────────────

    pub async fn record_cost(&self, value: CostRecord) -> EnterpriseResult<CostRecord> {
        value.validate()?;
        self.authorize(value.tenant_id, &value.actor, "enterprise.cost.record", "cost").await?;
        let mut state = self.write()?;
        require_principal(&state, value.tenant_id, &value.actor)?;
        if state.costs.contains_key(&value.event_key) {
            return Err(EnterpriseError::Conflict("cost event already recorded".into()));
        }
        state.costs.insert(value.event_key.clone(), value.clone());
        Ok(value)
    }

    // ─── Evidence Chain ─────────────────────────────────────────────────

    pub fn append_evidence(&self, value: EvidenceChain) -> EnterpriseResult<EvidenceChain> {
        value.validate()?;
        if !value.verify_chain() {
            return Err(EnterpriseError::Validation("evidence chain hash verification failed".into()));
        }
        let mut state = self.write()?;
        if let Some(prev_id) = value.previous_chain_id {
            if !state.evidence_chains.contains_key(&prev_id) {
                return Err(EnterpriseError::NotFound("previous evidence chain not found".into()));
            }
        }
        state.evidence_chains.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn list_evidence_chains(&self, tenant: Uuid) -> EnterpriseResult<Vec<EvidenceChain>> {
        Ok(self.read()?.evidence_chains.values().filter(|e| e.tenant_id == tenant).cloned().collect())
    }

    // ─── Compliance ─────────────────────────────────────────────────────

    pub fn create_compliance_record(&self, value: ComplianceRecord) -> EnterpriseResult<ComplianceRecord> {
        value.validate()?;
        let mut state = self.write()?;
        state.compliance_records.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn list_compliance_records(&self, tenant: Uuid, standard: Option<ComplianceStandard>) -> EnterpriseResult<Vec<ComplianceRecord>> {
        Ok(self.read()?.compliance_records.values()
            .filter(|r| r.tenant_id == tenant && standard.map_or(true, |s| r.standard == s))
            .cloned().collect())
    }

    pub fn compliance_snapshot(&self, tenant: Uuid) -> EnterpriseResult<ComplianceSnapshot> {
        let state = self.read()?;
        let records: Vec<_> = state.compliance_records.values().filter(|r| r.tenant_id == tenant).collect();
        let mut by_standard: BTreeMap<String, ComplianceStatusCounts> = BTreeMap::new();
        for r in &records {
            let standard_key = format!("{:?}", r.standard);
            let entry = by_standard.entry(standard_key).or_insert(ComplianceStatusCounts { total: 0, compliant: 0, non_compliant: 0 });
            entry.total += 1;
            match r.status {
                ComplianceStatus::Compliant => entry.compliant += 1,
                ComplianceStatus::NonCompliant => entry.non_compliant += 1,
                ComplianceStatus::NotEvaluated => {}
            }
        }
        Ok(ComplianceSnapshot {
            tenant_id: tenant,
            total_resources: records.len(),
            compliant: records.iter().filter(|r| r.status == ComplianceStatus::Compliant).count(),
            non_compliant: records.iter().filter(|r| r.status == ComplianceStatus::NonCompliant).count(),
            not_evaluated: records.iter().filter(|r| r.status == ComplianceStatus::NotEvaluated).count(),
            by_standard,
        })
    }

    pub fn create_compliance_mapping(&self, value: ComplianceMapping) -> EnterpriseResult<ComplianceMapping> {
        value.validate()?;
        let mut state = self.write()?;
        state.compliance_mappings.insert(value.id, value.clone());
        Ok(value)
    }

    // ─── Model Governance ───────────────────────────────────────────────

    pub fn record_model_use(&self, value: ModelGovernanceRecord) -> EnterpriseResult<ModelGovernanceRecord> {
        value.validate()?;
        let mut state = self.write()?;
        state.model_governance_records.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn query_model_usage(&self, tenant: Uuid, agent_id: Option<Uuid>) -> EnterpriseResult<Vec<ModelGovernanceRecord>> {
        Ok(self.read()?.model_governance_records.values()
            .filter(|r| r.tenant_id == tenant && agent_id.map_or(true, |a| r.agent_id == a))
            .cloned().collect())
    }

    // ─── Risk Assessment ────────────────────────────────────────────────

    pub fn assess_agent_risk(&self, value: AgentRiskAssessment) -> EnterpriseResult<AgentRiskAssessment> {
        value.validate()?;
        let mut state = self.write()?;
        state.risk_assessments.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn list_risk_assessments(&self, tenant: Uuid) -> EnterpriseResult<Vec<AgentRiskAssessment>> {
        Ok(self.read()?.risk_assessments.values().filter(|r| r.tenant_id == tenant).cloned().collect())
    }

    // ─── Query ──────────────────────────────────────────────────────────

    pub fn principals(&self, tenant: Uuid) -> EnterpriseResult<Vec<EnterprisePrincipal>> {
        Ok(self.read()?.principals.values().filter(|item| item.tenant_id == tenant).cloned().collect())
    }

    pub fn assets(&self, tenant: Uuid) -> EnterpriseResult<Vec<GovernanceAsset>> {
        Ok(self.read()?.assets.values().filter(|item| item.tenant_id == tenant).cloned().collect())
    }

    pub fn costs(&self, tenant: Uuid) -> EnterpriseResult<Vec<CostRecord>> {
        Ok(self.read()?.costs.values().filter(|item| item.tenant_id == tenant).cloned().collect())
    }

    pub fn snapshot(&self, tenant: Uuid) -> EnterpriseResult<GovernanceSnapshot> {
        let state = self.read()?;
        let assets: Vec<_> = state.assets.values().filter(|item| item.tenant_id == tenant).collect();
        let costs: Vec<_> = state.costs.values().filter(|item| item.tenant_id == tenant).collect();
        let mut by = BTreeMap::new();
        for cost in &costs {
            let total = by.entry(cost.currency.clone()).or_insert(0_u64);
            *total = total.saturating_add(cost.amount_micros);
        }
        Ok(GovernanceSnapshot {
            tenant_id: tenant,
            principals: state.principals.values().filter(|item| item.tenant_id == tenant).count(),
            assets: assets.len(),
            production_assets: assets.iter().filter(|item| item.state == GovernanceAssetState::Production).count(),
            high_risk_assets: assets.iter().filter(|item| item.risk_score >= 70).count(),
            cost_micros_by_currency: by,
            input_tokens: costs.iter().map(|item| item.input_tokens).sum(),
            output_tokens: costs.iter().map(|item| item.output_tokens).sum(),
            roles: state.roles.values().filter(|r| r.tenant_id == tenant).count(),
            secrets: state.secrets.values().filter(|s| s.tenant_id == tenant).count(),
            compliance_records: state.compliance_records.values().filter(|r| r.tenant_id == tenant).count(),
            model_governance_records: state.model_governance_records.values().filter(|r| r.tenant_id == tenant).count(),
            risk_assessments: state.risk_assessments.values().filter(|r| r.tenant_id == tenant).count(),
            agent_ownerships: state.agent_ownerships.values().filter(|o| o.tenant_id == tenant).count(),
        })
    }

    // ─── Internal ───────────────────────────────────────────────────────

    // ─── Agent Ownership ────────────────────────────────────────────────

    pub fn register_agent_ownership(&self, value: AgentOwnership) -> EnterpriseResult<AgentOwnership> {
        value.validate()?;
        let mut state = self.write()?;
        if state.agent_ownerships.values().any(|o| o.tenant_id == value.tenant_id && o.agent_id == value.agent_id) {
            return Err(EnterpriseError::Conflict("agent ownership already registered".into()));
        }
        state.agent_ownerships.insert(value.id, value.clone());
        Ok(value)
    }

    pub fn check_agent_access(&self, tenant_id: Uuid, agent_id: Uuid, principal_id: &Uuid, roles: &BTreeSet<String>) -> EnterpriseResult<bool> {
        let state = self.read()?;
        let ownership = state.agent_ownerships.values()
            .find(|o| o.tenant_id == tenant_id && o.agent_id == agent_id)
            .ok_or_else(|| EnterpriseError::NotFound("agent ownership not found".into()))?;
        Ok(ownership.is_authorized(principal_id, roles))
    }

    pub fn list_agent_ownerships(&self, tenant: Uuid) -> EnterpriseResult<Vec<AgentOwnership>> {
        Ok(self.read()?.agent_ownerships.values().filter(|o| o.tenant_id == tenant).cloned().collect())
    }

    // ─── Compliance Dashboard ───────────────────────────────────────────

    pub fn compliance_dashboard(&self, tenant: Uuid) -> EnterpriseResult<ComplianceDashboard> {
        let state = self.read()?;
        let records: Vec<_> = state.compliance_records.values().filter(|r| r.tenant_id == tenant).collect();
        let assessments: Vec<_> = state.risk_assessments.values().filter(|r| r.tenant_id == tenant).collect();
        let costs: Vec<_> = state.costs.values().filter(|c| c.tenant_id == tenant).collect();

        let mut by_standard = BTreeMap::new();
        for r in &records {
            let standard_key = format!("{:?}", r.standard);
            let entry = by_standard.entry(standard_key).or_insert(ComplianceStatusCounts { total: 0, compliant: 0, non_compliant: 0 });
            entry.total += 1;
            match r.status {
                ComplianceStatus::Compliant => entry.compliant += 1,
                ComplianceStatus::NonCompliant => entry.non_compliant += 1,
                ComplianceStatus::NotEvaluated => {}
            }
        }

        let mut cost_by_currency = BTreeMap::new();
        for cost in &costs {
            let total = cost_by_currency.entry(cost.currency.clone()).or_insert(0_u64);
            *total = total.saturating_add(cost.amount_micros);
        }

        Ok(ComplianceDashboard {
            tenant_id: tenant,
            total_agents: state.agent_ownerships.values().filter(|o| o.tenant_id == tenant).count(),
            total_policies: state.assets.values().filter(|a| a.tenant_id == tenant && a.asset_type == AiAssetType::Policy).count(),
            total_audit_events: 0, // Not tracked in-memory, would be from Platform audit
            total_compliance_records: records.len(),
            compliant_count: records.iter().filter(|r| r.status == ComplianceStatus::Compliant).count(),
            non_compliant_count: records.iter().filter(|r| r.status == ComplianceStatus::NonCompliant).count(),
            by_standard,
            high_risk_agents: assessments.iter().filter(|a| a.risk_level >= RiskLevel::High).count(),
            open_approval_requests: 0, // Not tracked in-memory, would be from ApprovalManager
            cost_by_currency,
            last_audit_at: None,
        })
    }

    async fn authorize(&self, tenant: Uuid, subject: &str, action: &str, resource: &str) -> EnterpriseResult<()> {
        let decision = self.platform.govern(GovernanceRequest::new(tenant, subject, action, resource, subject))
            .await.map_err(|error| EnterpriseError::Platform(error.to_string()))?;
        if !decision.allowed {
            return Err(EnterpriseError::Denied(decision.reason));
        }
        Ok(())
    }

    fn required_asset(&self, id: Uuid) -> EnterpriseResult<GovernanceAsset> {
        self.read()?.assets.get(&id).cloned().ok_or_else(|| EnterpriseError::NotFound(id.to_string()))
    }

    fn read(&self) -> EnterpriseResult<RwLockReadGuard<'_, State>> {
        self.state.read().map_err(|_| EnterpriseError::Internal("enterprise lock poisoned".into()))
    }

    fn write(&self) -> EnterpriseResult<RwLockWriteGuard<'_, State>> {
        self.state.write().map_err(|_| EnterpriseError::Internal("enterprise lock poisoned".into()))
    }
}

fn require_principal<'a>(state: &'a State, tenant: Uuid, subject: &str) -> EnterpriseResult<&'a EnterprisePrincipal> {
    let value = state.principals.values()
        .find(|item| item.tenant_id == tenant && item.external_subject == subject)
        .ok_or_else(|| EnterpriseError::Denied("actor has no enterprise identity binding".into()))?;
    if value.state != PrincipalState::Active {
        return Err(EnterpriseError::Denied("enterprise principal is suspended".into()));
    }
    Ok(value)
}

fn valid_asset_transition(from: GovernanceAssetState, to: GovernanceAssetState) -> bool {
    matches!(
        (from, to),
        (GovernanceAssetState::Approved, GovernanceAssetState::Production)
        | (GovernanceAssetState::Production, GovernanceAssetState::Suspended)
        | (GovernanceAssetState::Suspended, GovernanceAssetState::Production)
        | (_, GovernanceAssetState::Retired)
    )
}

fn advance_asset(value: &mut GovernanceAsset, actor: &str) {
    value.version += 1;
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at)
}

fn validate_actor(value: &str) -> EnterpriseResult<()> {
    validate_key("actor", value)
}

fn validate_key(label: &str, value: &str) -> EnterpriseResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(EnterpriseError::Validation(format!("{label} must be a safe identifier")));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> EnterpriseResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(EnterpriseError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_platform::{PlatformManager, PolicyEffect, PlatformPolicy, PolicyRule};

    fn governance_manager() -> (Arc<EnterpriseGovernanceManager>, Uuid) {
        let platform = Arc::new(PlatformManager::builder().build());
        platform.start().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let tenant = rt.block_on(async {
            let t = platform.create_tenant(core_agent_platform::Tenant::new("default", "Default Tenant", "system")).await.unwrap();
            // Create a policy that allows all actions
            let mut policy = core_agent_platform::PlatformPolicy::new(t.id, "enterprise-allow", "Enterprise Allow All", "system");
            policy.rules.push(core_agent_platform::PolicyRule {
                id: uuid::Uuid::new_v4(),
                subjects: ["*".into()].into_iter().collect(),
                actions: ["*".into()].into_iter().collect(),
                resources: ["*".into()].into_iter().collect(),
                attributes: std::collections::BTreeMap::new(),
                effect: core_agent_platform::PolicyEffect::Allow,
                priority: 100,
            });
            platform.create_policy(policy).await.unwrap();
            t
        });
        (Arc::new(EnterpriseGovernanceManager::new(platform)), tenant.id)
    }

    #[test]
    fn role_create_and_validate() {
        let role = Role::new(Uuid::new_v4(), "admin", "Administrator", "system");
        assert!(role.validate().is_ok());
        let mut bad = role.clone();
        bad.version = 0;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn permission_create_and_validate() {
        let p = Permission::new(Uuid::new_v4(), "code.read", "Read Code", "read", "code", "system");
        assert!(p.validate().is_ok());
    }

    #[test]
    fn role_binding_create_and_validate() {
        let b = RoleBinding::new(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), "system");
        assert!(b.validate().is_ok());
    }

    #[test]
    fn role_has_permission() {
        let mut role = Role::new(Uuid::new_v4(), "dev", "Developer", "system");
        role.permissions.insert("code.read".into());
        assert!(role.has_permission("code.read"));
        assert!(!role.has_permission("code.write"));
        role.permissions.insert("*".into());
        assert!(role.has_permission("anything"));
    }

    #[test]
    fn secret_create_and_validate() {
        let s = Secret::new(Uuid::new_v4(), "db-password", "DB Password", vec![1,2,3,4], "admin", "admin");
        assert!(s.validate().is_ok());
        let mut bad = s.clone();
        bad.encrypted_value = vec![];
        assert!(bad.validate().is_err());
    }

    #[test]
    fn governance_rbac_lifecycle() {
        let (mgr, tenant) = governance_manager();
        let role = mgr.create_role(Role::new(tenant, "admin", "Admin", "system")).unwrap();
        assert_eq!(mgr.roles(tenant).unwrap().len(), 1);
        let perm = mgr.create_permission(Permission::new(tenant, "code.read", "Read Code", "read", "code", "system")).unwrap();
        let _ = perm;
        let principal = EnterprisePrincipal::new(tenant, "alice", IdentityProviderKind::LocalAdapter, "Alice", "system");
        let rt = tokio::runtime::Runtime::new().unwrap();
        let principal = rt.block_on(mgr.bind_principal(principal)).unwrap();
        let binding = mgr.bind_role(RoleBinding::new(tenant, principal.id, role.id, "system")).unwrap();
        assert!(binding.id != Uuid::nil());
        let _ = binding;
    }

    #[test]
    fn secret_store_read_rotate() {
        let (mgr, tenant) = governance_manager();
        let secret = Secret::new(tenant, "api-key", "API Key", vec![10,20,30], "admin", "admin");
        mgr.store_secret(secret).unwrap();
        let read = mgr.read_secret(tenant, "api-key", "admin").unwrap();
        assert_eq!(read.encrypted_value, vec![10,20,30]);
        mgr.rotate_secret(tenant, "api-key", vec![40,50,60], "admin").unwrap();
        let rotated = mgr.read_secret(tenant, "api-key", "admin").unwrap();
        assert_eq!(rotated.encrypted_value, vec![40,50,60]);
        assert!(rotated.last_rotated_at.is_some());
        mgr.delete_secret(tenant, "api-key", "admin").unwrap();
        assert!(mgr.read_secret(tenant, "api-key", "admin").is_err());
    }

    #[test]
    fn agent_identity_credential_lifecycle() {
        let (mgr, tenant) = governance_manager();
        let agent_id = Uuid::new_v4();
        let cred = AgentIdentityCredential::new(agent_id, tenant, "ssh-rsa AAA...", "RSA-2048", "system");
        mgr.issue_agent_credential(cred).unwrap();
        assert!(mgr.verify_agent_identity(agent_id, Uuid::new_v4()).is_err()); // wrong id
    }

    #[test]
    fn resource_protection_access() {
        let (mgr, tenant) = governance_manager();
        let mut prot = ResourceProtection::new(tenant, "database", "salary-db", "admin");
        prot.required_permissions.insert("db.read".into());
        prot.required_permissions.insert("db.write".into());
        mgr.define_resource_protection(prot).unwrap();
        let mut perms = BTreeSet::new();
        perms.insert("db.read".into());
        assert!(mgr.check_resource_access(tenant, "database", "salary-db", "db.read", &perms).unwrap());
        assert!(!mgr.check_resource_access(tenant, "database", "salary-db", "db.write", &perms).unwrap());
        assert!(mgr.check_resource_access(tenant, "database", "other-db", "db.write", &perms).unwrap());
    }

    #[test]
    fn evidence_chain_verify() {
        let (mgr, tenant) = governance_manager();
        let audit_id = Uuid::new_v4();
        let chain = EvidenceChain::new(tenant, audit_id, "system", None);
        assert!(chain.verify_chain());
        mgr.append_evidence(chain).unwrap();
        let chains = mgr.list_evidence_chains(tenant).unwrap();
        assert_eq!(chains.len(), 1);
    }

    #[test]
    fn compliance_record_lifecycle() {
        let (mgr, tenant) = governance_manager();
        let record = ComplianceRecord::new(tenant, "agent", "agent-1", ComplianceStandard::Iso27001, "admin");
        let mut record = record;
        record.rule_name = "iso-27001-control-a6".into();
        mgr.create_compliance_record(record).unwrap();
        let records = mgr.list_compliance_records(tenant, None).unwrap();
        assert_eq!(records.len(), 1);
        let snapshot = mgr.compliance_snapshot(tenant).unwrap();
        assert_eq!(snapshot.total_resources, 1);
        assert_eq!(snapshot.not_evaluated, 1);
    }

    #[test]
    fn model_governance_record() {
        let (mgr, tenant) = governance_manager();
        let agent_id = Uuid::new_v4();
        let record = ModelGovernanceRecord::new(tenant, agent_id, "gpt-5", "1.0", "Hello", "Hi", "system");
        mgr.record_model_use(record).unwrap();
        let records = mgr.query_model_usage(tenant, None).unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn risk_assessment_validate() {
        let (mgr, tenant) = governance_manager();
        let mut assessment = AgentRiskAssessment::new(tenant, Uuid::new_v4(), "system");
        assessment.risk_score = 75;
        assessment.risk_level = AgentRiskAssessment::compute_risk_level(75);
        assert_eq!(assessment.risk_level, RiskLevel::High);
        mgr.assess_agent_risk(assessment).unwrap();
        let list = mgr.list_risk_assessments(tenant).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn compliance_mapping() {
        let mapping = ComplianceMapping::new(Uuid::new_v4(), ComplianceStandard::Soc2, "CC-1", Uuid::new_v4(), "admin");
        assert!(mapping.validate().is_ok());
    }

    #[test]
    fn governance_snapshot_includes_new_counts() {
        let (mgr, tenant) = governance_manager();
        mgr.create_role(Role::new(tenant, "viewer", "Viewer", "system")).unwrap();
        mgr.store_secret(Secret::new(tenant, "k", "K", vec![1], "admin", "admin")).unwrap();
        let snap = mgr.snapshot(tenant).unwrap();
        assert_eq!(snap.roles, 1);
        assert_eq!(snap.secrets, 1);
    }

    #[test]
    fn risk_level_computation() {
        assert_eq!(AgentRiskAssessment::compute_risk_level(0), RiskLevel::Low);
        assert_eq!(AgentRiskAssessment::compute_risk_level(29), RiskLevel::Low);
        assert_eq!(AgentRiskAssessment::compute_risk_level(30), RiskLevel::Medium);
        assert_eq!(AgentRiskAssessment::compute_risk_level(59), RiskLevel::Medium);
        assert_eq!(AgentRiskAssessment::compute_risk_level(60), RiskLevel::High);
        assert_eq!(AgentRiskAssessment::compute_risk_level(79), RiskLevel::High);
        assert_eq!(AgentRiskAssessment::compute_risk_level(80), RiskLevel::Critical);
        assert_eq!(AgentRiskAssessment::compute_risk_level(100), RiskLevel::Critical);
    }
}