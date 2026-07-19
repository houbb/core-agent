use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{validate_metadata, WorkspaceMetadata};
use crate::error::{WorkspaceError, WorkspaceResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub os: String,
    pub shell: Option<String>,
    pub git: Option<String>,
    pub languages: BTreeSet<String>,
    pub runtimes: BTreeSet<String>,
    pub package_managers: BTreeSet<String>,
    /// Names only. Values are deliberately never captured.
    pub variable_names: BTreeSet<String>,
    pub metadata: WorkspaceMetadata,
    pub detected_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Environment {
    pub fn new(workspace_id: Uuid, os: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v5(&workspace_id, b"environment"),
            workspace_id,
            os: os.into(),
            shell: None,
            git: None,
            languages: BTreeSet::new(),
            runtimes: BTreeSet::new(),
            package_managers: BTreeSet::new(),
            variable_names: BTreeSet::new(),
            metadata: BTreeMap::new(),
            detected_at: now,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn merge(&mut self, other: Self) -> WorkspaceResult<()> {
        if self.workspace_id != other.workspace_id {
            return Err(WorkspaceError::Validation(
                "cannot merge environments from different workspaces".into(),
            ));
        }
        if self.shell.is_none() {
            self.shell = other.shell;
        }
        if self.git.is_none() {
            self.git = other.git;
        }
        self.languages.extend(other.languages);
        self.runtimes.extend(other.runtimes);
        self.package_managers.extend(other.package_managers);
        self.variable_names.extend(other.variable_names);
        self.metadata.extend(other.metadata);
        self.detected_at = self.detected_at.max(other.detected_at);
        self.updated_at = Utc::now();
        self.validate()
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.os.trim().is_empty() || self.os.len() > 128 {
            return Err(WorkspaceError::Validation(
                "environment OS must contain 1..=128 characters".into(),
            ));
        }
        validate_metadata(&self.metadata)
    }
}
