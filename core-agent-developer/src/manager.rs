use std::sync::Arc;

use uuid::Uuid;

use crate::domain::{
    AgentManifest, AgentTestRun, DeveloperDashboard, DeveloperProfile, DeveloperProject,
    ProjectState, PublishRequest, TestStatus,
};
use crate::error::{DeveloperError, DeveloperResult};
use crate::infrastructure::{
    DeveloperProfileStore, DeveloperProjectStore, Publisher, TestRunner,
};

/// Developer Manager — orchestrates the developer portal.
pub struct DeveloperManager {
    profile_store: Arc<dyn DeveloperProfileStore>,
    project_store: Arc<dyn DeveloperProjectStore>,
    publisher: Arc<dyn Publisher>,
    test_runner: Arc<dyn TestRunner>,
}

impl DeveloperManager {
    pub fn new(
        profile_store: Arc<dyn DeveloperProfileStore>,
        project_store: Arc<dyn DeveloperProjectStore>,
        publisher: Arc<dyn Publisher>,
        test_runner: Arc<dyn TestRunner>,
    ) -> Self {
        Self {
            profile_store,
            project_store,
            publisher,
            test_runner,
        }
    }

    /// Register a new developer profile.
    pub async fn register_profile(
        &self,
        profile: DeveloperProfile,
    ) -> DeveloperResult<DeveloperProfile> {
        profile.validate()?;
        let existing = self
            .profile_store
            .find_by_subject(profile.tenant_id, &profile.subject)
            .await?;
        if existing.is_some() {
            return Err(DeveloperError::Conflict(
                "developer profile already exists".into(),
            ));
        }
        self.profile_store.store(&profile).await?;
        Ok(profile)
    }

    /// Create a new development project.
    pub async fn create_project(
        &self,
        developer_id: Uuid,
        tenant_id: Uuid,
        name: &str,
        manifest: AgentManifest,
        actor: &str,
    ) -> DeveloperResult<DeveloperProject> {
        let profile = self
            .profile_store
            .find(developer_id)
            .await?
            .ok_or_else(|| DeveloperError::NotFound("developer not found".into()))?;
        if profile.tenant_id != tenant_id {
            return Err(DeveloperError::Denied("tenant mismatch".into()));
        }
        let project = DeveloperProject::new(tenant_id, developer_id, name, manifest, actor);
        project.validate()?;
        self.project_store.store(&project).await?;
        Ok(project)
    }

    /// Publish an agent to the marketplace.
    pub async fn publish_agent(
        &self,
        project_id: Uuid,
        manifest: AgentManifest,
        actor: &str,
    ) -> DeveloperResult<()> {
        manifest.validate()?;
        let project = self
            .project_store
            .find(project_id)
            .await?
            .ok_or_else(|| DeveloperError::NotFound("project not found".into()))?;
        if project.actor != actor {
            return Err(DeveloperError::Denied(
                "only the project owner can publish".into(),
            ));
        }
        let request = PublishRequest::new(project_id, manifest, serde_json::Value::Null);
        self.publisher.publish(&request, actor).await
    }

    /// Run a test against an agent.
    pub async fn test_agent(
        &self,
        project_id: Uuid,
        input: &str,
        actor: &str,
    ) -> DeveloperResult<AgentTestRun> {
        let project = self
            .project_store
            .find(project_id)
            .await?
            .ok_or_else(|| DeveloperError::NotFound("project not found".into()))?;
        if project.state != ProjectState::Active {
            return Err(DeveloperError::Validation("project is not active".into()));
        }
        self.test_runner.run_test(project_id, input, actor).await
    }

    /// Archive a project.
    pub async fn archive_project(
        &self,
        project_id: Uuid,
        actor: &str,
    ) -> DeveloperResult<DeveloperProject> {
        let mut project = self
            .project_store
            .find(project_id)
            .await?
            .ok_or_else(|| DeveloperError::NotFound("project not found".into()))?;
        if project.actor != actor {
            return Err(DeveloperError::Denied(
                "only the project owner can archive".into(),
            ));
        }
        project.archive(actor);
        self.project_store.update(&project).await?;
        Ok(project)
    }

