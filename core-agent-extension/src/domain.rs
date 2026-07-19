use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ExtensionError, ExtensionResult};

const MAX_JSON_BYTES: usize = 256 * 1024;
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_ITEMS: usize = 256;

pub type ExtensionMetadata = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtensionPermission {
    FileRead,
    FileWrite,
    Network,
    Process,
    Environment,
}
string_enum!(ExtensionPermission {
    FileRead => "FILE_READ",
    FileWrite => "FILE_WRITE",
    Network => "NETWORK",
    Process => "PROCESS",
    Environment => "ENVIRONMENT",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderKind {
    Local,
    Mcp,
    Http,
    Process,
    Native,
    Wasm,
}
string_enum!(ProviderKind {
    Local => "LOCAL",
    Mcp => "MCP",
    Http => "HTTP",
    Process => "PROCESS",
    Native => "NATIVE",
    Wasm => "WASM",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub key: String,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub permissions: BTreeSet<ExtensionPermission>,
    #[serde(default)]
    pub metadata: ExtensionMetadata,
}

impl CapabilityManifest {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("capability key", &self.key)?;
        validate_version("capability version", &self.version)?;
        validate_text("capability name", &self.name, 256)?;
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderManifest {
    pub key: String,
    pub kind: ProviderKind,
    pub capabilities: BTreeSet<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub metadata: ExtensionMetadata,
}

impl ProviderManifest {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("provider key", &self.key)?;
        if self.capabilities.is_empty() || self.capabilities.len() > MAX_ITEMS {
            return Err(ExtensionError::Validation(
                "provider must declare 1..=256 capabilities".into(),
            ));
        }
        for capability in &self.capabilities {
            validate_key("provider capability", capability)?;
        }
        validate_json("provider config", &self.config, MAX_JSON_BYTES)?;
        validate_metadata(&self.metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub key: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub entrypoint: String,
    #[serde(default)]
    pub requested_permissions: BTreeSet<ExtensionPermission>,
    pub capabilities: Vec<CapabilityManifest>,
    pub providers: Vec<ProviderManifest>,
    #[serde(default)]
    pub metadata: ExtensionMetadata,
}

impl ExtensionManifest {
    pub fn from_yaml(value: &str) -> ExtensionResult<Self> {
        let manifest: Self = serde_yaml::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("extension key", &self.key)?;
        validate_text("extension name", &self.name, 256)?;
        validate_version("extension version", &self.version)?;
        validate_optional_text("extension description", &self.description, 4096)?;
        validate_text("extension entrypoint", &self.entrypoint, 1024)?;
        validate_metadata(&self.metadata)?;
        if self.capabilities.is_empty()
            || self.capabilities.len() > MAX_ITEMS
            || self.providers.is_empty()
            || self.providers.len() > MAX_ITEMS
        {
            return Err(ExtensionError::Validation(
                "extension must declare 1..=256 capabilities and providers".into(),
            ));
        }
        let mut capability_keys = BTreeSet::new();
        for capability in &self.capabilities {
            capability.validate()?;
            if !capability_keys.insert(capability.key.clone()) {
                return Err(ExtensionError::Validation(
                    "extension contains duplicate Capability keys".into(),
                ));
            }
            if !capability
                .permissions
                .is_subset(&self.requested_permissions)
            {
                return Err(ExtensionError::Validation(
                    "capability permissions exceed Extension manifest permissions".into(),
                ));
            }
        }
        let mut provider_keys = BTreeSet::new();
        for provider in &self.providers {
            provider.validate()?;
            if !provider_keys.insert(provider.key.clone())
                || !provider.capabilities.is_subset(&capability_keys)
            {
                return Err(ExtensionError::Validation(
                    "provider key is duplicated or references unknown Capability".into(),
                ));
            }
        }
        validate_size(self, "extension manifest")
    }
}

#[derive(Debug, Clone)]
pub struct InstallExtensionRequest {
    pub manifest: ExtensionManifest,
    pub source_uri: String,
    pub checksum: String,
    pub actor: String,
}

impl InstallExtensionRequest {
    pub fn validate(&self) -> ExtensionResult<()> {
        self.manifest.validate()?;
        validate_source_uri(&self.source_uri)?;
        validate_checksum(&self.checksum)?;
        validate_actor(&self.actor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExtensionState {
    Installed,
    Loaded,
    Enabled,
    Running,
    Disabled,
    Uninstalled,
}
string_enum!(ExtensionState {
    Installed => "INSTALLED",
    Loaded => "LOADED",
    Enabled => "ENABLED",
    Running => "RUNNING",
    Disabled => "DISABLED",
    Uninstalled => "UNINSTALLED",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extension {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub current_manifest_id: Uuid,
    pub current_version: String,
    pub state: ExtensionState,
    pub current_request_id: Option<Uuid>,
    pub current_capability_key: Option<String>,
    pub current_provider_id: Option<Uuid>,
    pub current_invocation_hash: Option<String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Extension {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("extension key", &self.key)?;
        validate_text("extension name", &self.name, 256)?;
        validate_version("extension version", &self.current_version)?;
        if let Some(key) = &self.current_capability_key {
            validate_key("current capability key", key)?;
        }
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)?;
        let running_identity = self.current_request_id.is_some()
            && self.current_capability_key.is_some()
            && self.current_provider_id.is_some()
            && self.current_invocation_hash.is_some();
        if (self.state == ExtensionState::Running) != running_identity {
            return Err(ExtensionError::Validation(
                "Extension Running state and invocation identity are inconsistent".into(),
            ));
        }
        validate_size(self, "extension")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionManifestRecord {
    pub id: Uuid,
    pub extension_id: Uuid,
    pub revision: u64,
    pub manifest: ExtensionManifest,
    pub source_uri: String,
    pub checksum: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl ExtensionManifestRecord {
    pub fn validate(&self) -> ExtensionResult<()> {
        self.manifest.validate()?;
        validate_source_uri(&self.source_uri)?;
        validate_checksum(&self.checksum)?;
        validate_actor(&self.actor)?;
        if self.revision == 0 {
            return Err(ExtensionError::Validation(
                "manifest revision must be positive".into(),
            ));
        }
        validate_size(self, "extension manifest record")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: Uuid,
    pub extension_id: Uuid,
    pub manifest_id: Uuid,
    pub key: String,
    pub version_name: String,
    pub name: String,
    pub permissions: BTreeSet<ExtensionPermission>,
    pub enabled: bool,
    pub metadata: ExtensionMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Capability {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("capability key", &self.key)?;
        validate_version("capability version", &self.version_name)?;
        validate_text("capability name", &self.name, 256)?;
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)?;
        validate_size(self, "capability")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    pub id: Uuid,
    pub extension_id: Uuid,
    pub manifest_id: Uuid,
    pub key: String,
    pub kind: ProviderKind,
    pub capabilities: BTreeSet<String>,
    pub priority: i32,
    pub config: Value,
    pub enabled: bool,
    pub metadata: ExtensionMetadata,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Provider {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("provider key", &self.key)?;
        if self.capabilities.is_empty() || self.capabilities.len() > MAX_ITEMS {
            return Err(ExtensionError::Validation(
                "provider Capability set is invalid".into(),
            ));
        }
        for capability in &self.capabilities {
            validate_key("provider capability", capability)?;
        }
        validate_json("provider config", &self.config, MAX_JSON_BYTES)?;
        validate_metadata(&self.metadata)?;
        validate_entity(self.version, self.created_at, self.updated_at, &self.actor)?;
        validate_size(self, "provider")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtensionStateRecord {
    pub id: Uuid,
    pub extension_id: Uuid,
    pub sequence: u64,
    pub from_state: Option<ExtensionState>,
    pub to_state: ExtensionState,
    pub reason: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl ExtensionStateRecord {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_text("extension state reason", &self.reason, 1024)?;
        validate_actor(&self.actor)?;
        if self.sequence == 0 || self.from_state == Some(self.to_state) {
            return Err(ExtensionError::Validation(
                "extension state record is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionLoadHandle {
    pub extension_id: Uuid,
    pub manifest_id: Uuid,
    pub generation: Uuid,
}

#[derive(Debug, Clone)]
pub struct CapabilityInvocation {
    pub request_id: Uuid,
    pub capability_key: String,
    pub input: Value,
    pub metadata: ExtensionMetadata,
    pub actor: String,
}

impl CapabilityInvocation {
    pub fn new(capability_key: impl Into<String>, input: Value, actor: impl Into<String>) -> Self {
        Self {
            request_id: Uuid::new_v4(),
            capability_key: capability_key.into(),
            input,
            metadata: BTreeMap::new(),
            actor: actor.into(),
        }
    }

    pub fn validate(&self) -> ExtensionResult<()> {
        validate_key("invocation capability", &self.capability_key)?;
        validate_json("invocation input", &self.input, MAX_JSON_BYTES)?;
        validate_metadata(&self.metadata)?;
        validate_actor(&self.actor)
    }
}

pub(crate) fn invocation_hash(value: &CapabilityInvocation) -> ExtensionResult<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "request_id": value.request_id,
        "capability_key": value.capability_key,
        "input": value.input,
        "metadata": value.metadata,
        "actor": value.actor,
    }))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityResult {
    pub request_id: Uuid,
    pub provider_id: Uuid,
    pub summary: String,
    pub output: Value,
    pub completed_at: DateTime<Utc>,
}

impl CapabilityResult {
    pub fn validate(&self) -> ExtensionResult<()> {
        validate_text("capability result summary", &self.summary, 2048)?;
        validate_json("capability result output", &self.output, MAX_JSON_BYTES)
    }
}

pub(crate) fn entities_from_manifest(
    extension: &Extension,
    record: &ExtensionManifestRecord,
    actor: &str,
) -> (Vec<Capability>, Vec<Provider>) {
    let now = Utc::now();
    let capabilities = record
        .manifest
        .capabilities
        .iter()
        .map(|value| Capability {
            id: Uuid::new_v4(),
            extension_id: extension.id,
            manifest_id: record.id,
            key: value.key.clone(),
            version_name: value.version.clone(),
            name: value.name.clone(),
            permissions: value.permissions.clone(),
            enabled: false,
            metadata: value.metadata.clone(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        })
        .collect();
    let providers = record
        .manifest
        .providers
        .iter()
        .map(|value| Provider {
            id: Uuid::new_v4(),
            extension_id: extension.id,
            manifest_id: record.id,
            key: value.key.clone(),
            kind: value.kind,
            capabilities: value.capabilities.clone(),
            priority: value.priority,
            config: value.config.clone(),
            enabled: false,
            metadata: value.metadata.clone(),
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        })
        .collect();
    (capabilities, providers)
}

pub(crate) fn validate_actor(value: &str) -> ExtensionResult<()> {
    validate_text("actor", value, 256)
}

pub(crate) fn validate_source_uri(value: &str) -> ExtensionResult<()> {
    validate_text("extension source URI", value, 2048)?;
    if !value.starts_with("file:") || value.contains('@') || value.contains("..") {
        return Err(ExtensionError::Validation(
            "P12.0 source must be a credential-free local file URI without traversal".into(),
        ));
    }
    Ok(())
}

fn validate_checksum(value: &str) -> ExtensionResult<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ExtensionError::Validation(
            "extension checksum must be a SHA-256 hex digest".into(),
        ));
    }
    Ok(())
}

fn validate_version(label: &str, value: &str) -> ExtensionResult<()> {
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
        return Err(ExtensionError::Validation(format!(
            "{label} must be a bounded semantic version"
        )));
    }
    Ok(())
}

fn validate_key(label: &str, value: &str) -> ExtensionResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(ExtensionError::Validation(format!(
            "{label} must be a safe bounded key"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, max: usize) -> ExtensionResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(ExtensionError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

fn validate_optional_text(label: &str, value: &str, max: usize) -> ExtensionResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}

fn validate_entity(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    actor: &str,
) -> ExtensionResult<()> {
    validate_actor(actor)?;
    if version == 0 || updated_at < created_at {
        return Err(ExtensionError::Validation(
            "entity version or timestamps are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_metadata(value: &ExtensionMetadata) -> ExtensionResult<()> {
    if value.len() > MAX_ITEMS {
        return Err(ExtensionError::Validation(
            "metadata entry count exceeds 256".into(),
        ));
    }
    validate_json("metadata", &serde_json::to_value(value)?, MAX_JSON_BYTES)
}

fn validate_json(label: &str, value: &Value, max: usize) -> ExtensionResult<()> {
    reject_sensitive(value, label, 0)?;
    if serde_json::to_vec(value)?.len() > max {
        return Err(ExtensionError::Validation(format!(
            "{label} exceeds {max} bytes"
        )));
    }
    Ok(())
}

fn reject_sensitive(value: &Value, label: &str, depth: usize) -> ExtensionResult<()> {
    if depth > 32 {
        return Err(ExtensionError::Validation(format!(
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
                    || key.ends_with("_api_key")
                {
                    return Err(ExtensionError::Validation(format!(
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

fn validate_size<T: Serialize>(value: &T, label: &str) -> ExtensionResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(ExtensionError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ExtensionManifest {
        ExtensionManifest {
            key: "git".into(),
            name: "Git".into(),
            version: "1.0.0".into(),
            description: String::new(),
            entrypoint: "git-extension".into(),
            requested_permissions: BTreeSet::new(),
            capabilities: vec![CapabilityManifest {
                key: "git.status".into(),
                version: "1.0.0".into(),
                name: "Git Status".into(),
                permissions: BTreeSet::new(),
                metadata: BTreeMap::new(),
            }],
            providers: vec![ProviderManifest {
                key: "local-git".into(),
                kind: ProviderKind::Local,
                capabilities: ["git.status".into()].into_iter().collect(),
                priority: 0,
                config: Value::Null,
                metadata: BTreeMap::new(),
            }],
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn yaml_manifest_is_strict_and_round_trips() {
        let yaml = serde_yaml::to_string(&manifest()).unwrap();
        assert_eq!(ExtensionManifest::from_yaml(&yaml).unwrap(), manifest());
    }

    #[test]
    fn provider_cannot_reference_unknown_capability() {
        let mut value = manifest();
        value.providers[0].capabilities.insert("git.push".into());
        assert!(matches!(
            value.validate(),
            Err(ExtensionError::Validation(_))
        ));
    }

    #[test]
    fn manifest_rejects_nested_secrets() {
        let mut value = manifest();
        value.providers[0].config = serde_json::json!({"auth": {"api_key": "x"}});
        assert!(matches!(
            value.validate(),
            Err(ExtensionError::Validation(_))
        ));
    }
}
