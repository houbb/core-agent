use std::fs;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use core_agent_workspace::{
    InMemoryWorkspaceCatalog, LocalResourceProvider, LocalWorkspaceSnapshot, ResourceManager,
    ScanOptions, Snapshot, SnapshotOptions, SqliteWorkspaceStore, Workspace, WorkspaceCatalog,
    WorkspaceError, WorkspaceInterceptor, WorkspaceManager, WorkspaceObservation,
    WorkspaceObserver, WorkspaceOpenRequest, WorkspaceOperation, WorkspacePolicy, WorkspaceResult,
    WorkspaceState,
};
use tempfile::tempdir;
use uuid::Uuid;

fn rust_workspace(root: &std::path::Path) {
    fs::write(root.join("Cargo.toml"), "[package]\nname='demo'").unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn original() {}").unwrap();
    fs::write(root.join("README.md"), "# Demo").unwrap();
}

#[tokio::test]
async fn open_detect_index_search_and_sqlite_round_trip() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    fs::create_dir(directory.path().join(".git")).unwrap();
    let store = Arc::new(SqliteWorkspaceStore::new(":memory:").unwrap());
    let manager = WorkspaceManager::new(store.clone());

    let workspace = manager
        .open(
            WorkspaceOpenRequest::local("demo", directory.path())
                .unwrap()
                .actor("e2e"),
        )
        .await
        .unwrap();

    assert_eq!(workspace.state, WorkspaceState::Ready);
    assert_eq!(workspace.projects.len(), 1);
    assert_eq!(workspace.projects[0].kind.as_str(), "RUST");
    assert!(workspace
        .environment
        .as_ref()
        .unwrap()
        .package_managers
        .contains("cargo"));
    assert_eq!(
        workspace.environment.as_ref().unwrap().git.as_deref(),
        Some("repository")
    );
    assert!(workspace
        .resources
        .iter()
        .any(|resource| resource.name == "lib.rs"));
    assert!(!workspace
        .resources
        .iter()
        .any(|resource| resource.name == ".git"));
    assert_eq!(
        workspace.graph.nodes.len(),
        1 + workspace.projects.len() + 1 + workspace.resources.len()
    );

    let hits = manager.search(workspace.id, "readme", 10).await.unwrap();
    assert_eq!(hits[0].label, "README.md");
    assert!(hits[0].score >= 80);

    let cold_reader = WorkspaceManager::new(store);
    let persisted = cold_reader.find(workspace.id).await.unwrap().unwrap();
    assert_eq!(persisted.resources.len(), workspace.resources.len());
    assert_eq!(persisted.projects.len(), workspace.projects.len());
    assert_eq!(persisted.graph.nodes.len(), workspace.graph.nodes.len());
    assert_eq!(manager.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn reload_replaces_removed_resources_and_indexes_new_files() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder().build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await
        .unwrap();

    fs::remove_file(directory.path().join("README.md")).unwrap();
    fs::write(directory.path().join("CHANGELOG.md"), "new").unwrap();
    let modified = manager.mark_modified(workspace.id, "e2e").await.unwrap();
    assert_eq!(modified.state, WorkspaceState::Modified);

    let reloaded = manager.reload(workspace.id, "e2e").await.unwrap();
    assert_eq!(reloaded.state, WorkspaceState::Ready);
    assert!(!reloaded
        .resources
        .iter()
        .any(|resource| resource.name == "README.md"));
    assert!(reloaded
        .resources
        .iter()
        .any(|resource| resource.name == "CHANGELOG.md"));
    assert_eq!(
        manager.search(workspace.id, "changelog", 1).await.unwrap()[0].label,
        "CHANGELOG.md"
    );
}

