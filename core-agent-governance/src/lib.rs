use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use core_agent_platform::{GovernanceRequest, PlatformManager};
use serde::{Deserialize, Serialize};
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
}

#[derive(Default, Clone)]
struct State {
    principals: BTreeMap<Uuid, EnterprisePrincipal>,
    assets: BTreeMap<Uuid, GovernanceAsset>,
    costs: BTreeMap<String, CostRecord>,
}
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
    pub async fn bind_principal(
        &self,
        value: EnterprisePrincipal,
    ) -> EnterpriseResult<EnterprisePrincipal> {
        value.validate()?;
        self.authorize(
            value.tenant_id,
            &value.actor,
            "enterprise.identity.bind",
            "identity",
        )
        .await?;
        let mut state = self.write()?;
        if state.principals.values().any(|item| {
            item.tenant_id == value.tenant_id && item.external_subject == value.external_subject
        }) {
            return Err(EnterpriseError::Conflict(
                "external subject already bound".into(),
            ));
        }
        state.principals.insert(value.id, value.clone());
        Ok(value)
    }
    pub async fn register_asset(
        &self,
        value: GovernanceAsset,
    ) -> EnterpriseResult<GovernanceAsset> {
        value.validate()?;
        self.authorize(
            value.tenant_id,
            &value.actor,
            "enterprise.asset.register",
            value.asset_type.as_str(),
        )
        .await?;
        let state = self.read()?;
        require_principal(&state, value.tenant_id, &value.actor)?;
        if state.assets.values().any(|item| {
            item.tenant_id == value.tenant_id
                && item.asset_type == value.asset_type
                && item.key == value.key
                && item.asset_version == value.asset_version
        }) {
            return Err(EnterpriseError::Conflict(
                "asset version already registered".into(),
            ));
        }
        drop(state);
        self.write()?.assets.insert(value.id, value.clone());
        Ok(value)
    }
    pub async fn submit_asset(&self, id: Uuid, actor: &str) -> EnterpriseResult<GovernanceAsset> {
        let current = self.required_asset(id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "enterprise.asset.review",
            current.asset_type.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        require_principal(&state, current.tenant_id, actor)?;
        let value = state
            .assets
            .get_mut(&id)
            .ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if value.state != GovernanceAssetState::Draft {
            return Err(EnterpriseError::Conflict(
                "only Draft assets can enter review".into(),
            ));
        }
        value.state = GovernanceAssetState::Reviewed;
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }
    pub async fn approve_asset(
        &self,
        id: Uuid,
        comment: &str,
        actor: &str,
    ) -> EnterpriseResult<GovernanceAsset> {
        validate_text("approval comment", comment, 2048)?;
        let current = self.required_asset(id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "enterprise.asset.approve",
            current.asset_type.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        let principal = require_principal(&state, current.tenant_id, actor)?.clone();
        let value = state
            .assets
            .get_mut(&id)
            .ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if value.state != GovernanceAssetState::Reviewed {
            return Err(EnterpriseError::Conflict(
                "asset is not awaiting approval".into(),
            ));
        }
        if value.owner_subject == actor {
            return Err(EnterpriseError::Denied(
                "asset owner cannot self-approve".into(),
            ));
        }
        if value
            .approvals
            .iter()
            .any(|item| item.principal_id == principal.id)
        {
            return Err(EnterpriseError::Conflict(
                "principal already approved this asset".into(),
            ));
        }
        value.approvals.push(AssetApproval {
            id: Uuid::new_v4(),
            principal_id: principal.id,
            subject: actor.into(),
            comment: comment.into(),
            approved_at: Utc::now(),
        });
        if value.approvals.len() >= usize::from(value.required_approvals) {
            value.state = GovernanceAssetState::Approved;
        }
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }
    pub async fn transition_asset(
        &self,
        id: Uuid,
        target: GovernanceAssetState,
        actor: &str,
    ) -> EnterpriseResult<GovernanceAsset> {
        let current = self.required_asset(id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "enterprise.asset.transition",
            current.asset_type.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        require_principal(&state, current.tenant_id, actor)?;
        let value = state
            .assets
            .get_mut(&id)
            .ok_or_else(|| EnterpriseError::NotFound(id.to_string()))?;
        if !valid_asset_transition(value.state, target) {
            return Err(EnterpriseError::Conflict(
                "invalid governed asset transition".into(),
            ));
        }
        value.state = target;
        if target == GovernanceAssetState::Production {
            value.environment = AssetEnvironment::Production;
        }
        advance_asset(value, actor);
        value.validate()?;
        Ok(value.clone())
    }
    pub async fn record_cost(&self, value: CostRecord) -> EnterpriseResult<CostRecord> {
        value.validate()?;
        self.authorize(
            value.tenant_id,
            &value.actor,
            "enterprise.cost.record",
            "cost",
        )
        .await?;
        let mut state = self.write()?;
        require_principal(&state, value.tenant_id, &value.actor)?;
        if state.costs.contains_key(&value.event_key) {
            return Err(EnterpriseError::Conflict(
                "cost event already recorded".into(),
            ));
        }
        state.costs.insert(value.event_key.clone(), value.clone());
        Ok(value)
    }
    pub fn principals(&self, tenant: Uuid) -> EnterpriseResult<Vec<EnterprisePrincipal>> {
        Ok(self
            .read()?
            .principals
            .values()
            .filter(|item| item.tenant_id == tenant)
            .cloned()
            .collect())
    }
    pub fn assets(&self, tenant: Uuid) -> EnterpriseResult<Vec<GovernanceAsset>> {
        Ok(self
            .read()?
            .assets
            .values()
            .filter(|item| item.tenant_id == tenant)
            .cloned()
            .collect())
    }
    pub fn costs(&self, tenant: Uuid) -> EnterpriseResult<Vec<CostRecord>> {
        Ok(self
            .read()?
            .costs
            .values()
            .filter(|item| item.tenant_id == tenant)
            .cloned()
            .collect())
    }
    pub fn snapshot(&self, tenant: Uuid) -> EnterpriseResult<GovernanceSnapshot> {
        let state = self.read()?;
        let assets = state
            .assets
            .values()
            .filter(|item| item.tenant_id == tenant)
            .collect::<Vec<_>>();
        let costs = state
            .costs
            .values()
            .filter(|item| item.tenant_id == tenant)
            .collect::<Vec<_>>();
        let mut by = BTreeMap::new();
        for cost in &costs {
            let total = by.entry(cost.currency.clone()).or_insert(0_u64);
            *total = total.saturating_add(cost.amount_micros);
        }
        Ok(GovernanceSnapshot {
            tenant_id: tenant,
            principals: state
                .principals
                .values()
                .filter(|item| item.tenant_id == tenant)
                .count(),
            assets: assets.len(),
            production_assets: assets
                .iter()
                .filter(|item| item.state == GovernanceAssetState::Production)
                .count(),
            high_risk_assets: assets.iter().filter(|item| item.risk_score >= 70).count(),
            cost_micros_by_currency: by,
            input_tokens: costs.iter().map(|item| item.input_tokens).sum(),
            output_tokens: costs.iter().map(|item| item.output_tokens).sum(),
        })
    }
    async fn authorize(
        &self,
        tenant: Uuid,
        subject: &str,
        action: &str,
        resource: &str,
    ) -> EnterpriseResult<()> {
        let decision = self
            .platform
            .govern(GovernanceRequest::new(
                tenant, subject, action, resource, subject,
            ))
            .await
            .map_err(|error| EnterpriseError::Platform(error.to_string()))?;
        if !decision.allowed {
            return Err(EnterpriseError::Denied(decision.reason));
        }
        Ok(())
    }
    fn required_asset(&self, id: Uuid) -> EnterpriseResult<GovernanceAsset> {
        self.read()?
            .assets
            .get(&id)
            .cloned()
            .ok_or_else(|| EnterpriseError::NotFound(id.to_string()))
    }
    fn read(&self) -> EnterpriseResult<RwLockReadGuard<'_, State>> {
        self.state
            .read()
            .map_err(|_| EnterpriseError::Internal("enterprise lock poisoned".into()))
    }
    fn write(&self) -> EnterpriseResult<RwLockWriteGuard<'_, State>> {
        self.state
            .write()
            .map_err(|_| EnterpriseError::Internal("enterprise lock poisoned".into()))
    }
}

fn require_principal<'a>(
    state: &'a State,
    tenant: Uuid,
    subject: &str,
) -> EnterpriseResult<&'a EnterprisePrincipal> {
    let value = state
        .principals
        .values()
        .find(|item| item.tenant_id == tenant && item.external_subject == subject)
        .ok_or_else(|| {
            EnterpriseError::Denied("actor has no enterprise identity binding".into())
        })?;
    if value.state != PrincipalState::Active {
        return Err(EnterpriseError::Denied(
            "enterprise principal is suspended".into(),
        ));
    }
    Ok(value)
}
fn valid_asset_transition(from: GovernanceAssetState, to: GovernanceAssetState) -> bool {
    matches!(
        (from, to),
        (
            GovernanceAssetState::Approved,
            GovernanceAssetState::Production
        ) | (
            GovernanceAssetState::Production,
            GovernanceAssetState::Suspended
        ) | (
            GovernanceAssetState::Suspended,
            GovernanceAssetState::Production
        ) | (_, GovernanceAssetState::Retired)
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
        return Err(EnterpriseError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}
fn validate_text(label: &str, value: &str, max: usize) -> EnterpriseResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(EnterpriseError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}
