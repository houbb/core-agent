use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{validate_metadata, WorkspaceMetadata};
use crate::error::{WorkspaceError, WorkspaceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceType {
    File,
    Directory,
    Image,
    Pdf,
    Markdown,
    Binary,
    Terminal,
    Database,
}

impl ResourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "FILE",
            Self::Directory => "DIRECTORY",
            Self::Image => "IMAGE",
            Self::Pdf => "PDF",
            Self::Markdown => "MARKDOWN",
            Self::Binary => "BINARY",
            Self::Terminal => "TERMINAL",
            Self::Database => "DATABASE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceCapability {
    Read,
    Write,
    Delete,
    Search,
    Execute,
    Watch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub project_id: Option<Uuid>,
    pub resource_type: ResourceType,
    pub uri: String,
    pub name: String,
    pub size_bytes: Option<u64>,
    pub capabilities: BTreeSet<ResourceCapability>,
    pub provider_key: String,
    pub metadata: WorkspaceMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Resource {
    pub fn new(
        workspace_id: Uuid,
        resource_type: ResourceType,
        uri: impl Into<String>,
        name: impl Into<String>,
        size_bytes: Option<u64>,
        capabilities: BTreeSet<ResourceCapability>,
        provider_key: impl Into<String>,
    ) -> Self {
        let uri = uri.into();
        let now = Utc::now();
        Self {
            id: Uuid::new_v5(&workspace_id, uri.as_bytes()),
            workspace_id,
            project_id: None,
            resource_type,
            uri,
            name: name.into(),
            size_bytes,
            capabilities,
            provider_key: provider_key.into(),
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.name.is_empty() || self.name.len() > 512 {
            return Err(WorkspaceError::Validation(
                "resource name must contain 1..=512 characters".into(),
            ));
        }
        if self.provider_key.trim().is_empty() {
            return Err(WorkspaceError::Validation(
                "resource provider key cannot be empty".into(),
            ));
        }
        let uri = url::Url::parse(&self.uri).map_err(|error| {
            WorkspaceError::Validation(format!("invalid resource URI: {error}"))
        })?;
        if !uri.username().is_empty() || uri.password().is_some() {
            return Err(WorkspaceError::Validation(
                "resource URI must not contain credentials".into(),
            ));
        }
        validate_metadata(&self.metadata)
    }
}
