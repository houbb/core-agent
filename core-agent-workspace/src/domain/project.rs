use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{validate_metadata, WorkspaceMetadata};
use crate::error::{WorkspaceError, WorkspaceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectKind {
    Rust,
    Maven,
    Gradle,
    Node,
    Python,
    Generic,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "RUST",
            Self::Maven => "MAVEN",
            Self::Gradle => "GRADLE",
            Self::Node => "NODE",
            Self::Python => "PYTHON",
            Self::Generic => "GENERIC",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub kind: ProjectKind,
    pub root_uri: String,
    pub module_count: u32,
    pub markers: Vec<String>,
    pub metadata: WorkspaceMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    pub fn new(
        workspace_id: Uuid,
        name: impl Into<String>,
        kind: ProjectKind,
        root_uri: impl Into<String>,
        markers: Vec<String>,
    ) -> Self {
        let root_uri = root_uri.into();
        let now = Utc::now();
        Self {
            id: Uuid::new_v5(&workspace_id, root_uri.as_bytes()),
            workspace_id,
            name: name.into(),
            kind,
            root_uri,
            module_count: 1,
            markers,
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(WorkspaceError::Validation(
                "project name must contain 1..=256 characters".into(),
            ));
        }
        let uri = url::Url::parse(&self.root_uri)
            .map_err(|error| WorkspaceError::Validation(format!("invalid project URI: {error}")))?;
        if !uri.username().is_empty() || uri.password().is_some() {
            return Err(WorkspaceError::Validation(
                "project URI must not contain credentials".into(),
            ));
        }
        validate_metadata(&self.metadata)
    }
}
