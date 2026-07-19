use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    Environment, Project, Resource, Snapshot, Workspace, WorkspaceGraph, WorkspaceOpenRequest,
    WorkspaceSearchHit, WorkspaceState,
};
use crate::error::WorkspaceResult;

#[async_trait]
pub trait WorkspaceProvider: Send + Sync {
    fn key(&self) -> &str;
    fn supports(&self, uri: &str) -> bool;
    async fn load(&self, request: &WorkspaceOpenRequest) -> WorkspaceResult<Workspace>;
}

#[async_trait]
pub trait ResourceProvider: Send + Sync {
    fn key(&self) -> &str;
    fn supports(&self, workspace: &Workspace) -> bool;
    async fn scan(&self, workspace: &Workspace) -> WorkspaceResult<Vec<Resource>>;
}

#[async_trait]
pub trait ProjectScanner: Send + Sync {
    fn key(&self) -> &str;
    async fn scan(
        &self,
        workspace: &Workspace,
        resources: &[Resource],
    ) -> WorkspaceResult<Vec<Project>>;
}

#[async_trait]
pub trait EnvironmentDetector: Send + Sync {
    fn key(&self) -> &str;
    async fn detect(
        &self,
        workspace: &Workspace,
        projects: &[Project],
        resources: &[Resource],
    ) -> WorkspaceResult<Environment>;
}

#[async_trait]
pub trait WorkspaceSnapshot: Send + Sync {
    async fn create(
        &self,
        workspace: &Workspace,
        label: &str,
        actor: &str,
    ) -> WorkspaceResult<Snapshot>;

    /// Restores snapshot files as an overlay. Implementations must not delete
    /// resources that were created after the snapshot unless a future explicit
    /// destructive mode is introduced.
    async fn restore(
        &self,
        workspace: &Workspace,
        snapshot: &Snapshot,
        actor: &str,
    ) -> WorkspaceResult<()>;

    /// Removes snapshot bodies after a failed metadata commit or explicit cleanup.
    async fn discard(&self, snapshot: &Snapshot) -> WorkspaceResult<()>;
}

#[async_trait]
pub trait WorkspaceIndexer: Send + Sync {
    async fn build(&self, workspace: &Workspace) -> WorkspaceResult<WorkspaceGraph>;
    async fn search(
        &self,
        graph: &WorkspaceGraph,
        query: &str,
        limit: usize,
    ) -> WorkspaceResult<Vec<WorkspaceSearchHit>>;
}

pub trait WorkspaceLifecycle: Send + Sync {
    fn transition(&self, workspace: &mut Workspace, next: WorkspaceState) -> WorkspaceResult<()>;
}

pub trait WorkspaceRegistry: Send + Sync {
    fn register(&self, workspace: Workspace) -> WorkspaceResult<()>;
    fn find(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>>;
    fn find_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>>;
    fn list(&self) -> WorkspaceResult<Vec<Workspace>>;
    fn remove(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>>;
}

#[async_trait]
pub trait WorkspaceCatalog: Send + Sync {
    async fn save_workspace(&self, workspace: &Workspace, actor: &str) -> WorkspaceResult<()>;
    async fn find_workspace(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>>;
    async fn find_workspace_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>>;
    async fn list_workspaces(&self) -> WorkspaceResult<Vec<Workspace>>;
    async fn remove_workspace(&self, id: Uuid) -> WorkspaceResult<bool>;

    async fn save_snapshot(&self, snapshot: &Snapshot, actor: &str) -> WorkspaceResult<()>;
    async fn find_snapshot(&self, id: Uuid) -> WorkspaceResult<Option<Snapshot>>;
    async fn list_snapshots(&self, workspace_id: Uuid) -> WorkspaceResult<Vec<Snapshot>>;
    async fn remove_snapshot(&self, id: Uuid) -> WorkspaceResult<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceOperation {
    Open,
    Reload,
    MarkModified,
    Snapshot,
    Restore,
    Close,
    Search,
}

#[derive(Debug, Clone)]
pub struct WorkspaceObservation {
    pub operation: WorkspaceOperation,
    pub workspace_id: Option<Uuid>,
    pub state: Option<WorkspaceState>,
    pub project_count: usize,
    pub resource_count: usize,
    pub occurred_at: DateTime<Utc>,
}

impl WorkspaceObservation {
    pub fn new(operation: WorkspaceOperation, workspace: Option<&Workspace>) -> Self {
        Self {
            operation,
            workspace_id: workspace.map(|value| value.id),
            state: workspace.map(|value| value.state),
            project_count: workspace.map_or(0, |value| value.projects.len()),
            resource_count: workspace.map_or(0, |value| value.resources.len()),
            occurred_at: Utc::now(),
        }
    }
}

pub trait WorkspaceObserver: Send + Sync {
    fn observe(&self, observation: &WorkspaceObservation);
}

pub trait WorkspacePolicy: Send + Sync {
    fn evaluate(
        &self,
        operation: WorkspaceOperation,
        workspace: Option<&Workspace>,
    ) -> WorkspaceResult<()>;
}

pub trait WorkspaceInterceptor: Send + Sync {
    fn before_open(&self, _request: &mut WorkspaceOpenRequest) -> WorkspaceResult<()> {
        Ok(())
    }

    fn after_load(&self, _workspace: &mut Workspace) -> WorkspaceResult<()> {
        Ok(())
    }
}

pub(crate) type DynWorkspaceProvider = Arc<dyn WorkspaceProvider>;
