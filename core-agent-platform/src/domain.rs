use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{PlatformError, PlatformResult};

const MAX_ITEMS: usize = 256;
const MAX_JSON_BYTES: usize = 256 * 1024;

pub type PlatformMetadata = BTreeMap<String, Value>;

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" { return true; }
    if !pattern.contains('*') { return pattern == value; }
    let prefix: &str = pattern.split('*').next().unwrap_or("");
    let suffix: &str = pattern.rsplit('*').next().unwrap_or("");
    value.starts_with(prefix) && value.ends_with(suffix)
}

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
            pub fn parse(value: &str) -> Option<Self> { match value { $($value => Some(Self::$variant),)+ _ => None } }
        }
    };
}

// ─── Tenant Plan & Settings ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TenantPlan {
    Free,
    Pro,
    Enterprise,
    Custom,
}
string_enum!(TenantPlan { Free=>"FREE", Pro=>"PRO", Enterprise=>"ENTERPRISE", Custom=>"CUSTOM" });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantSettings {
    pub max_users: u32,
    pub max_agents: u32,
    pub max_organizations: u32,
    pub features: BTreeSet<String>,
}
impl TenantSettings {
    pub fn free() -> Self {
        Self { max_users: 5, max_agents: 3, max_organizations: 1, features: BTreeSet::new() }
    }
    pub fn pro() -> Self {
        Self { max_users: 50, max_agents: 20, max_organizations: 5, features: BTreeSet::new() }
    }
    pub fn enterprise() -> Self {
        Self { max_users: 10000, max_agents: 1000, max_organizations: 100, features: BTreeSet::new() }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        if self.max_users == 0 || self.max_agents == 0 || self.max_organizations == 0 {
            return Err(PlatformError::Validation("TenantSettings limits must be > 0".into()));
        }
        if self.features.len() > MAX_ITEMS {
            return Err(PlatformError::Validation("TenantSettings features exceed 256".into()));
        }
        for f in &self.features {
            validate_key("feature", f)?;
        }
        Ok(())
    }
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
    pub plan: TenantPlan,
    pub settings: TenantSettings,
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
            plan: TenantPlan::Free,
            settings: TenantSettings::free(),
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
        self.settings.validate()?;
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

// ─── Department / Team / EnterpriseUser ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Department {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Department {
    pub fn new(tenant_id: Uuid, organization_id: Uuid, key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id, parent_id: None,
            key: key.into(), name: name.into(), metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("department key", &self.key)?;
        validate_text("department name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        if self.parent_id == Some(self.id) {
            return Err(PlatformError::Validation("Department cannot parent itself".into()));
        }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub department_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Team {
    pub fn new(tenant_id: Uuid, organization_id: Uuid, key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id, department_id: None,
            key: key.into(), name: name.into(), metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("team key", &self.key)?;
        validate_text("team name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnterpriseUser {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub external_subject: String,
    pub display_name: String,
    pub email: String,
    pub department_ids: BTreeSet<Uuid>,
    pub team_ids: BTreeSet<Uuid>,
    pub roles: BTreeSet<String>,
    pub state: TenantState,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl EnterpriseUser {
    pub fn new(tenant_id: Uuid, external_subject: impl Into<String>, display_name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id,
            external_subject: external_subject.into(),
            display_name: display_name.into(),
            email: String::new(), department_ids: BTreeSet::new(), team_ids: BTreeSet::new(),
            roles: BTreeSet::new(), state: TenantState::Active,
            metadata: BTreeMap::new(), version: 1, actor: actor.into(),
            created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("user external subject", &self.external_subject)?;
        validate_text("user display name", &self.display_name, 256)?;
        validate_text("user email", &self.email, 256)?;
        if self.department_ids.len() > MAX_ITEMS || self.team_ids.len() > MAX_ITEMS || self.roles.len() > MAX_ITEMS {
            return Err(PlatformError::Validation("EnterpriseUser sets exceed 256".into()));
        }
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

// ─── Tenant Context ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub tenant_key: String,
    pub tenant_name: String,
    pub tenant_plan: TenantPlan,
    pub organization_id: Uuid,
    pub department_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub roles: BTreeSet<String>,
}

// ─── Policy ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyEffect {
    Allow,
    Deny,
    Ask,
}
string_enum!(PolicyEffect { Allow=>"ALLOW", Deny=>"DENY", Ask=>"ASK" });

// ─── Data Policy ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}
string_enum!(DataClassification { Public=>"PUBLIC", Internal=>"INTERNAL", Confidential=>"CONFIDENTIAL", Restricted=>"RESTRICTED" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub data_classification: DataClassification,
    pub resource_pattern: String,
    pub allowed_actions: BTreeSet<String>,
    pub denied_actions: BTreeSet<String>,
    pub enabled: bool,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl DataPolicy {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, classification: DataClassification, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id: None,
            key: key.into(), name: name.into(),
            data_classification: classification,
            resource_pattern: String::new(),
            allowed_actions: BTreeSet::new(), denied_actions: BTreeSet::new(),
            enabled: true, metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("data policy key", &self.key)?;
        validate_text("data policy name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        if self.allowed_actions.is_empty() && self.denied_actions.is_empty() {
            return Err(PlatformError::Validation("DataPolicy must have allowed or denied actions".into()));
        }
        for a in &self.allowed_actions { validate_key("data policy allowed action", a)?; }
        for a in &self.denied_actions { validate_key("data policy denied action", a)?; }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
    pub fn evaluate(&self, classification: DataClassification, resource: &str, action: &str) -> PolicyEffect {
        if !self.enabled { return PolicyEffect::Allow; }
        if classification != self.data_classification { return PolicyEffect::Allow; }
        if !self.resource_pattern.is_empty() && !glob_match(&self.resource_pattern, resource) { return PolicyEffect::Allow; }
        if self.denied_actions.contains("*") || self.denied_actions.contains(action) { return PolicyEffect::Deny; }
        if self.allowed_actions.contains("*") || self.allowed_actions.contains(action) { return PolicyEffect::Allow; }
        PolicyEffect::Deny // default deny for classified data
    }
}

// ─── Action Policy ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionEnvironment {
    Development,
    Staging,
    Production,
}
string_enum!(ActionEnvironment { Development=>"DEVELOPMENT", Staging=>"STAGING", Production=>"PRODUCTION" });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}
string_enum!(ActionRiskLevel { Low=>"LOW", Medium=>"MEDIUM", High=>"HIGH", Critical=>"CRITICAL" });

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub action_pattern: String,
    pub environment: ActionEnvironment,
    pub risk_level: ActionRiskLevel,
    pub required_approval: bool,
    pub enabled: bool,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ActionPolicy {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, action_pattern: impl Into<String>, environment: ActionEnvironment, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id: None,
            key: key.into(), name: name.into(),
            action_pattern: action_pattern.into(),
            environment, risk_level: ActionRiskLevel::Medium,
            required_approval: false, enabled: true, metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("action policy key", &self.key)?;
        validate_text("action policy name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
    pub fn matches(&self, action: &str, environment: ActionEnvironment) -> bool {
        if !self.enabled { return false; }
        if self.environment != environment { return false; }
        self.action_pattern == "*" || self.action_pattern == action
            || (action.starts_with(&self.action_pattern.replace('*', "")) && self.action_pattern.contains('*'))
    }
}

// ─── Tool Policy ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub tool_pattern: String,
    pub allowed_categories: BTreeSet<String>,
    pub denied_categories: BTreeSet<String>,
    pub allowed_tools: BTreeSet<String>,
    pub denied_tools: BTreeSet<String>,
    pub enabled: bool,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ToolPolicy {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id: None,
            key: key.into(), name: name.into(),
            tool_pattern: String::new(),
            allowed_categories: BTreeSet::new(), denied_categories: BTreeSet::new(),
            allowed_tools: BTreeSet::new(), denied_tools: BTreeSet::new(),
            enabled: true, metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("tool policy key", &self.key)?;
        validate_text("tool policy name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        for c in &self.allowed_categories { validate_key("tool policy allowed category", c)?; }
        for c in &self.denied_categories { validate_key("tool policy denied category", c)?; }
        for t in &self.allowed_tools { validate_key("tool policy allowed tool", t)?; }
        for t in &self.denied_tools { validate_key("tool policy denied tool", t)?; }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
    pub fn evaluate(&self, tool_name: &str, category: &str) -> PolicyEffect {
        if !self.enabled { return PolicyEffect::Allow; }
        if !self.tool_pattern.is_empty() && !glob_match(&self.tool_pattern, tool_name) { return PolicyEffect::Allow; }
        if self.denied_tools.contains("*") || self.denied_tools.contains(tool_name) { return PolicyEffect::Deny; }
        if self.denied_categories.contains("*") || self.denied_categories.contains(category) { return PolicyEffect::Deny; }
        if self.allowed_tools.contains("*") || self.allowed_tools.contains(tool_name) { return PolicyEffect::Allow; }
        if self.allowed_categories.contains(category) { return PolicyEffect::Allow; }
        PolicyEffect::Deny // default deny
    }
}

// ─── Model Policy ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPolicy {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub key: String,
    pub name: String,
    pub allowed_providers: BTreeSet<String>,
    pub denied_providers: BTreeSet<String>,
    pub allowed_models: BTreeSet<String>,
    pub denied_models: BTreeSet<String>,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub require_approval_for_external: bool,
    pub enabled: bool,
    pub metadata: PlatformMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl ModelPolicy {
    pub fn new(tenant_id: Uuid, key: impl Into<String>, name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(), tenant_id, organization_id: None,
            key: key.into(), name: name.into(),
            allowed_providers: BTreeSet::new(), denied_providers: BTreeSet::new(),
            allowed_models: BTreeSet::new(), denied_models: BTreeSet::new(),
            max_input_tokens: 0, max_output_tokens: 0,
            require_approval_for_external: false, enabled: true,
            metadata: BTreeMap::new(),
            version: 1, actor: actor.into(), created_at: now, updated_at: now,
        }
    }
    pub fn validate(&self) -> PlatformResult<()> {
        validate_key("model policy key", &self.key)?;
        validate_text("model policy name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        for p in &self.allowed_providers { validate_key("model policy allowed provider", p)?; }
        for p in &self.denied_providers { validate_key("model policy denied provider", p)?; }
        for m in &self.allowed_models { validate_key("model policy allowed model", m)?; }
        for m in &self.denied_models { validate_key("model policy denied model", m)?; }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
    pub fn evaluate(&self, provider: &str, model: &str) -> PolicyEffect {
        if !self.enabled { return PolicyEffect::Allow; }
        if self.denied_providers.contains("*") || self.denied_providers.contains(provider) { return PolicyEffect::Deny; }
        if self.denied_models.contains("*") || self.denied_models.contains(model) { return PolicyEffect::Deny; }
        if self.allowed_models.contains("*") || self.allowed_models.contains(model) { return PolicyEffect::Allow; }
        if self.allowed_providers.contains(provider) { return PolicyEffect::Allow; }
        PolicyEffect::Deny // default deny
    }
}

// ─── PolicyRule ──────────────────────────────────────────────────────────

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
    pub tenant_context: Option<TenantContext>,
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
            tenant_context: None,
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
    #[test]
    fn tenant_plan_and_settings() {
        let t = Tenant::new("t1", "Test", "admin");
        assert_eq!(t.plan, TenantPlan::Free);
        assert_eq!(t.settings.max_users, 5);
        assert!(t.validate().is_ok());
        let mut t2 = Tenant::new("t2", "Enterprise", "admin");
        t2.plan = TenantPlan::Enterprise;
        t2.settings = TenantSettings::enterprise();
        assert!(t2.validate().is_ok());
    }
    #[test]
    fn department_validate() {
        let d = Department::new(Uuid::new_v4(), Uuid::new_v4(), "eng", "Engineering", "admin");
        assert!(d.validate().is_ok());
        let mut bad = d.clone();
        bad.parent_id = Some(bad.id);
        assert!(bad.validate().is_err());
    }
    #[test]
    fn team_validate() {
        let t = Team::new(Uuid::new_v4(), Uuid::new_v4(), "platform", "Platform Team", "admin");
        assert!(t.validate().is_ok());
    }
    #[test]
    fn enterprise_user_validate() {
        let mut u = EnterpriseUser::new(Uuid::new_v4(), "alice", "Alice", "admin");
        u.email = "alice@example.com".into();
        assert!(u.validate().is_ok());
    }
    #[test]
    fn data_policy_evaluate() {
        let dp = DataPolicy {
            id: Uuid::new_v4(), tenant_id: Uuid::new_v4(), organization_id: None,
            key: "salary".into(), name: "Salary Data".into(),
            data_classification: DataClassification::Confidential,
            resource_pattern: "salary/*".into(),
            allowed_actions: ["read.self".into()].into_iter().collect(),
            denied_actions: ["read.all".into(), "write".into()].into_iter().collect(),
            enabled: true, metadata: BTreeMap::new(),
            version: 1, actor: "admin".into(),
            created_at: Utc::now(), updated_at: Utc::now(),
        };
        assert_eq!(dp.evaluate(DataClassification::Public, "salary/abc", "read.all"), PolicyEffect::Allow);
        assert_eq!(dp.evaluate(DataClassification::Confidential, "salary/abc", "read.all"), PolicyEffect::Deny);
        assert_eq!(dp.evaluate(DataClassification::Confidential, "salary/abc", "read.self"), PolicyEffect::Allow);
        assert_eq!(dp.evaluate(DataClassification::Confidential, "other", "read.all"), PolicyEffect::Allow);
    }
    #[test]
    fn action_policy_matches() {
        let ap = ActionPolicy::new(Uuid::new_v4(), "prod-deploy", "Production Deploy", "deploy.*", ActionEnvironment::Production, "admin");
        assert!(ap.matches("deploy.release", ActionEnvironment::Production));
        assert!(!ap.matches("deploy.release", ActionEnvironment::Development));
        assert!(!ap.matches("build", ActionEnvironment::Production));
    }
    #[test]
    fn policy_effect_ask_supported() {
        assert_eq!(PolicyEffect::Ask.as_str(), "ASK");
        assert_eq!(PolicyEffect::parse("ASK"), Some(PolicyEffect::Ask));
    }
    #[test]
    fn tenant_context_create() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(), tenant_key: "t1".into(), tenant_name: "Test".into(),
            tenant_plan: TenantPlan::Enterprise, organization_id: Uuid::new_v4(),
            department_id: None, team_id: None, user_id: None, roles: BTreeSet::new(),
        };
        assert_eq!(ctx.tenant_plan, TenantPlan::Enterprise);
    }
    #[test]
    fn glob_match_works() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("prefix/*", "prefix/value"));
        assert!(!glob_match("prefix/*", "other/value"));
    }
}
