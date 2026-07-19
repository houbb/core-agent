use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, RwLock};

use uuid::Uuid;

use crate::domain::{
    Environment, Project, Resource, Snapshot, Workspace, WorkspaceOpenRequest, WorkspaceSearchHit,
    WorkspaceState,
};
use crate::error::{WorkspaceError, WorkspaceResult};
use crate::infrastructure::{
    AllowAllWorkspacePolicy, DefaultWorkspaceLifecycle, DynWorkspaceProvider, EnvironmentDetector,
    InMemoryWorkspaceCatalog, InMemoryWorkspaceRegistry, ProjectScanner, ResourceProvider,
    WorkspaceCatalog, WorkspaceIndexer, WorkspaceInterceptor, WorkspaceLifecycle,
    WorkspaceObservation, WorkspaceObserver, WorkspaceOperation, WorkspacePolicy,
    WorkspaceProvider, WorkspaceRegistry, WorkspaceSnapshot,
};
use crate::providers::{
    LocalEnvironmentDetector, LocalProjectScanner, LocalResourceProvider, LocalWorkspaceIndexer,
    LocalWorkspaceProvider, LocalWorkspaceSnapshot,
};

pub struct ResourceManager {
    providers: Vec<Arc<dyn ResourceProvider>>,
}

impl ResourceManager {
    pub fn new(providers: Vec<Arc<dyn ResourceProvider>>) -> Self {
        Self { providers }
    }

    pub async fn scan(&self, workspace: &Workspace) -> WorkspaceResult<Vec<Resource>> {
        let mut by_uri = BTreeMap::new();
        for provider in self
            .providers
            .iter()
            .filter(|provider| provider.supports(workspace))
        {
            for resource in provider.scan(workspace).await? {
                resource.validate()?;
                if by_uri.insert(resource.uri.clone(), resource).is_some() {
                    return Err(WorkspaceError::Conflict(
                        "multiple resource providers returned the same URI".into(),
                    ));
                }
            }
        }
        Ok(by_uri.into_values().collect())
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new(vec![Arc::new(LocalResourceProvider::default())])
    }
}

pub struct ProjectManager {
    scanners: Vec<Arc<dyn ProjectScanner>>,
}

impl ProjectManager {
    pub fn new(scanners: Vec<Arc<dyn ProjectScanner>>) -> Self {
        Self { scanners }
    }

    pub async fn scan(
        &self,
        workspace: &Workspace,
        resources: &[Resource],
    ) -> WorkspaceResult<Vec<Project>> {
        let mut by_uri = BTreeMap::new();
        for scanner in &self.scanners {
            for project in scanner.scan(workspace, resources).await? {
                project.validate()?;
                by_uri.entry(project.root_uri.clone()).or_insert(project);
            }
        }
        Ok(by_uri.into_values().collect())
    }

    pub async fn refresh(
        &self,
        workspace: &Workspace,
        resources: &[Resource],
    ) -> WorkspaceResult<Vec<Project>> {
        self.scan(workspace, resources).await
    }
}

impl Default for ProjectManager {
    fn default() -> Self {
        Self::new(vec![Arc::new(LocalProjectScanner)])
    }
}

pub struct EnvironmentManager {
    detectors: Vec<Arc<dyn EnvironmentDetector>>,
}

impl EnvironmentManager {
    pub fn new(detectors: Vec<Arc<dyn EnvironmentDetector>>) -> Self {
        Self { detectors }
    }

    pub async fn detect(
        &self,
        workspace: &Workspace,
        projects: &[Project],
        resources: &[Resource],
    ) -> WorkspaceResult<Environment> {
        let mut detected: Option<Environment> = None;
        for detector in &self.detectors {
            let contribution = detector.detect(workspace, projects, resources).await?;
            if let Some(current) = &mut detected {
                current.merge(contribution)?;
            } else {
                detected = Some(contribution);
            }
        }
        detected.ok_or_else(|| {
            WorkspaceError::Validation("no environment detector is configured".into())
        })
    }
}

