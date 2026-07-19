use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{validate_metadata, WorkspaceMetadata};
use crate::error::{WorkspaceError, WorkspaceResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub label: String,
    pub storage_uri: String,
    pub resource_count: u64,
    pub total_bytes: u64,
    pub metadata: WorkspaceMetadata,
    pub created_at: DateTime<Utc>,
}

impl Snapshot {
    pub fn new(
        workspace_id: Uuid,
        label: impl Into<String>,
        storage_uri: impl Into<String>,
        resource_count: u64,
        total_bytes: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            label: label.into(),
            storage_uri: storage_uri.into(),
            resource_count,
            total_bytes,
            metadata: BTreeMap::new(),
            created_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.label.trim().is_empty() || self.label.len() > 256 {
            return Err(WorkspaceError::Validation(
                "snapshot label must contain 1..=256 characters".into(),
            ));
        }
        let url = url::Url::parse(&self.storage_uri).map_err(|error| {
            WorkspaceError::Validation(format!("invalid snapshot storage URI: {error}"))
        })?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(WorkspaceError::Validation(
                "snapshot storage URI must not contain credentials".into(),
            ));
        }
        validate_metadata(&self.metadata)
    }
}
