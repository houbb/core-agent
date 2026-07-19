use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;
use uuid::Uuid;

use crate::domain::{Environment, GraphNodeKind, Project, Resource, WorkspaceGraph};
use crate::error::{WorkspaceError, WorkspaceResult};

pub type WorkspaceMetadata = BTreeMap<String, Value>;

const MAX_METADATA_BYTES: usize = 64 * 1024;

pub fn validate_actor(actor: &str) -> WorkspaceResult<()> {
    let actor = actor.trim();
    if actor.is_empty() || actor.len() > 128 || actor.chars().any(char::is_control) {
        return Err(WorkspaceError::Validation(
            "actor must contain 1..=128 printable characters".into(),
        ));
    }
    Ok(())
}

pub fn validate_metadata(metadata: &WorkspaceMetadata) -> WorkspaceResult<()> {
    let encoded = serde_json::to_vec(metadata)?;
    if encoded.len() > MAX_METADATA_BYTES {
        return Err(WorkspaceError::Validation(format!(
            "metadata exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    for key in metadata.keys() {
        let normalized = key.to_ascii_lowercase();
        if ["password", "secret", "token", "api_key", "private_key"]
            .iter()
            .any(|sensitive| normalized.contains(sensitive))
        {
            return Err(WorkspaceError::Validation(format!(
                "metadata key `{key}` may contain secret material"
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkspaceState {
    Created,
    Loaded,
    Ready,
    Modified,
    Snapshot,
    Closed,
}

impl WorkspaceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "CREATED",
            Self::Loaded => "LOADED",
            Self::Ready => "READY",
            Self::Modified => "MODIFIED",
            Self::Snapshot => "SNAPSHOT",
            Self::Closed => "CLOSED",
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        use WorkspaceState::*;
        matches!(
            (self, next),
            (Created, Loaded | Closed)
                | (Loaded, Ready | Closed)
                | (Ready, Loaded | Modified | Snapshot | Closed)
                | (Modified, Loaded | Snapshot | Closed)
                | (Snapshot, Loaded | Ready | Modified | Snapshot | Closed)
                | (Closed, Loaded)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub provider_key: String,
    pub uri: String,
    pub state: WorkspaceState,
    pub projects: Vec<Project>,
    pub environment: Option<Environment>,
    pub resources: Vec<Resource>,
    pub graph: WorkspaceGraph,
    pub metadata: WorkspaceMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Workspace {
    pub fn new(
        name: impl Into<String>,
        provider_key: impl Into<String>,
        uri: impl Into<String>,
        metadata: WorkspaceMetadata,
    ) -> WorkspaceResult<Self> {
        let now = Utc::now();
        let workspace = Self {
            id: Uuid::new_v4(),
            name: name.into(),
            provider_key: provider_key.into(),
            uri: uri.into(),
            state: WorkspaceState::Created,
            projects: Vec::new(),
            environment: None,
            resources: Vec::new(),
            graph: WorkspaceGraph::default(),
            metadata,
            created_at: now,
            updated_at: now,
        };
        workspace.validate()?;
        Ok(workspace)
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(WorkspaceError::Validation(
                "workspace name must contain 1..=256 characters".into(),
            ));
        }
        if self.provider_key.trim().is_empty() || self.provider_key.len() > 128 {
            return Err(WorkspaceError::Validation(
                "provider key must contain 1..=128 characters".into(),
            ));
        }
        let uri = Url::parse(&self.uri)
            .map_err(|error| WorkspaceError::Validation(format!("invalid URI: {error}")))?;
        if uri.scheme().is_empty() || !uri.username().is_empty() || uri.password().is_some() {
            return Err(WorkspaceError::Validation(
                "workspace URI must have a scheme and must not contain credentials".into(),
            ));
        }
        validate_metadata(&self.metadata)?;
        let project_ids = self
            .projects
            .iter()
            .map(|project| project.id)
            .collect::<BTreeSet<_>>();
        if self.projects.iter().any(|p| p.workspace_id != self.id)
            || self.resources.iter().any(|r| r.workspace_id != self.id)
            || self.resources.iter().any(|resource| {
                resource
                    .project_id
                    .is_some_and(|project_id| !project_ids.contains(&project_id))
            })
            || self
                .environment
                .as_ref()
                .is_some_and(|environment| environment.workspace_id != self.id)
        {
            return Err(WorkspaceError::Validation(
                "workspace child identity mismatch".into(),
            ));
        }
        for project in &self.projects {
            project.validate()?;
        }
        for resource in &self.resources {
            resource.validate()?;
        }
        if let Some(environment) = &self.environment {
            environment.validate()?;
        }
        self.graph.validate()?;
        if matches!(
            self.state,
            WorkspaceState::Ready
                | WorkspaceState::Modified
                | WorkspaceState::Snapshot
                | WorkspaceState::Closed
        ) {
            if self.environment.is_none() {
                return Err(WorkspaceError::Validation(
                    "active or closed workspace must contain a detected environment".into(),
                ));
            }
            let mut expected_graph_nodes =
                BTreeMap::from([(format!("workspace:{}", self.id), GraphNodeKind::Workspace)]);
            expected_graph_nodes.extend(
                self.projects
                    .iter()
                    .map(|project| (format!("project:{}", project.id), GraphNodeKind::Project)),
            );
            expected_graph_nodes.extend(self.environment.iter().map(|environment| {
                (
                    format!("environment:{}", environment.id),
                    GraphNodeKind::Environment,
                )
            }));
            expected_graph_nodes.extend(
                self.resources
                    .iter()
                    .map(|resource| (format!("resource:{}", resource.id), GraphNodeKind::Resource)),
            );
            let actual_graph_nodes = self
                .graph
                .nodes
                .iter()
                .map(|node| (node.id.clone(), node.kind))
                .collect::<BTreeMap<_, _>>();
            if expected_graph_nodes != actual_graph_nodes {
                return Err(WorkspaceError::Validation(
                    "workspace graph does not match aggregate children".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn transition(&mut self, next: WorkspaceState) -> WorkspaceResult<()> {
        if !self.state.can_transition_to(next) {
            return Err(WorkspaceError::InvalidState(format!(
                "{} -> {}",
                self.state.as_str(),
                next.as_str()
            )));
        }
        self.state = next;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Returns a local path only for `file:` Workspaces. Runtime identity remains URI-based.
    pub fn local_path(&self) -> Option<std::path::PathBuf> {
        Url::parse(&self.uri).ok()?.to_file_path().ok()
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceOpenRequest {
    pub name: String,
    pub uri: String,
    pub provider_key: Option<String>,
    pub actor: String,
    pub metadata: WorkspaceMetadata,
}

impl WorkspaceOpenRequest {
    pub fn new(name: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            uri: uri.into(),
            provider_key: None,
            actor: "system".into(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn local(name: impl Into<String>, path: impl AsRef<Path>) -> WorkspaceResult<Self> {
        let path = path.as_ref();
        let url = Url::from_directory_path(path).map_err(|_| {
            WorkspaceError::UnsupportedUri(format!(
                "cannot represent local path `{}` as a file URI",
                path.display()
            ))
        })?;
        let mut request = Self::new(name, url.to_string());
        request.provider_key = Some("local".into());
        Ok(request)
    }

    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = actor.into();
        self
    }

    pub fn validate(&self) -> WorkspaceResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(WorkspaceError::Validation(
                "workspace name must contain 1..=256 characters".into(),
            ));
        }
        let uri = Url::parse(&self.uri)
            .map_err(|error| WorkspaceError::Validation(format!("invalid URI: {error}")))?;
        if !uri.username().is_empty() || uri.password().is_some() {
            return Err(WorkspaceError::Validation(
                "workspace URI must not contain credentials".into(),
            ));
        }
        validate_actor(&self.actor)?;
        validate_metadata(&self.metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_accepts_only_declared_transitions() {
        assert!(WorkspaceState::Created.can_transition_to(WorkspaceState::Loaded));
        assert!(WorkspaceState::Ready.can_transition_to(WorkspaceState::Snapshot));
        assert!(WorkspaceState::Closed.can_transition_to(WorkspaceState::Loaded));
        assert!(!WorkspaceState::Created.can_transition_to(WorkspaceState::Ready));
        assert!(!WorkspaceState::Closed.can_transition_to(WorkspaceState::Modified));
    }

    #[test]
    fn secret_like_metadata_keys_are_rejected() {
        let metadata = BTreeMap::from([("api_token".into(), Value::String("x".into()))]);
        assert!(validate_metadata(&metadata).is_err());
    }

    #[test]
    fn open_request_rejects_embedded_credentials() {
        let request = WorkspaceOpenRequest::new("remote", "ssh://user:secret@example.com/repo");
        assert!(request.validate().is_err());
    }
}