#[tokio::test]
async fn snapshot_restore_overlays_without_deleting_new_files() {
    let directory = tempdir().unwrap();
    let snapshot_root = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .snapshotter(Arc::new(LocalWorkspaceSnapshot::new(
            snapshot_root.path(),
            SnapshotOptions::default(),
        )))
        .build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await
        .unwrap();

    let snapshot = manager
        .snapshot(workspace.id, "before edit", "e2e")
        .await
        .unwrap();
    fs::write(directory.path().join("src/lib.rs"), "pub fn changed() {}").unwrap();
    fs::write(directory.path().join("after.txt"), "keep").unwrap();

    let restored = manager.restore(snapshot.id, "e2e").await.unwrap();
    assert_eq!(restored.state, WorkspaceState::Ready);
    assert_eq!(
        fs::read_to_string(directory.path().join("src/lib.rs")).unwrap(),
        "pub fn original() {}"
    );
    assert_eq!(
        fs::read_to_string(directory.path().join("after.txt")).unwrap(),
        "keep"
    );
    assert_eq!(manager.list_snapshots(workspace.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn closed_workspace_reopens_with_stable_identity() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let database = directory.path().join("workspace.db");
    let first_store = Arc::new(SqliteWorkspaceStore::new(database.to_str().unwrap()).unwrap());
    let first_manager = WorkspaceManager::new(first_store);
    let request = WorkspaceOpenRequest::local("demo", directory.path()).unwrap();
    let opened = first_manager.open(request.clone()).await.unwrap();
    let closed = first_manager.close(opened.id, "e2e").await.unwrap();
    assert_eq!(closed.state, WorkspaceState::Closed);
    drop(first_manager);

    let second_store = Arc::new(SqliteWorkspaceStore::new(database.to_str().unwrap()).unwrap());
    let second_manager = WorkspaceManager::new(second_store);
    assert_eq!(
        second_manager.find(opened.id).await.unwrap().unwrap().state,
        WorkspaceState::Closed
    );
    let reopened = second_manager.open(request).await.unwrap();
    assert_eq!(reopened.id, opened.id);
    assert_eq!(reopened.state, WorkspaceState::Ready);
}

struct DenyClose;

impl WorkspacePolicy for DenyClose {
    fn evaluate(
        &self,
        operation: WorkspaceOperation,
        _workspace: Option<&Workspace>,
    ) -> WorkspaceResult<()> {
        if operation == WorkspaceOperation::Close {
            Err(WorkspaceError::PolicyDenied("close disabled".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn policy_denial_prevents_state_change() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .policy(Arc::new(DenyClose))
        .build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await
        .unwrap();
    assert!(matches!(
        manager.close(workspace.id, "e2e").await,
        Err(WorkspaceError::PolicyDenied(_))
    ));
    assert_eq!(
        manager.find(workspace.id).await.unwrap().unwrap().state,
        WorkspaceState::Ready
    );
}

struct LabelInterceptor;

impl WorkspaceInterceptor for LabelInterceptor {
    fn before_open(&self, request: &mut WorkspaceOpenRequest) -> WorkspaceResult<()> {
        request.name = "intercepted".into();
        Ok(())
    }
}

struct IdentityMutatingInterceptor;

impl WorkspaceInterceptor for IdentityMutatingInterceptor {
    fn after_load(&self, workspace: &mut Workspace) -> WorkspaceResult<()> {
        workspace.uri = "file:///different/".into();
        Ok(())
    }
}

struct CountingObserver(Arc<Mutex<Vec<WorkspaceOperation>>>);

impl WorkspaceObserver for CountingObserver {
    fn observe(&self, observation: &WorkspaceObservation) {
        self.0.lock().unwrap().push(observation.operation);
    }
}

struct PanickingObserver;

impl WorkspaceObserver for PanickingObserver {
    fn observe(&self, _observation: &WorkspaceObservation) {
        panic!("observer failure must be isolated");
    }
}

#[tokio::test]
async fn interceptor_and_observer_extensions_are_composable_and_panic_safe() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let operations = Arc::new(Mutex::new(Vec::new()));
    let manager = WorkspaceManager::builder()
        .interceptor(Arc::new(LabelInterceptor))
        .observer(Arc::new(PanickingObserver))
        .observer(Arc::new(CountingObserver(operations.clone())))
        .build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("original", directory.path()).unwrap())
        .await
        .unwrap();
    assert_eq!(workspace.name, "intercepted");
    manager.search(workspace.id, "lib", 5).await.unwrap();
    assert_eq!(
        *operations.lock().unwrap(),
        vec![WorkspaceOperation::Open, WorkspaceOperation::Search]
    );
}

#[tokio::test]
async fn after_load_interceptor_cannot_change_workspace_identity() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .interceptor(Arc::new(IdentityMutatingInterceptor))
        .build();
    let result = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await;
    assert!(matches!(result, Err(WorkspaceError::Validation(_))));
    assert!(manager.list().await.unwrap().is_empty());
}

struct DenyResolvedOpen;

impl WorkspacePolicy for DenyResolvedOpen {
    fn evaluate(
        &self,
        operation: WorkspaceOperation,
        workspace: Option<&Workspace>,
    ) -> WorkspaceResult<()> {
        if operation == WorkspaceOperation::Open && workspace.is_some() {
            Err(WorkspaceError::PolicyDenied("resolved URI denied".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn open_policy_can_evaluate_canonical_workspace() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .policy(Arc::new(DenyResolvedOpen))
        .build();
    assert!(matches!(
        manager
            .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
            .await,
        Err(WorkspaceError::PolicyDenied(_))
    ));
    assert!(manager.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_snapshot_state_fails_before_catalog_mutation() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder().build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await
        .unwrap();
    manager.close(workspace.id, "e2e").await.unwrap();
    assert!(matches!(
        manager.snapshot(workspace.id, "closed", "e2e").await,
        Err(WorkspaceError::InvalidState(_))
    ));
    assert!(manager
        .list_snapshots(workspace.id)
        .await
        .unwrap()
        .is_empty());
}

#[derive(Default)]
struct RejectSnapshotCatalog {
    inner: InMemoryWorkspaceCatalog,
}

#[async_trait]
impl WorkspaceCatalog for RejectSnapshotCatalog {
    async fn save_workspace(&self, workspace: &Workspace, actor: &str) -> WorkspaceResult<()> {
        self.inner.save_workspace(workspace, actor).await
    }

    async fn find_workspace(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        self.inner.find_workspace(id).await
    }

    async fn find_workspace_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>> {
        self.inner.find_workspace_by_uri(uri).await
    }

    async fn list_workspaces(&self) -> WorkspaceResult<Vec<Workspace>> {
        self.inner.list_workspaces().await
    }

    async fn remove_workspace(&self, id: Uuid) -> WorkspaceResult<bool> {
        self.inner.remove_workspace(id).await
    }

    async fn save_snapshot(&self, _snapshot: &Snapshot, _actor: &str) -> WorkspaceResult<()> {
        Err(WorkspaceError::Internal("injected snapshot failure".into()))
    }

    async fn find_snapshot(&self, id: Uuid) -> WorkspaceResult<Option<Snapshot>> {
        self.inner.find_snapshot(id).await
    }

    async fn list_snapshots(&self, workspace_id: Uuid) -> WorkspaceResult<Vec<Snapshot>> {
        self.inner.list_snapshots(workspace_id).await
    }

    async fn remove_snapshot(&self, id: Uuid) -> WorkspaceResult<bool> {
        self.inner.remove_snapshot(id).await
    }
}

#[tokio::test]
async fn snapshot_catalog_failure_discards_copied_files() {
    let directory = tempdir().unwrap();
    let snapshot_root = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .catalog(Arc::new(RejectSnapshotCatalog::default()))
        .snapshotter(Arc::new(LocalWorkspaceSnapshot::new(
            snapshot_root.path(),
            SnapshotOptions::default(),
        )))
        .build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await
        .unwrap();
    assert!(manager
        .snapshot(workspace.id, "must roll back", "e2e")
        .await
        .is_err());
    assert!(fs::read_dir(snapshot_root.path()).unwrap().next().is_none());
    assert!(manager
        .list_snapshots(workspace.id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        manager.find(workspace.id).await.unwrap().unwrap().state,
        WorkspaceState::Ready
    );
}

#[tokio::test]
async fn resource_scan_limit_fails_before_catalog_mutation() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .resource_manager(ResourceManager::new(vec![Arc::new(
            LocalResourceProvider::new(ScanOptions {
                max_resources: 1,
                ..ScanOptions::default()
            }),
        )]))
        .build();
    let result = manager
        .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
        .await;
    assert!(matches!(result, Err(WorkspaceError::LimitExceeded(_))));
    assert!(manager.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn resource_depth_limit_is_explicit_instead_of_silent_truncation() {
    let directory = tempdir().unwrap();
    rust_workspace(directory.path());
    let manager = WorkspaceManager::builder()
        .resource_manager(ResourceManager::new(vec![Arc::new(
            LocalResourceProvider::new(ScanOptions {
                max_depth: 1,
                ..ScanOptions::default()
            }),
        )]))
        .build();
    assert!(matches!(
        manager
            .open(WorkspaceOpenRequest::local("demo", directory.path()).unwrap())
            .await,
        Err(WorkspaceError::LimitExceeded(_))
    ));
    assert!(manager.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn empty_directory_is_a_valid_generic_workspace() {
    let directory = tempdir().unwrap();
    let manager = WorkspaceManager::builder().build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("empty", directory.path()).unwrap())
        .await
        .unwrap();
    assert_eq!(workspace.state, WorkspaceState::Ready);
    assert_eq!(workspace.projects.len(), 1);
    assert_eq!(workspace.projects[0].kind.as_str(), "GENERIC");
    assert!(workspace.resources.is_empty());
    assert!(workspace.environment.is_some());
}
