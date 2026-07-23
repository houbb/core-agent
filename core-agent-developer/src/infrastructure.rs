use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{AgentTestRun, DeveloperProject, DeveloperProfile, PublishRequest};
use crate::error::DeveloperResult;

/// Developer Profile Store.
#[async_trait]
pub trait DeveloperProfileStore: Send + Sync {
    /// Store a developer profile.
    async fn store(&self, profile: &DeveloperProfile) -> DeveloperResult<()>;

    /// Find a developer profile by subject.
    async fn find_by_subject(&self, tenant_id: Uuid, subject: &str) -> DeveloperResult<Option<DeveloperProfile>>;

    /// Find a developer profile by ID.
    async fn find(&self, id: Uuid) -> DeveloperResult<Option<DeveloperProfile>>;
}

/// Developer Project Store.
#[async_trait]
pub trait DeveloperProjectStore: Send + Sync {
    /// Store a project.
    async fn store(&self, project: &DeveloperProject) -> DeveloperResult<()>;

    /// Find a project by ID.
    async fn find(&self, id: Uuid) -> DeveloperResult<Option<DeveloperProject>>;

    /// List all projects for a developer.
    async fn list_by_developer(&self, developer_id: Uuid) -> DeveloperResult<Vec<DeveloperProject>>;

    /// List all projects for a tenant.
    async fn list_by_tenant(&self, tenant_id: Uuid) -> DeveloperResult<Vec<DeveloperProject>>;

    /// Update a project's state.
    async fn update(&self, project: &DeveloperProject) -> DeveloperResult<()>;
}

/// Publisher — interface for publishing agents to the marketplace.
#[async_trait]
pub trait Publisher: Send + Sync {
    /// Publish an agent package.
    async fn publish(&self, request: &PublishRequest, actor: &str) -> DeveloperResult<()>;
}

/// Test Runner — interface for testing agents.
#[async_trait]
pub trait TestRunner: Send + Sync {
    /// Run a test against an agent.
    async fn run_test(&self, project_id: Uuid, input: &str, actor: &str) -> DeveloperResult<AgentTestRun>;
}