impl Default for EnvironmentManager {
    fn default() -> Self {
        Self::new(vec![Arc::new(LocalEnvironmentDetector)])
    }
}

pub struct WorkspaceManagerBuilder {
    catalog: Arc<dyn WorkspaceCatalog>,
    registry: Arc<dyn WorkspaceRegistry>,
    providers: Vec<DynWorkspaceProvider>,
    resource_manager: ResourceManager,
    project_manager: ProjectManager,
    environment_manager: EnvironmentManager,
    snapshotter: Arc<dyn WorkspaceSnapshot>,
    indexer: Arc<dyn WorkspaceIndexer>,
    lifecycle: Arc<dyn WorkspaceLifecycle>,
    policy: Arc<dyn WorkspacePolicy>,
    interceptors: Vec<Arc<dyn WorkspaceInterceptor>>,
    observers: Vec<Arc<dyn WorkspaceObserver>>,
}

impl Default for WorkspaceManagerBuilder {
    fn default() -> Self {
        Self {
            catalog: Arc::new(InMemoryWorkspaceCatalog::default()),
            registry: Arc::new(InMemoryWorkspaceRegistry::default()),
            providers: vec![Arc::new(LocalWorkspaceProvider)],
            resource_manager: ResourceManager::default(),
            project_manager: ProjectManager::default(),
            environment_manager: EnvironmentManager::default(),
            snapshotter: Arc::new(LocalWorkspaceSnapshot::default()),
            indexer: Arc::new(LocalWorkspaceIndexer),
            lifecycle: Arc::new(DefaultWorkspaceLifecycle),
            policy: Arc::new(AllowAllWorkspacePolicy),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl WorkspaceManagerBuilder {
    pub fn catalog(mut self, value: Arc<dyn WorkspaceCatalog>) -> Self {
        self.catalog = value;
        self
    }

    pub fn registry(mut self, value: Arc<dyn WorkspaceRegistry>) -> Self {
        self.registry = value;
        self
    }

    pub fn workspace_provider(mut self, value: Arc<dyn WorkspaceProvider>) -> Self {
        self.providers.push(value);
        self
    }

    pub fn resource_manager(mut self, value: ResourceManager) -> Self {
        self.resource_manager = value;
        self
    }

    pub fn project_manager(mut self, value: ProjectManager) -> Self {
        self.project_manager = value;
        self
    }

    pub fn environment_manager(mut self, value: EnvironmentManager) -> Self {
        self.environment_manager = value;
        self
    }

    pub fn snapshotter(mut self, value: Arc<dyn WorkspaceSnapshot>) -> Self {
        self.snapshotter = value;
        self
    }

    pub fn indexer(mut self, value: Arc<dyn WorkspaceIndexer>) -> Self {
        self.indexer = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn WorkspaceLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn WorkspacePolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn WorkspaceInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn WorkspaceObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> WorkspaceManager {
        let mut providers = BTreeMap::new();
        for provider in self.providers {
            providers.insert(provider.key().to_string(), provider);
        }
        WorkspaceManager {
            catalog: self.catalog,
            registry: self.registry,
            providers: RwLock::new(providers),
            resource_manager: self.resource_manager,
            project_manager: self.project_manager,
            environment_manager: self.environment_manager,
            snapshotter: self.snapshotter,
            indexer: self.indexer,
            lifecycle: self.lifecycle,
            policy: self.policy,
            interceptors: self.interceptors,
            observers: self.observers,
        }
    }
}

pub struct WorkspaceManager {
    catalog: Arc<dyn WorkspaceCatalog>,
    registry: Arc<dyn WorkspaceRegistry>,
    providers: RwLock<BTreeMap<String, DynWorkspaceProvider>>,
    resource_manager: ResourceManager,
    project_manager: ProjectManager,
    environment_manager: EnvironmentManager,
    snapshotter: Arc<dyn WorkspaceSnapshot>,
    indexer: Arc<dyn WorkspaceIndexer>,
    lifecycle: Arc<dyn WorkspaceLifecycle>,
    policy: Arc<dyn WorkspacePolicy>,
    interceptors: Vec<Arc<dyn WorkspaceInterceptor>>,
    observers: Vec<Arc<dyn WorkspaceObserver>>,
}

impl WorkspaceManager {
    pub fn builder() -> WorkspaceManagerBuilder {
        WorkspaceManagerBuilder::default()
    }

    pub fn new(catalog: Arc<dyn WorkspaceCatalog>) -> Self {
        Self::builder().catalog(catalog).build()
    }

    pub fn register_provider(&self, provider: DynWorkspaceProvider) -> WorkspaceResult<()> {
        let key = provider.key().trim();
        if key.is_empty() {
            return Err(WorkspaceError::Validation(
                "workspace provider key cannot be empty".into(),
            ));
        }
        self.providers
            .write()
            .map_err(|_| WorkspaceError::Internal("provider registry lock poisoned".into()))?
            .insert(key.to_string(), provider);
        Ok(())
    }

    pub async fn open(&self, mut request: WorkspaceOpenRequest) -> WorkspaceResult<Workspace> {
        request.validate()?;
        self.policy.evaluate(WorkspaceOperation::Open, None)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_open(&mut request)))
                .map_err(|_| WorkspaceError::Internal("workspace interceptor panicked".into()))??;
        }
        request.validate()?;
        let provider = self.provider_for(&request)?;
        let loaded = provider.load(&request).await?;
        loaded.validate()?;
        if loaded.provider_key != provider.key() {
            return Err(WorkspaceError::Validation(format!(
                "provider `{}` returned workspace owned by `{}`",
                provider.key(),
                loaded.provider_key
            )));
        }
        self.policy
            .evaluate(WorkspaceOperation::Open, Some(&loaded))?;

        if let Some(live) = self.registry.find_by_uri(&loaded.uri)? {
            self.policy
                .evaluate(WorkspaceOperation::Open, Some(&live))?;
            return Ok(live);
        }
        let mut workspace =
            if let Some(mut existing) = self.catalog.find_workspace_by_uri(&loaded.uri).await? {
                existing.name = request.name.clone();
                existing.metadata = request.metadata.clone();
                existing.provider_key = loaded.provider_key;
                existing.uri = loaded.uri;
                existing.updated_at = chrono::Utc::now();
                existing
            } else {
                loaded
            };
        self.ensure_loaded(&mut workspace)?;
        self.refresh_content(&mut workspace).await?;
        self.catalog
            .save_workspace(&workspace, &request.actor)
            .await?;
        self.registry.register(workspace.clone())?;
        self.notify(WorkspaceOperation::Open, Some(&workspace));
        Ok(workspace)
    }

    pub async fn reload(&self, id: Uuid, actor: &str) -> WorkspaceResult<Workspace> {
        crate::domain::validate_actor(actor)?;
        let mut workspace = self.required(id).await?;
        self.policy
            .evaluate(WorkspaceOperation::Reload, Some(&workspace))?;
        self.ensure_loaded(&mut workspace)?;
        self.refresh_content(&mut workspace).await?;
        self.catalog.save_workspace(&workspace, actor).await?;
        self.registry.register(workspace.clone())?;
        self.notify(WorkspaceOperation::Reload, Some(&workspace));
        Ok(workspace)
    }

    pub async fn mark_modified(&self, id: Uuid, actor: &str) -> WorkspaceResult<Workspace> {
        crate::domain::validate_actor(actor)?;
        let mut workspace = self.required(id).await?;
        self.policy
            .evaluate(WorkspaceOperation::MarkModified, Some(&workspace))?;
        if workspace.state != WorkspaceState::Modified {
            self.lifecycle
                .transition(&mut workspace, WorkspaceState::Modified)?;
        }
        self.catalog.save_workspace(&workspace, actor).await?;
        self.registry.register(workspace.clone())?;
        self.notify(WorkspaceOperation::MarkModified, Some(&workspace));
        Ok(workspace)
    }

    pub async fn snapshot(&self, id: Uuid, label: &str, actor: &str) -> WorkspaceResult<Snapshot> {
        crate::domain::validate_actor(actor)?;
        let mut workspace = self.required(id).await?;
        self.policy
            .evaluate(WorkspaceOperation::Snapshot, Some(&workspace))?;
        let original = workspace.clone();
        let mut transitioned = workspace.clone();
        self.lifecycle
            .transition(&mut transitioned, WorkspaceState::Snapshot)?;
        let snapshot = self.snapshotter.create(&workspace, label, actor).await?;
        if let Err(error) = self.catalog.save_snapshot(&snapshot, actor).await {
            self.discard_after_failure(&snapshot, Some(snapshot.id))
                .await?;
            return Err(error);
        }
        workspace = transitioned;
        if let Err(error) = self.catalog.save_workspace(&workspace, actor).await {
            self.discard_after_failure(&snapshot, Some(snapshot.id))
                .await?;
            return Err(error);
        }
        if let Err(error) = self.registry.register(workspace.clone()) {
            let compensation = async {
                self.catalog.save_workspace(&original, actor).await?;
                self.discard_after_failure(&snapshot, Some(snapshot.id))
                    .await
            }
            .await;
            if let Err(compensation_error) = compensation {
                return Err(WorkspaceError::Internal(format!(
                    "snapshot registry commit failed: {error}; compensation failed: {compensation_error}"
                )));
            }
            return Err(error);
        }
        self.notify(WorkspaceOperation::Snapshot, Some(&workspace));
        Ok(snapshot)
    }

    pub async fn restore(&self, snapshot_id: Uuid, actor: &str) -> WorkspaceResult<Workspace> {
        crate::domain::validate_actor(actor)?;
        let snapshot = self
            .catalog
            .find_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| WorkspaceError::NotFound(snapshot_id.to_string()))?;
        let mut workspace = self.required(snapshot.workspace_id).await?;
        self.policy
            .evaluate(WorkspaceOperation::Restore, Some(&workspace))?;
        self.snapshotter
            .restore(&workspace, &snapshot, actor)
            .await?;
        self.ensure_loaded(&mut workspace)?;
        self.refresh_content(&mut workspace).await?;
        self.catalog.save_workspace(&workspace, actor).await?;
        self.registry.register(workspace.clone())?;
        self.notify(WorkspaceOperation::Restore, Some(&workspace));
        Ok(workspace)
    }

