//! AgentOS Internal Contract 0.1.
//!
//! This crate is deliberately an internal, practice-derived contract. It is
//! not a public interoperability specification yet.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_SCHEMA_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 256;

pub type ProtocolResult<T> = Result<T, ProtocolError>;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("protocol validation failed: {0}")]
    Validation(String),
    #[error("protocol resource not found: {0}")]
    NotFound(String),
    #[error("protocol conflict: {0}")]
    Conflict(String),
    #[error("protocol internal error: {0}")]
    Internal(String),
    #[error("protocol serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl ProtocolVersion {
    pub const CURRENT: Self = Self {
        major: 0,
        minor: 1,
        patch: 0,
    };
    pub fn compatible_with_current(self) -> bool {
        self.major == Self::CURRENT.major && self.minor <= Self::CURRENT.minor
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtocolKind {
    Runtime,
    Capability,
    Agent,
    Workflow,
    Memory,
    Event,
    Trace,
    Ui,
    Marketplace,
    Sdk,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ResourceCoordinate {
    pub kind: ProtocolKind,
    pub key: String,
    pub version: String,
}

impl ResourceCoordinate {
    pub fn new(kind: ProtocolKind, key: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
            version: version.into(),
        }
    }
    fn validate(&self) -> ProtocolResult<()> {
        validate_key("resource key", &self.key)?;
        validate_version(&self.version)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSpec {
    pub lifecycle_endpoint: String,
    pub health_endpoint: String,
    pub event_endpoint: String,
    pub capabilities: Vec<ResourceCoordinate>,
    pub events: Vec<ResourceCoordinate>,
    pub ui: Vec<ResourceCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySpec {
    pub permissions: BTreeSet<String>,
    pub input_schema: Value,
    pub output_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    pub model: String,
    pub memory: Option<ResourceCoordinate>,
    pub workflow: Option<ResourceCoordinate>,
    pub capabilities: Vec<ResourceCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub key: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub trigger: String,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemorySpec {
    pub memory_types: BTreeSet<String>,
    pub scopes: BTreeSet<String>,
    pub shareable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSpec {
    pub category: String,
    pub payload_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceSpec {
    pub stages: Vec<String>,
    pub event_refs: Vec<ResourceCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiFieldSpec {
    pub key: String,
    pub label: String,
    pub value_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPanelSpec {
    pub key: String,
    pub title: String,
    pub panel_type: String,
    pub data_endpoint: String,
    pub fields: Vec<UiFieldSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiSpec {
    pub panels: Vec<UiPanelSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceSpec {
    pub package_kind: String,
    pub dependencies: Vec<ResourceCoordinate>,
    pub required_capabilities: Vec<ResourceCoordinate>,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkSpec {
    pub languages: BTreeSet<String>,
    pub generated_from: ProtocolVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub command: String,
    pub summary: String,
    pub invoke_endpoint: String,
    pub capability: Option<ResourceCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "spec", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtocolSpec {
    Runtime(RuntimeSpec),
    Capability(CapabilitySpec),
    Agent(AgentSpec),
    Workflow(WorkflowSpec),
    Memory(MemorySpec),
    Event(EventSpec),
    Trace(TraceSpec),
    Ui(UiSpec),
    Marketplace(MarketplaceSpec),
    Sdk(SdkSpec),
    Command(CommandSpec),
}

impl ProtocolSpec {
    pub fn kind(&self) -> ProtocolKind {
        match self {
            Self::Runtime(_) => ProtocolKind::Runtime,
            Self::Capability(_) => ProtocolKind::Capability,
            Self::Agent(_) => ProtocolKind::Agent,
            Self::Workflow(_) => ProtocolKind::Workflow,
            Self::Memory(_) => ProtocolKind::Memory,
            Self::Event(_) => ProtocolKind::Event,
            Self::Trace(_) => ProtocolKind::Trace,
            Self::Ui(_) => ProtocolKind::Ui,
            Self::Marketplace(_) => ProtocolKind::Marketplace,
            Self::Sdk(_) => ProtocolKind::Sdk,
            Self::Command(_) => ProtocolKind::Command,
        }
    }
    pub fn references(&self) -> Vec<ResourceCoordinate> {
        match self {
            Self::Runtime(value) => value
                .capabilities
                .iter()
                .chain(&value.events)
                .chain(&value.ui)
                .cloned()
                .collect(),
            Self::Agent(value) => value
                .memory
                .iter()
                .chain(value.workflow.iter())
                .chain(&value.capabilities)
                .cloned()
                .collect(),
            Self::Trace(value) => value.event_refs.clone(),
            Self::Marketplace(value) => value
                .dependencies
                .iter()
                .chain(&value.required_capabilities)
                .cloned()
                .collect(),
            Self::Command(value) => value.capability.iter().cloned().collect(),
            _ => Vec::new(),
        }
    }
    fn advertised_capabilities(&self) -> BTreeSet<String> {
        match self {
            Self::Capability(_) => BTreeSet::new(),
            Self::Runtime(value) => value
                .capabilities
                .iter()
                .map(|item| item.key.clone())
                .collect(),
            Self::Agent(value) => value
                .capabilities
                .iter()
                .map(|item| item.key.clone())
                .collect(),
            Self::Marketplace(value) => value
                .required_capabilities
                .iter()
                .map(|item| item.key.clone())
                .collect(),
            Self::Command(value) => value
                .capability
                .iter()
                .map(|item| item.key.clone())
                .collect(),
            _ => BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolDocument {
    pub contract_version: ProtocolVersion,
    pub key: String,
    pub name: String,
    pub resource_version: String,
    pub kind: ProtocolKind,
    pub spec: ProtocolSpec,
}

impl ProtocolDocument {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        resource_version: impl Into<String>,
        spec: ProtocolSpec,
    ) -> Self {
        let kind = spec.kind();
        Self {
            contract_version: ProtocolVersion::CURRENT,
            key: key.into(),
            name: name.into(),
            resource_version: resource_version.into(),
            kind,
            spec,
        }
    }
    pub fn coordinate(&self) -> ResourceCoordinate {
        ResourceCoordinate::new(self.kind, self.key.clone(), self.resource_version.clone())
    }
    pub fn content_hash(&self) -> ProtocolResult<String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub compatible: bool,
    pub issues: Vec<CompatibilityIssue>,
}

pub struct CompatibilityTestKit;

impl CompatibilityTestKit {
    pub fn check(document: &ProtocolDocument) -> CompatibilityReport {
        let mut issues = Vec::new();
        let mut issue = |code: &str, message: String| {
            issues.push(CompatibilityIssue {
                code: code.into(),
                message,
            })
        };
        if !document.contract_version.compatible_with_current() {
            issue(
                "UNSUPPORTED_CONTRACT",
                format!("contract {:?} is not supported", document.contract_version),
            );
        }
        if document.kind != document.spec.kind() {
            issue(
                "KIND_MISMATCH",
                "document kind and typed spec differ".into(),
            );
        }
        if let Err(error) = validate_key("document key", &document.key) {
            issue("INVALID_KEY", error.to_string());
        }
        if let Err(error) = validate_text("document name", &document.name, 256) {
            issue("INVALID_NAME", error.to_string());
        }
        if let Err(error) = validate_version(&document.resource_version) {
            issue("INVALID_VERSION", error.to_string());
        }
        if let Err(error) = validate_spec(&document.spec) {
            issue("INVALID_SPEC", error.to_string());
        }
        match serde_json::to_vec(document) {
            Ok(bytes) if bytes.len() > MAX_DOCUMENT_BYTES => issue(
                "DOCUMENT_TOO_LARGE",
                format!("document exceeds {MAX_DOCUMENT_BYTES} bytes"),
            ),
            Err(error) => issue("SERIALIZATION", error.to_string()),
            _ => {}
        }
        CompatibilityReport {
            compatible: issues.is_empty(),
            issues,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisteredProtocol {
    pub document: ProtocolDocument,
    pub registry_revision: u64,
    pub content_hash: String,
    pub actor: String,
    pub registered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiscoveryQuery {
    pub kind: Option<ProtocolKind>,
    pub capability: Option<String>,
}

#[derive(Default)]
struct RegistryState {
    revision: u64,
    resources: BTreeMap<ResourceCoordinate, RegisteredProtocol>,
}

#[derive(Default)]
pub struct ProtocolRegistry {
    state: RwLock<RegistryState>,
}

impl ProtocolRegistry {
    pub fn register(
        &self,
        document: ProtocolDocument,
        actor: &str,
    ) -> ProtocolResult<RegisteredProtocol> {
        validate_actor(actor)?;
        let report = CompatibilityTestKit::check(&document);
        if !report.compatible {
            return Err(ProtocolError::Validation(
                report
                    .issues
                    .into_iter()
                    .map(|item| format!("{}: {}", item.code, item.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let coordinate = document.coordinate();
        let content_hash = document.content_hash()?;
        let mut state = self.write()?;
        if let Some(existing) = state.resources.get(&coordinate) {
            return if existing.content_hash == content_hash {
                Ok(existing.clone())
            } else {
                Err(ProtocolError::Conflict(
                    "same resource version has different protocol content".into(),
                ))
            };
        }
        for reference in document.spec.references() {
            reference.validate()?;
            if !state.resources.contains_key(&reference) {
                return Err(ProtocolError::NotFound(format!(
                    "referenced {:?}:{}@{}",
                    reference.kind, reference.key, reference.version
                )));
            }
        }
        state.revision = state.revision.saturating_add(1);
        let value = RegisteredProtocol {
            document,
            registry_revision: state.revision,
            content_hash,
            actor: actor.into(),
            registered_at: Utc::now(),
        };
        state.resources.insert(coordinate, value.clone());
        Ok(value)
    }

    pub fn find(
        &self,
        coordinate: &ResourceCoordinate,
    ) -> ProtocolResult<Option<RegisteredProtocol>> {
        Ok(self.read()?.resources.get(coordinate).cloned())
    }
    pub fn schema(&self, coordinate: &ResourceCoordinate) -> ProtocolResult<Option<Value>> {
        Ok(self
            .read()?
            .resources
            .get(coordinate)
            .and_then(|item| match &item.document.spec {
                ProtocolSpec::Capability(value) => Some(
                    serde_json::json!({"input": value.input_schema, "output": value.output_schema}),
                ),
                ProtocolSpec::Event(value) => Some(value.payload_schema.clone()),
                _ => None,
            }))
    }
    pub fn discover(&self, query: &DiscoveryQuery) -> ProtocolResult<Vec<RegisteredProtocol>> {
        if let Some(capability) = &query.capability {
            validate_key("discovery capability", capability)?;
        }
        Ok(self
            .read()?
            .resources
            .values()
            .filter(|item| {
                query
                    .kind
                    .map(|kind| kind == item.document.kind)
                    .unwrap_or(true)
                    && query
                        .capability
                        .as_ref()
                        .map(|capability| {
                            item.document.kind == ProtocolKind::Capability
                                && item.document.key == *capability
                                || item
                                    .document
                                    .spec
                                    .advertised_capabilities()
                                    .contains(capability)
                        })
                        .unwrap_or(true)
            })
            .cloned()
            .collect())
    }
    pub fn revision(&self) -> ProtocolResult<u64> {
        Ok(self.read()?.revision)
    }
    fn read(&self) -> ProtocolResult<RwLockReadGuard<'_, RegistryState>> {
        self.state
            .read()
            .map_err(|_| ProtocolError::Internal("protocol registry lock poisoned".into()))
    }
    fn write(&self) -> ProtocolResult<RwLockWriteGuard<'_, RegistryState>> {
        self.state
            .write()
            .map_err(|_| ProtocolError::Internal("protocol registry lock poisoned".into()))
    }
}

fn validate_spec(spec: &ProtocolSpec) -> ProtocolResult<()> {
    match spec {
        ProtocolSpec::Runtime(value) => {
            validate_endpoint(&value.lifecycle_endpoint)?;
            validate_endpoint(&value.health_endpoint)?;
            validate_endpoint(&value.event_endpoint)?;
            validate_refs(&value.capabilities, MAX_ITEMS)?;
            validate_refs(&value.events, MAX_ITEMS)?;
            validate_refs(&value.ui, 64)
        }
        ProtocolSpec::Capability(value) => {
            if value.permissions.len() > 64 {
                return Err(ProtocolError::Validation(
                    "capability permissions exceed 64".into(),
                ));
            }
            for permission in &value.permissions {
                validate_key("permission", permission)?;
            }
            validate_schema("input schema", &value.input_schema)?;
            validate_schema("output schema", &value.output_schema)
        }
        ProtocolSpec::Agent(value) => {
            validate_key("agent model", &value.model)?;
            let refs = value
                .memory
                .iter()
                .chain(value.workflow.iter())
                .chain(&value.capabilities)
                .cloned()
                .collect::<Vec<_>>();
            validate_refs(&refs, MAX_ITEMS)
        }
        ProtocolSpec::Workflow(value) => validate_workflow(value),
        ProtocolSpec::Memory(value) => {
            if value.memory_types.is_empty()
                || value.scopes.is_empty()
                || value.memory_types.len() > 64
                || value.scopes.len() > 64
            {
                return Err(ProtocolError::Validation(
                    "memory types and scopes must contain 1..=64 entries".into(),
                ));
            }
            for item in value.memory_types.iter().chain(&value.scopes) {
                validate_key("memory declaration", item)?;
            }
            Ok(())
        }
        ProtocolSpec::Event(value) => {
            validate_key("event category", &value.category)?;
            validate_schema("event schema", &value.payload_schema)
        }
        ProtocolSpec::Trace(value) => {
            if value.stages.is_empty() || value.stages.len() > MAX_ITEMS {
                return Err(ProtocolError::Validation(
                    "trace stages must contain 1..=256 entries".into(),
                ));
            }
            validate_unique_keys("trace stage", &value.stages)?;
            validate_refs(&value.event_refs, MAX_ITEMS)
        }
        ProtocolSpec::Ui(value) => validate_ui(value),
        ProtocolSpec::Marketplace(value) => {
            validate_key("package kind", &value.package_kind)?;
            validate_checksum(&value.content_sha256)?;
            validate_refs(&value.dependencies, 128)?;
            validate_refs(&value.required_capabilities, MAX_ITEMS)
        }
        ProtocolSpec::Sdk(value) => {
            if value.languages.is_empty()
                || value.languages.len() > 16
                || !value.generated_from.compatible_with_current()
            {
                return Err(ProtocolError::Validation(
                    "SDK language set or generated contract version is invalid".into(),
                ));
            }
            for language in &value.languages {
                validate_key("SDK language", language)?;
            }
            Ok(())
        }
        ProtocolSpec::Command(value) => {
            validate_key("command", &value.command)?;
            validate_text("command summary", &value.summary, 512)?;
            validate_endpoint(&value.invoke_endpoint)?;
            if let Some(capability) = &value.capability {
                capability.validate()?;
            }
            Ok(())
        }
    }
}

fn validate_workflow(value: &WorkflowSpec) -> ProtocolResult<()> {
    validate_key("workflow trigger", &value.trigger)?;
    if value.nodes.is_empty() || value.nodes.len() > MAX_ITEMS || value.edges.len() > 1024 {
        return Err(ProtocolError::Validation(
            "workflow node or edge bounds are invalid".into(),
        ));
    }
    let mut nodes = BTreeSet::new();
    for node in &value.nodes {
        validate_key("workflow node", &node.key)?;
        validate_key("workflow node kind", &node.kind)?;
        if !nodes.insert(&node.key) {
            return Err(ProtocolError::Validation("duplicate workflow node".into()));
        }
    }
    let mut edges = BTreeSet::new();
    for edge in &value.edges {
        if edge.from == edge.to
            || !nodes.contains(&edge.from)
            || !nodes.contains(&edge.to)
            || !edges.insert((&edge.from, &edge.to))
        {
            return Err(ProtocolError::Validation("workflow edge is invalid".into()));
        }
    }
    Ok(())
}
fn validate_ui(value: &UiSpec) -> ProtocolResult<()> {
    if value.panels.is_empty() || value.panels.len() > 64 {
        return Err(ProtocolError::Validation(
            "UI must contain 1..=64 panels".into(),
        ));
    }
    let mut panels = BTreeSet::new();
    for panel in &value.panels {
        validate_key("panel key", &panel.key)?;
        validate_text("panel title", &panel.title, 128)?;
        validate_key("panel type", &panel.panel_type)?;
        validate_endpoint(&panel.data_endpoint)?;
        if !panels.insert(&panel.key) || panel.fields.len() > 64 {
            return Err(ProtocolError::Validation(
                "duplicate panel or too many fields".into(),
            ));
        }
        let mut fields = BTreeSet::new();
        for field in &panel.fields {
            validate_key("field key", &field.key)?;
            validate_text("field label", &field.label, 128)?;
            validate_key("field value type", &field.value_type)?;
            if !fields.insert(&field.key) {
                return Err(ProtocolError::Validation("duplicate UI field".into()));
            }
        }
    }
    Ok(())
}
fn validate_refs(values: &[ResourceCoordinate], max: usize) -> ProtocolResult<()> {
    if values.len() > max {
        return Err(ProtocolError::Validation(format!(
            "resource reference count exceeds {max}"
        )));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        value.validate()?;
        if !unique.insert(value) {
            return Err(ProtocolError::Validation(
                "duplicate resource reference".into(),
            ));
        }
    }
    Ok(())
}
fn validate_unique_keys(label: &str, values: &[String]) -> ProtocolResult<()> {
    let mut unique = BTreeSet::new();
    for value in values {
        validate_key(label, value)?;
        if !unique.insert(value) {
            return Err(ProtocolError::Validation(format!("duplicate {label}")));
        }
    }
    Ok(())
}
fn validate_schema(label: &str, value: &Value) -> ProtocolResult<()> {
    reject_sensitive(value, label, 0)?;
    if serde_json::to_vec(value)?.len() > MAX_SCHEMA_BYTES {
        return Err(ProtocolError::Validation(format!(
            "{label} exceeds {MAX_SCHEMA_BYTES} bytes"
        )));
    }
    Ok(())
}
fn reject_sensitive(value: &Value, label: &str, depth: usize) -> ProtocolResult<()> {
    if depth > 32 {
        return Err(ProtocolError::Validation(format!(
            "{label} nesting exceeds 32"
        )));
    }
    match value {
        Value::Object(values) => {
            for (key, nested) in values {
                let key = key.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    key.as_str(),
                    "password" | "secret" | "api_key" | "access_token" | "refresh_token"
                ) || key.ends_with("_secret")
                    || key.ends_with("_password")
                {
                    return Err(ProtocolError::Validation(format!(
                        "{label} contains a sensitive key"
                    )));
                }
                reject_sensitive(nested, label, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for nested in values {
                reject_sensitive(nested, label, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn validate_endpoint(value: &str) -> ProtocolResult<()> {
    if value.len() > 512
        || !value.starts_with("/api/")
        || value.contains("..")
        || value.contains("//")
        || value.contains(['?', '#', '@'])
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::Validation(
            "endpoint must be a safe relative /api/ path".into(),
        ));
    }
    Ok(())
}
fn validate_checksum(value: &str) -> ProtocolResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtocolError::Validation(
            "content hash must be SHA-256 hex".into(),
        ));
    }
    Ok(())
}
fn validate_version(value: &str) -> ProtocolResult<()> {
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
        return Err(ProtocolError::Validation(
            "resource version must be bounded semantic version".into(),
        ));
    }
    Ok(())
}
fn validate_actor(value: &str) -> ProtocolResult<()> {
    validate_key("actor", value)
}
fn validate_key(label: &str, value: &str) -> ProtocolResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(ProtocolError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}
fn validate_text(label: &str, value: &str, max: usize) -> ProtocolResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ProtocolError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}
