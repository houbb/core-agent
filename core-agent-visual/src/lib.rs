//! Safe, declarative visual protocol shared by Runtime and Studio surfaces.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub type VisualResult<T> = Result<T, VisualError>;

#[derive(Debug, thiserror::Error)]
pub enum VisualError {
    #[error("visual descriptor validation failed: {0}")]
    Validation(String),
    #[error("visual descriptor conflict: {0}")]
    Conflict(String),
    #[error("visual registry internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PanelKind {
    Summary,
    Table,
    Timeline,
    Form,
    Graph,
    Metrics,
    Inspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldKind {
    Text,
    Number,
    Boolean,
    Status,
    Timestamp,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionMethod {
    Get,
    Post,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualField {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    pub sortable: bool,
    pub filterable: bool,
}

impl VisualField {
    fn validate(&self) -> VisualResult<()> {
        validate_key("field key", &self.key)?;
        validate_text("field label", &self.label, 128)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualAction {
    pub key: String,
    pub label: String,
    pub method: ActionMethod,
    pub endpoint: String,
    pub dangerous: bool,
    pub requires_approval: bool,
}

impl VisualAction {
    fn validate(&self) -> VisualResult<()> {
        validate_key("action key", &self.key)?;
        validate_text("action label", &self.label, 128)?;
        validate_endpoint(&self.endpoint)?;
        if (self.dangerous || self.method == ActionMethod::Delete) && !self.requires_approval {
            return Err(VisualError::Validation(
                "dangerous and DELETE actions must require approval".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualDataSource {
    pub endpoint: String,
    pub refresh_seconds: Option<u64>,
}

impl VisualDataSource {
    fn validate(&self) -> VisualResult<()> {
        validate_endpoint(&self.endpoint)?;
        if self
            .refresh_seconds
            .is_some_and(|value| !(2..=86_400).contains(&value))
        {
            return Err(VisualError::Validation(
                "visual refresh must be between 2 seconds and 1 day".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualPanelDescriptor {
    pub key: String,
    pub title: String,
    pub description: String,
    pub icon: Option<String>,
    pub kind: PanelKind,
    pub data_source: VisualDataSource,
    pub fields: Vec<VisualField>,
    pub actions: Vec<VisualAction>,
}

impl VisualPanelDescriptor {
    pub fn validate(&self) -> VisualResult<()> {
        validate_key("panel key", &self.key)?;
        validate_text("panel title", &self.title, 128)?;
        if self.description.len() > 1024 || self.description.chars().any(char::is_control) {
            return Err(VisualError::Validation(
                "panel description is invalid".into(),
            ));
        }
        if let Some(icon) = &self.icon {
            validate_key("icon key", icon)?;
        }
        self.data_source.validate()?;
        if self.fields.len() > 64 || self.actions.len() > 32 {
            return Err(VisualError::Validation(
                "panel field or action count exceeds the protocol limit".into(),
            ));
        }
        let mut fields = BTreeSet::new();
        for field in &self.fields {
            field.validate()?;
            if !fields.insert(&field.key) {
                return Err(VisualError::Validation("duplicate visual field".into()));
            }
        }
        let mut actions = BTreeSet::new();
        for action in &self.actions {
            action.validate()?;
            if !actions.insert(&action.key) {
                return Err(VisualError::Validation("duplicate visual action".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualDescriptor {
    pub protocol_version: u64,
    pub runtime_id: String,
    pub runtime_version: String,
    pub revision: u64,
    pub panels: Vec<VisualPanelDescriptor>,
    pub updated_at: DateTime<Utc>,
}

impl VisualDescriptor {
    pub fn new(
        runtime_id: impl Into<String>,
        runtime_version: impl Into<String>,
        panels: Vec<VisualPanelDescriptor>,
    ) -> Self {
        Self {
            protocol_version: 1,
            runtime_id: runtime_id.into(),
            runtime_version: runtime_version.into(),
            revision: 1,
            panels,
            updated_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> VisualResult<()> {
        if self.protocol_version != 1 || self.revision == 0 {
            return Err(VisualError::Validation(
                "unsupported protocol version or revision".into(),
            ));
        }
        validate_key("runtime id", &self.runtime_id)?;
        validate_key("runtime version", &self.runtime_version)?;
        if self.panels.is_empty() || self.panels.len() > 64 {
            return Err(VisualError::Validation(
                "descriptor must contain 1 to 64 panels".into(),
            ));
        }
        let mut panels = BTreeSet::new();
        for panel in &self.panels {
            panel.validate()?;
            if !panels.insert(&panel.key) {
                return Err(VisualError::Validation("duplicate panel key".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredPanel {
    pub id: String,
    pub runtime_id: String,
    pub runtime_version: String,
    pub descriptor_revision: u64,
    pub panel: VisualPanelDescriptor,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudioPanelCatalog {
    pub panels: Vec<RegisteredPanel>,
}

#[derive(Default)]
pub struct VisualRegistry {
    descriptors: RwLock<BTreeMap<String, VisualDescriptor>>,
}

impl VisualRegistry {
    pub fn register(
        &self,
        descriptor: VisualDescriptor,
        expected_revision: Option<u64>,
    ) -> VisualResult<VisualDescriptor> {
        descriptor.validate()?;
        let mut descriptors = self.write()?;
        match (descriptors.get(&descriptor.runtime_id), expected_revision) {
            (None, None) if descriptor.revision == 1 => {}
            (Some(current), Some(expected))
                if current.revision == expected && descriptor.revision == expected + 1 => {}
            _ => {
                return Err(VisualError::Conflict(
                    "visual descriptor revision conflict".into(),
                ))
            }
        }
        descriptors.insert(descriptor.runtime_id.clone(), descriptor.clone());
        Ok(descriptor)
    }

    pub fn find(&self, runtime_id: &str) -> VisualResult<Option<VisualDescriptor>> {
        Ok(self.read()?.get(runtime_id).cloned())
    }

    pub fn list(&self) -> VisualResult<Vec<VisualDescriptor>> {
        Ok(self.read()?.values().cloned().collect())
    }

    pub fn catalog(&self) -> VisualResult<StudioPanelCatalog> {
        let mut panels = self
            .read()?
            .values()
            .flat_map(|descriptor| {
                descriptor
                    .panels
                    .iter()
                    .cloned()
                    .map(|panel| RegisteredPanel {
                        id: format!("{}/{}", descriptor.runtime_id, panel.key),
                        runtime_id: descriptor.runtime_id.clone(),
                        runtime_version: descriptor.runtime_version.clone(),
                        descriptor_revision: descriptor.revision,
                        panel,
                    })
            })
            .collect::<Vec<_>>();
        panels.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(StudioPanelCatalog { panels })
    }

    fn read(&self) -> VisualResult<RwLockReadGuard<'_, BTreeMap<String, VisualDescriptor>>> {
        self.descriptors
            .read()
            .map_err(|_| VisualError::Internal("visual registry lock poisoned".into()))
    }

    fn write(&self) -> VisualResult<RwLockWriteGuard<'_, BTreeMap<String, VisualDescriptor>>> {
        self.descriptors
            .write()
            .map_err(|_| VisualError::Internal("visual registry lock poisoned".into()))
    }
}

fn validate_endpoint(endpoint: &str) -> VisualResult<()> {
    if !endpoint.starts_with("/api/")
        || endpoint.len() > 1024
        || endpoint.contains("..")
        || endpoint.contains('?')
        || endpoint.contains('#')
        || endpoint.chars().any(char::is_control)
    {
        return Err(VisualError::Validation(
            "visual endpoint must be a safe relative /api/ path".into(),
        ));
    }
    Ok(())
}

fn validate_key(label: &str, value: &str) -> VisualResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(VisualError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}

fn validate_text(label: &str, value: &str, maximum: usize) -> VisualResult<()> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(VisualError::Validation(format!("{label} is invalid")));
    }
    Ok(())
}