    pub async fn close(&self, id: Uuid, actor: &str) -> WorkspaceResult<Workspace> {
        crate::domain::validate_actor(actor)?;
        let mut workspace = self.required(id).await?;
        self.policy
            .evaluate(WorkspaceOperation::Close, Some(&workspace))?;
        if workspace.state != WorkspaceState::Closed {
            self.lifecycle
                .transition(&mut workspace, WorkspaceState::Closed)?;
            self.catalog.save_workspace(&workspace, actor).await?;
        }
        self.registry.remove(id)?;
        self.notify(WorkspaceOperation::Close, Some(&workspace));
        Ok(workspace)
    }

    pub async fn find(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        if let Some(workspace) = self.registry.find(id)? {
            return Ok(Some(workspace));
        }
        self.catalog.find_workspace(id).await
    }

    pub async fn list(&self) -> WorkspaceResult<Vec<Workspace>> {
        self.catalog.list_workspaces().await
    }

    pub async fn list_snapshots(&self, id: Uuid) -> WorkspaceResult<Vec<Snapshot>> {
        self.catalog.list_snapshots(id).await
    }

    pub async fn search(
        &self,
        id: Uuid,
        query: &str,
        limit: usize,
    ) -> WorkspaceResult<Vec<WorkspaceSearchHit>> {
        let workspace = self.required(id).await?;
        self.policy
            .evaluate(WorkspaceOperation::Search, Some(&workspace))?;
        let result = self.indexer.search(&workspace.graph, query, limit).await?;
        self.notify(WorkspaceOperation::Search, Some(&workspace));
        Ok(result)
    }

