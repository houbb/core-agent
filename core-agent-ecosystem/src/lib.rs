//! Governed metadata catalog for the AgentOS ecosystem.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use core_agent_platform::{GovernanceRequest, PlatformManager};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type EcosystemResult<T> = Result<T, EcosystemError>;

#[derive(Debug, thiserror::Error)]
pub enum EcosystemError {
    #[error("ecosystem validation failed: {0}")]
    Validation(String),
    #[error("ecosystem resource not found: {0}")]
    NotFound(String),
    #[error("ecosystem state conflict: {0}")]
    Conflict(String),
    #[error("ecosystem authorization denied: {0}")]
    Denied(String),
    #[error("Platform governance failed: {0}")]
    Platform(String),
    #[error("ecosystem internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublisherState {
    Active,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Publisher {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key: String,
    pub name: String,
    pub subject: String,
    pub state: PublisherState,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Publisher {
    pub fn new(
        tenant_id: Uuid,
        key: impl Into<String>,
        name: impl Into<String>,
        subject: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            key: key.into(),
            name: name.into(),
            subject: subject.into(),
            state: PublisherState::Active,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    fn validate(&self) -> EcosystemResult<()> {
        validate_key("publisher key", &self.key)?;
        validate_text("publisher name", &self.name, 256)?;
        validate_actor(&self.subject)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageKind {
    Agent,
    Capability,
    Template,
    Sdk,
}

impl PackageKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Capability => "capability",
            Self::Template => "template",
            Self::Sdk => "sdk",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageState {
    Draft,
    InReview,
    Listed,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PackageCoordinate {
    pub key: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDependency {
    pub key: String,
    pub version: String,
}

impl PackageDependency {
    fn validate(&self) -> EcosystemResult<()> {
        validate_key("dependency key", &self.key)?;
        validate_version(&self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketplacePackage {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub publisher_id: Uuid,
    pub kind: PackageKind,
    pub key: String,
    pub name: String,
    pub package_version: String,
    pub description: String,
    pub dependencies: Vec<PackageDependency>,
    pub required_capabilities: BTreeSet<String>,
    pub checksum_sha256: String,
    pub signing_key_id: String,
    pub state: PackageState,
    pub download_count: u64,
    pub rating_total: u64,
    pub rating_count: u64,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MarketplacePackage {
    pub fn new(
        tenant_id: Uuid,
        publisher_id: Uuid,
        kind: PackageKind,
        key: impl Into<String>,
        name: impl Into<String>,
        package_version: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            publisher_id,
            kind,
            key: key.into(),
            name: name.into(),
            package_version: package_version.into(),
            description: String::new(),
            dependencies: Vec::new(),
            required_capabilities: BTreeSet::new(),
            checksum_sha256: "0".repeat(64),
            signing_key_id: "unsigned-local".into(),
            state: PackageState::Draft,
            download_count: 0,
            rating_total: 0,
            rating_count: 0,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
    pub fn coordinate(&self) -> PackageCoordinate {
        PackageCoordinate {
            key: self.key.clone(),
            version: self.package_version.clone(),
        }
    }
    pub fn average_rating(&self) -> Option<f64> {
        (self.rating_count > 0).then(|| self.rating_total as f64 / self.rating_count as f64)
    }
    fn validate(&self) -> EcosystemResult<()> {
        validate_key("package key", &self.key)?;
        validate_text("package name", &self.name, 256)?;
        validate_optional_text("package description", &self.description, 4096)?;
        validate_version(&self.package_version)?;
        validate_checksum(&self.checksum_sha256)?;
        validate_key("signing key id", &self.signing_key_id)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)?;
        if self.dependencies.len() > 128
            || self.required_capabilities.len() > 256
            || self.rating_total > self.rating_count.saturating_mul(5)
        {
            return Err(EcosystemError::Validation(
                "package bounds or rating aggregate are invalid".into(),
            ));
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            dependency.validate()?;
            if !dependencies.insert((dependency.key.clone(), dependency.version.clone()))
                || dependency.key == self.key && dependency.version == self.package_version
            {
                return Err(EcosystemError::Validation(
                    "duplicate or self package dependency".into(),
                ));
            }
        }
        for capability in &self.required_capabilities {
            validate_key("required capability", capability)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublicationDecision {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationReview {
    pub id: Uuid,
    pub package_id: Uuid,
    pub reviewer_subject: String,
    pub decision: PublicationDecision,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRating {
    pub id: Uuid,
    pub package_id: Uuid,
    pub subject: String,
    pub score: u8,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallationPlan {
    pub root: PackageCoordinate,
    pub packages: Vec<PackageCoordinate>,
    pub required_capabilities: BTreeSet<String>,
}

#[derive(Default)]
struct State {
    publishers: BTreeMap<Uuid, Publisher>,
    packages: BTreeMap<Uuid, MarketplacePackage>,
    reviews: Vec<PublicationReview>,
    ratings: BTreeMap<(Uuid, String), PackageRating>,
}

pub struct EcosystemManager {
    platform: Arc<PlatformManager>,
    state: RwLock<State>,
}

impl EcosystemManager {
    pub fn new(platform: Arc<PlatformManager>) -> Self {
        Self {
            platform,
            state: RwLock::new(State::default()),
        }
    }

    pub async fn register_publisher(&self, value: Publisher) -> EcosystemResult<Publisher> {
        value.validate()?;
        self.authorize(
            value.tenant_id,
            &value.actor,
            "ecosystem.publisher.register",
            "publisher",
        )
        .await?;
        let mut state = self.write()?;
        if state
            .publishers
            .values()
            .any(|item| item.tenant_id == value.tenant_id && item.key == value.key)
        {
            return Err(EcosystemError::Conflict(
                "publisher key already exists".into(),
            ));
        }
        state.publishers.insert(value.id, value.clone());
        Ok(value)
    }

    pub async fn create_package(
        &self,
        value: MarketplacePackage,
    ) -> EcosystemResult<MarketplacePackage> {
        value.validate()?;
        self.authorize(
            value.tenant_id,
            &value.actor,
            "ecosystem.package.create",
            value.kind.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        let publisher = required_publisher(&state, value.publisher_id, value.tenant_id)?;
        if publisher.subject != value.actor {
            return Err(EcosystemError::Denied(
                "only the publisher subject can create a package".into(),
            ));
        }
        if state.packages.values().any(|item| {
            item.tenant_id == value.tenant_id
                && item.key == value.key
                && item.package_version == value.package_version
        }) {
            return Err(EcosystemError::Conflict(
                "package version already exists".into(),
            ));
        }
        state.packages.insert(value.id, value.clone());
        Ok(value)
    }

    pub async fn submit(
        &self,
        package_id: Uuid,
        actor: &str,
    ) -> EcosystemResult<MarketplacePackage> {
        let current = self.required_package(package_id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "ecosystem.package.submit",
            current.kind.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        let publisher = required_publisher(&state, current.publisher_id, current.tenant_id)?;
        if publisher.subject != actor {
            return Err(EcosystemError::Denied(
                "only the publisher subject can submit a package".into(),
            ));
        }
        for dependency in &current.dependencies {
            required_listed_package(&state, current.tenant_id, dependency)?;
        }
        let package = state
            .packages
            .get_mut(&package_id)
            .ok_or_else(|| EcosystemError::NotFound(package_id.to_string()))?;
        if package.state != PackageState::Draft {
            return Err(EcosystemError::Conflict(
                "only Draft packages can enter review".into(),
            ));
        }
        package.state = PackageState::InReview;
        advance(package, actor);
        Ok(package.clone())
    }

    pub async fn review(
        &self,
        package_id: Uuid,
        decision: PublicationDecision,
        comment: &str,
        actor: &str,
    ) -> EcosystemResult<MarketplacePackage> {
        validate_text("review comment", comment, 2048)?;
        let current = self.required_package(package_id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "ecosystem.package.review",
            current.kind.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        let publisher = required_publisher(&state, current.publisher_id, current.tenant_id)?;
        if publisher.subject == actor {
            return Err(EcosystemError::Denied(
                "publisher cannot review its own package".into(),
            ));
        }
        let package = state
            .packages
            .get_mut(&package_id)
            .ok_or_else(|| EcosystemError::NotFound(package_id.to_string()))?;
        if package.state != PackageState::InReview {
            return Err(EcosystemError::Conflict(
                "package is not awaiting review".into(),
            ));
        }
        package.state = match decision {
            PublicationDecision::Approved => PackageState::Listed,
            PublicationDecision::Rejected => PackageState::Draft,
        };
        advance(package, actor);
        let result = package.clone();
        state.reviews.push(PublicationReview {
            id: Uuid::new_v4(),
            package_id,
            reviewer_subject: actor.into(),
            decision,
            comment: comment.into(),
            created_at: Utc::now(),
        });
        Ok(result)
    }

    pub async fn set_state(
        &self,
        package_id: Uuid,
        target: PackageState,
        actor: &str,
    ) -> EcosystemResult<MarketplacePackage> {
        let current = self.required_package(package_id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "ecosystem.package.transition",
            current.kind.as_str(),
        )
        .await?;
        if !matches!(
            (current.state, target),
            (PackageState::Listed, PackageState::Suspended)
                | (PackageState::Suspended, PackageState::Listed)
                | (_, PackageState::Retired)
        ) {
            return Err(EcosystemError::Conflict(
                "invalid package transition".into(),
            ));
        }
        let mut state = self.write()?;
        let package = state
            .packages
            .get_mut(&package_id)
            .ok_or_else(|| EcosystemError::NotFound(package_id.to_string()))?;
        package.state = target;
        advance(package, actor);
        Ok(package.clone())
    }

    pub async fn resolve_install(
        &self,
        tenant_id: Uuid,
        key: &str,
        version: &str,
        actor: &str,
    ) -> EcosystemResult<InstallationPlan> {
        validate_key("package key", key)?;
        validate_version(version)?;
        self.authorize(tenant_id, actor, "ecosystem.package.install", "package")
            .await?;
        let mut state = self.write()?;
        let root_id = state
            .packages
            .values()
            .find(|item| {
                item.tenant_id == tenant_id
                    && item.key == key
                    && item.package_version == version
                    && item.state == PackageState::Listed
            })
            .map(|item| item.id)
            .ok_or_else(|| EcosystemError::NotFound(format!("{key}@{version}")))?;
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut packages = Vec::new();
        let mut capabilities = BTreeSet::new();
        visit_package(
            &state,
            root_id,
            &mut visiting,
            &mut visited,
            &mut packages,
            &mut capabilities,
        )?;
        let root = state
            .packages
            .get_mut(&root_id)
            .expect("root package exists");
        root.download_count = root.download_count.saturating_add(1);
        advance(root, actor);
        Ok(InstallationPlan {
            root: PackageCoordinate {
                key: key.into(),
                version: version.into(),
            },
            packages,
            required_capabilities: capabilities,
        })
    }

    pub async fn rate(
        &self,
        package_id: Uuid,
        score: u8,
        comment: &str,
        actor: &str,
    ) -> EcosystemResult<MarketplacePackage> {
        if !(1..=5).contains(&score) {
            return Err(EcosystemError::Validation(
                "rating must be between 1 and 5".into(),
            ));
        }
        validate_optional_text("rating comment", comment, 2048)?;
        let current = self.required_package(package_id)?;
        self.authorize(
            current.tenant_id,
            actor,
            "ecosystem.rating.record",
            current.kind.as_str(),
        )
        .await?;
        let mut state = self.write()?;
        let rating_key = (package_id, actor.to_string());
        if state.ratings.contains_key(&rating_key) {
            return Err(EcosystemError::Conflict(
                "subject already rated this package".into(),
            ));
        }
        let package = state
            .packages
            .get_mut(&package_id)
            .ok_or_else(|| EcosystemError::NotFound(package_id.to_string()))?;
        if package.state != PackageState::Listed {
            return Err(EcosystemError::Conflict(
                "only Listed packages can be rated".into(),
            ));
        }
        package.rating_total = package.rating_total.saturating_add(u64::from(score));
        package.rating_count = package.rating_count.saturating_add(1);
        advance(package, actor);
        let result = package.clone();
        state.ratings.insert(
            rating_key,
            PackageRating {
                id: Uuid::new_v4(),
                package_id,
                subject: actor.into(),
                score,
                comment: comment.into(),
                created_at: Utc::now(),
            },
        );
        Ok(result)
    }

    pub fn packages(
        &self,
        tenant_id: Uuid,
        listed_only: bool,
    ) -> EcosystemResult<Vec<MarketplacePackage>> {
        Ok(self
            .read()?
            .packages
            .values()
            .filter(|item| {
                item.tenant_id == tenant_id && (!listed_only || item.state == PackageState::Listed)
            })
            .cloned()
            .collect())
    }
    pub fn publishers(&self, tenant_id: Uuid) -> EcosystemResult<Vec<Publisher>> {
        Ok(self
            .read()?
            .publishers
            .values()
            .filter(|item| item.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
    pub fn reviews(&self, package_id: Uuid) -> EcosystemResult<Vec<PublicationReview>> {
        Ok(self
            .read()?
            .reviews
            .iter()
            .filter(|item| item.package_id == package_id)
            .cloned()
            .collect())
    }

    async fn authorize(
        &self,
        tenant_id: Uuid,
        subject: &str,
        action: &str,
        resource: &str,
    ) -> EcosystemResult<()> {
        let decision = self
            .platform
            .govern(GovernanceRequest::new(
                tenant_id, subject, action, resource, subject,
            ))
            .await
            .map_err(|error| EcosystemError::Platform(error.to_string()))?;
        if !decision.allowed {
            return Err(EcosystemError::Denied(decision.reason));
        }
        Ok(())
    }
    fn required_package(&self, id: Uuid) -> EcosystemResult<MarketplacePackage> {
        self.read()?
            .packages
            .get(&id)
            .cloned()
            .ok_or_else(|| EcosystemError::NotFound(id.to_string()))
    }
    fn read(&self) -> EcosystemResult<RwLockReadGuard<'_, State>> {
        self.state
            .read()
            .map_err(|_| EcosystemError::Internal("ecosystem lock poisoned".into()))
    }
    fn write(&self) -> EcosystemResult<RwLockWriteGuard<'_, State>> {
        self.state
            .write()
            .map_err(|_| EcosystemError::Internal("ecosystem lock poisoned".into()))
    }
}

fn required_publisher(state: &State, id: Uuid, tenant_id: Uuid) -> EcosystemResult<&Publisher> {
    let value = state
        .publishers
        .get(&id)
        .filter(|item| item.tenant_id == tenant_id)
        .ok_or_else(|| EcosystemError::NotFound(id.to_string()))?;
    if value.state != PublisherState::Active {
        return Err(EcosystemError::Denied("publisher is not Active".into()));
    }
    Ok(value)
}
fn required_listed_package<'a>(
    state: &'a State,
    tenant_id: Uuid,
    dependency: &PackageDependency,
) -> EcosystemResult<&'a MarketplacePackage> {
    state
        .packages
        .values()
        .find(|item| {
            item.tenant_id == tenant_id
                && item.key == dependency.key
                && item.package_version == dependency.version
                && item.state == PackageState::Listed
        })
        .ok_or_else(|| {
            EcosystemError::NotFound(format!(
                "dependency {}@{} is not Listed",
                dependency.key, dependency.version
            ))
        })
}
fn visit_package(
    state: &State,
    id: Uuid,
    visiting: &mut BTreeSet<Uuid>,
    visited: &mut BTreeSet<Uuid>,
    output: &mut Vec<PackageCoordinate>,
    capabilities: &mut BTreeSet<String>,
) -> EcosystemResult<()> {
    if visited.contains(&id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(EcosystemError::Conflict("package dependency cycle".into()));
    }
    let package = state
        .packages
        .get(&id)
        .filter(|item| item.state == PackageState::Listed)
        .ok_or_else(|| EcosystemError::NotFound(id.to_string()))?;
    for dependency in &package.dependencies {
        let value = required_listed_package(state, package.tenant_id, dependency)?;
        visit_package(state, value.id, visiting, visited, output, capabilities)?;
    }
    capabilities.extend(package.required_capabilities.iter().cloned());
    visiting.remove(&id);
    visited.insert(id);
    output.push(package.coordinate());
    Ok(())
}
fn advance(value: &mut MarketplacePackage, actor: &str) {
    value.version = value.version.saturating_add(1);
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at);
}
fn validate_entity(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    actor: &str,
) -> EcosystemResult<()> {
    validate_actor(actor)?;
    if version == 0 || updated_at < created_at {
        return Err(EcosystemError::Validation(
            "entity version or timestamps are invalid".into(),
        ));
    }
    Ok(())
}
fn validate_actor(value: &str) -> EcosystemResult<()> {
    validate_key("actor", value)
}
fn validate_checksum(value: &str) -> EcosystemResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EcosystemError::Validation(
            "checksum must be SHA-256 hex".into(),
        ));
    }
    Ok(())
}
fn validate_version(value: &str) -> EcosystemResult<()> {
    let core = value.split(['-', '+']).next().unwrap_or_default();
    if value.len() > 64
        || core.split('.').count() != 3
        || !core
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return Err(EcosystemError::Validation(
            "version must be bounded semantic version".into(),
        ));
    }
    Ok(())
}
fn validate_key(label: &str, value: &str) -> EcosystemResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(EcosystemError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}
fn validate_text(label: &str, value: &str, max: usize) -> EcosystemResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(EcosystemError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}
fn validate_optional_text(label: &str, value: &str, max: usize) -> EcosystemResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}