    /// Get developer dashboard.
    pub async fn dashboard(
        &self,
        developer_id: Uuid,
    ) -> DeveloperResult<DeveloperDashboard> {
        let projects = self.project_store.list_by_developer(developer_id).await?;
        let total_projects = projects.len() as u64;
        let published = projects
            .iter()
            .filter(|p| p.state == ProjectState::Active)
            .count() as u64;

        Ok(DeveloperDashboard {
            total_projects,
            published_agents: published,
            total_downloads: 0,
            average_rating: 0.0,
            recent_runs: Vec::new(),
            api_keys_count: 0,
        })
    }

    /// List projects for a tenant.
    pub async fn list_projects(
        &self,
        tenant_id: Uuid,
    ) -> DeveloperResult<Vec<DeveloperProject>> {
        self.project_store.list_by_tenant(tenant_id).await
    }
}

// ── Default In-Memory Implementations ─────────────────────────────────────

#[derive(Default)]
pub struct InMemoryDeveloperProfileStore {
    profiles: std::sync::RwLock<Vec<DeveloperProfile>>,
}

#[async_trait::async_trait]
impl DeveloperProfileStore for InMemoryDeveloperProfileStore {
    async fn store(&self, profile: &DeveloperProfile) -> DeveloperResult<()> {
        let mut profiles = self.profiles.write().map_err(|_| {
            DeveloperError::Internal("profile store lock poisoned".into())
        })?;
        if profiles.iter().any(|p| p.id == profile.id) {
            return Err(DeveloperError::Conflict("profile already exists".into()));
        }
        profiles.push(profile.clone());
        Ok(())
    }

    async fn find_by_subject(
        &self,
        tenant_id: Uuid,
        subject: &str,
    ) -> DeveloperResult<Option<DeveloperProfile>> {
        let profiles = self.profiles.read().map_err(|_| {
            DeveloperError::Internal("profile store lock poisoned".into())
        })?;
        Ok(profiles
            .iter()
            .find(|p| p.tenant_id == tenant_id && p.subject == subject)
            .cloned())
    }

    async fn find(&self, id: Uuid) -> DeveloperResult<Option<DeveloperProfile>> {
        let profiles = self.profiles.read().map_err(|_| {
            DeveloperError::Internal("profile store lock poisoned".into())
        })?;
        Ok(profiles.iter().find(|p| p.id == id).cloned())
    }
}

#[derive(Default)]
pub struct InMemoryDeveloperProjectStore {
    projects: std::sync::RwLock<Vec<DeveloperProject>>,
}