    fn provider_for(
        &self,
        request: &WorkspaceOpenRequest,
    ) -> WorkspaceResult<DynWorkspaceProvider> {
        let providers = self
            .providers
            .read()
            .map_err(|_| WorkspaceError::Internal("provider registry lock poisoned".into()))?;
        if let Some(key) = &request.provider_key {
            return providers
                .get(key)
                .cloned()
                .ok_or_else(|| WorkspaceError::ProviderNotFound(key.clone()));
        }
        providers
            .values()
            .find(|provider| provider.supports(&request.uri))
            .cloned()
            .ok_or_else(|| WorkspaceError::ProviderNotFound(request.uri.clone()))
    }

    async fn required(&self, id: Uuid) -> WorkspaceResult<Workspace> {
        self.find(id)
            .await?
            .ok_or_else(|| WorkspaceError::NotFound(id.to_string()))
    }

    fn ensure_loaded(&self, workspace: &mut Workspace) -> WorkspaceResult<()> {
        if workspace.state != WorkspaceState::Loaded {
            self.lifecycle
                .transition(workspace, WorkspaceState::Loaded)?;
        }
        Ok(())
    }

    async fn refresh_content(&self, workspace: &mut Workspace) -> WorkspaceResult<()> {
        let mut resources = self.resource_manager.scan(workspace).await?;
        let projects = self.project_manager.refresh(workspace, &resources).await?;
        for resource in &mut resources {
            resource.project_id = projects
                .iter()
                .filter(|project| resource.uri.starts_with(&project.root_uri))
                .max_by_key(|project| project.root_uri.len())
                .map(|project| project.id);
        }
        let environment = self
            .environment_manager
            .detect(workspace, &projects, &resources)
            .await?;
        workspace.projects = projects;
        workspace.resources = resources;
        workspace.environment = Some(environment);
        let identity = (
            workspace.id,
            workspace.provider_key.clone(),
            workspace.uri.clone(),
            workspace.state,
            workspace.created_at,
        );
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.after_load(workspace)))
                .map_err(|_| WorkspaceError::Internal("workspace interceptor panicked".into()))??;
        }
        if identity
            != (
                workspace.id,
                workspace.provider_key.clone(),
                workspace.uri.clone(),
                workspace.state,
                workspace.created_at,
            )
        {
            return Err(WorkspaceError::Validation(
                "after_load interceptor changed immutable workspace identity or lifecycle".into(),
            ));
        }
        workspace.graph = self.indexer.build(workspace).await?;
        self.lifecycle
            .transition(workspace, WorkspaceState::Ready)?;
        workspace.validate()
    }

    async fn discard_after_failure(
        &self,
        snapshot: &Snapshot,
        catalog_id: Option<Uuid>,
    ) -> WorkspaceResult<()> {
        let catalog_result = if let Some(id) = catalog_id {
            self.catalog.remove_snapshot(id).await.map(|_| ())
        } else {
            Ok(())
        };
        let storage_result = self.snapshotter.discard(snapshot).await;
        match (catalog_result, storage_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(catalog), Err(storage)) => Err(WorkspaceError::Internal(format!(
                "snapshot catalog cleanup failed: {catalog}; storage cleanup failed: {storage}"
            ))),
        }
    }

    fn notify(&self, operation: WorkspaceOperation, workspace: Option<&Workspace>) {
        let observation = WorkspaceObservation::new(operation, workspace);
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.observe(&observation)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DenySearch;

    impl WorkspacePolicy for DenySearch {
        fn evaluate(
            &self,
            operation: WorkspaceOperation,
            _workspace: Option<&Workspace>,
        ) -> WorkspaceResult<()> {
            if operation == WorkspaceOperation::Search {
                Err(WorkspaceError::PolicyDenied("search disabled".into()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn builder_accepts_replaceable_policy() {
        let _manager = WorkspaceManager::builder()
            .policy(Arc::new(DenySearch))
            .build();
    }
}