#[async_trait::async_trait]
impl DeveloperProjectStore for InMemoryDeveloperProjectStore {
    async fn store(&self, project: &DeveloperProject) -> DeveloperResult<()> {
        let mut projects = self.projects.write().map_err(|_| {
            DeveloperError::Internal("project store lock poisoned".into())
        })?;
        if projects.iter().any(|p| p.id == project.id) {
            return Err(DeveloperError::Conflict("project already exists".into()));
        }
        projects.push(project.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> DeveloperResult<Option<DeveloperProject>> {
        let projects = self.projects.read().map_err(|_| {
            DeveloperError::Internal("project store lock poisoned".into())
        })?;
        Ok(projects.iter().find(|p| p.id == id).cloned())
    }

    async fn list_by_developer(&self, developer_id: Uuid) -> DeveloperResult<Vec<DeveloperProject>> {
        let projects = self.projects.read().map_err(|_| {
            DeveloperError::Internal("project store lock poisoned".into())
        })?;
        Ok(projects
            .iter()
            .filter(|p| p.developer_id == developer_id)
            .cloned()
            .collect())
    }

    async fn list_by_tenant(&self, tenant_id: Uuid) -> DeveloperResult<Vec<DeveloperProject>> {
        let projects = self.projects.read().map_err(|_| {
            DeveloperError::Internal("project store lock poisoned".into())
        })?;
        Ok(projects
            .iter()
            .filter(|p| p.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn update(&self, project: &DeveloperProject) -> DeveloperResult<()> {
        let mut projects = self.projects.write().map_err(|_| {
            DeveloperError::Internal("project store lock poisoned".into())
        })?;
        if let Some(existing) = projects.iter_mut().find(|p| p.id == project.id) {
            *existing = project.clone();
            Ok(())
        } else {
            Err(DeveloperError::NotFound(project.id.to_string()))
        }
    }
}

#[derive(Default)]
pub struct NoopPublisher;

#[async_trait::async_trait]
impl Publisher for NoopPublisher {
    async fn publish(&self, _request: &PublishRequest, _actor: &str) -> DeveloperResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopTestRunner;

#[async_trait::async_trait]
impl TestRunner for NoopTestRunner {
    async fn run_test(
        &self,
        project_id: Uuid,
        input: &str,
        actor: &str,
    ) -> DeveloperResult<AgentTestRun> {
        Ok(AgentTestRun::new(
            project_id,
            input,
            "mock output",
            TestStatus::Passed,
            actor,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn register_and_find_profile() {
        let store = Arc::new(InMemoryDeveloperProfileStore::default());
        let manager = DeveloperManager::new(
            store,
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "alice", "Alice", "alice@example.com");
        let saved = manager.register_profile(profile).await.unwrap();
        assert_eq!(saved.subject, "alice");
    }

    #[tokio::test]
    async fn duplicate_profile_fails() {
        let store = Arc::new(InMemoryDeveloperProfileStore::default());
        let manager = DeveloperManager::new(
            store,
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "alice", "Alice", "alice@example.com");
        manager.register_profile(profile).await.unwrap();
        let dup = DeveloperProfile::new(tenant_id, "alice", "Alice", "alice@example.com");
        assert!(manager.register_profile(dup).await.is_err());
    }

    #[tokio::test]
    async fn create_and_list_projects() {
        let manager = DeveloperManager::new(
            Arc::new(InMemoryDeveloperProfileStore::default()),
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "bob", "Bob", "bob@example.com");
        let profile = manager.register_profile(profile).await.unwrap();

        let manifest = AgentManifest {
            name: "my-agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };

        let project = manager
            .create_project(profile.id, tenant_id, "my-project", manifest, "bob")
            .await
            .unwrap();
        assert_eq!(project.name, "my-project");

        let projects = manager.list_projects(tenant_id).await.unwrap();
        assert_eq!(projects.len(), 1);
    }

    #[tokio::test]
    async fn archive_project() {
        let manager = DeveloperManager::new(
            Arc::new(InMemoryDeveloperProfileStore::default()),
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "carol", "Carol", "carol@example.com");
        let profile = manager.register_profile(profile).await.unwrap();

        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };

        let project = manager
            .create_project(profile.id, tenant_id, "proj", manifest, "carol")
            .await
            .unwrap();
        let archived = manager.archive_project(project.id, "carol").await.unwrap();
        assert_eq!(archived.state, ProjectState::Archived);
    }

    #[tokio::test]
    async fn test_agent() {
        let manager = DeveloperManager::new(
            Arc::new(InMemoryDeveloperProfileStore::default()),
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "dave", "Dave", "dave@example.com");
        let profile = manager.register_profile(profile).await.unwrap();

        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };

        let project = manager
            .create_project(profile.id, tenant_id, "proj", manifest, "dave")
            .await
            .unwrap();

        let run = manager.test_agent(project.id, "test input", "dave").await.unwrap();
        assert_eq!(run.status, TestStatus::Passed);
    }

    #[tokio::test]
    async fn dashboard_shows_metrics() {
        let manager = DeveloperManager::new(
            Arc::new(InMemoryDeveloperProfileStore::default()),
            Arc::new(InMemoryDeveloperProjectStore::default()),
            Arc::new(NoopPublisher),
            Arc::new(NoopTestRunner),
        );

        let tenant_id = Uuid::new_v4();
        let profile = DeveloperProfile::new(tenant_id, "eve", "Eve", "eve@example.com");
        let profile = manager.register_profile(profile).await.unwrap();

        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };

        manager
            .create_project(profile.id, tenant_id, "proj", manifest, "eve")
            .await
            .unwrap();

        let dashboard = manager.dashboard(profile.id).await.unwrap();
        assert_eq!(dashboard.total_projects, 1);
        assert_eq!(dashboard.published_agents, 1);
    }
}