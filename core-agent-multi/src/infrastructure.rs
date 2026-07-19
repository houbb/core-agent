use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    AgentDescriptor, AgentMember, AgentMessage, AssignmentRequest, Collaboration,
    CollaborationBinding, CollaborationOutcome, Organization, Role, Team, TeamState,
};
use crate::error::MultiAgentResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiAgentOperation {
    CreateOrganization,
    CreateRole,
    CreateTeam,
    Join,
    Leave,
    Activate,
    Assign,
    Resume,
    Handover,
    Complete,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiAgentStage {
    Validation,
    Routing,
    Persistence,
    Dispatch,
    Outcome,
}

#[derive(Debug, Clone)]
pub struct MultiAgentObservation {
    pub operation: MultiAgentOperation,
    pub stage: MultiAgentStage,
    pub success: bool,
    pub team_id: Option<Uuid>,
    pub collaboration_id: Option<Uuid>,
    pub member_id: Option<Uuid>,
    pub actor: String,
    pub message: Option<String>,
}

pub trait MultiAgentObserver: Send + Sync {
    fn on_observation(&self, observation: &MultiAgentObservation);
}

pub trait MultiAgentInterceptor: Send + Sync {
    fn before_assignment(
        &self,
        _team: &Team,
        _request: &mut AssignmentRequest,
    ) -> MultiAgentResult<()> {
        Ok(())
    }
}

pub trait MultiAgentPolicy: Send + Sync {
    fn check(
        &self,
        operation: MultiAgentOperation,
        team: Option<&Team>,
        actor: &str,
    ) -> MultiAgentResult<()>;
}

pub trait TeamLifecycle: Send + Sync {
    fn transition(&self, from: TeamState, to: TeamState) -> MultiAgentResult<()>;
}

#[async_trait]
pub trait AgentDirectory: Send + Sync {
    async fn lookup(&self, agent_id: Uuid) -> MultiAgentResult<Option<AgentDescriptor>>;
}

#[derive(Debug, Clone)]
pub struct RoutingCandidate {
    pub member: AgentMember,
    pub role: Role,
    pub descriptor: AgentDescriptor,
}

#[async_trait]
pub trait AgentRouter: Send + Sync {
    async fn route(
        &self,
        team: &Team,
        request: &AssignmentRequest,
        candidates: &[RoutingCandidate],
    ) -> MultiAgentResult<Uuid>;
}

#[async_trait]
pub trait AgentDispatcher: Send + Sync {
    /// Prepare must be idempotent for the stable Collaboration dispatch ID and
    /// must not start an Agent Goal.
    async fn prepare(
        &self,
        collaboration: &Collaboration,
        member: &AgentMember,
        message: &AgentMessage,
    ) -> MultiAgentResult<CollaborationBinding>;

    /// Execute may have external effects. OutcomeUnknown must be returned when
    /// callers cannot prove whether the Agent accepted or completed the Goal.
    async fn execute(
        &self,
        binding: &CollaborationBinding,
        message: &AgentMessage,
    ) -> MultiAgentResult<CollaborationOutcome>;
}

#[derive(Debug, Clone)]
pub struct Versioned<T> {
    pub value: T,
    pub expected_version: Option<u64>,
}

impl<T> Versioned<T> {
    pub fn create(value: T) -> Self {
        Self {
            value,
            expected_version: None,
        }
    }

    pub fn update(value: T, expected_version: u64) -> Self {
        Self {
            value,
            expected_version: Some(expected_version),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CollaborationCommit {
    pub team: Versioned<Team>,
    pub collaboration: Versioned<Collaboration>,
    pub members: Vec<Versioned<AgentMember>>,
}

impl CollaborationCommit {
    pub fn validate(&self) -> MultiAgentResult<()> {
        self.team.value.validate()?;
        self.collaboration.value.validate()?;
        let mut ids = std::collections::BTreeSet::new();
        for member in &self.members {
            member.value.validate()?;
            if !ids.insert(member.value.id) {
                return Err(crate::error::MultiAgentError::Validation(
                    "collaboration commit contains duplicate Member updates".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait MultiAgentStore: Send + Sync {
    async fn save_organization(
        &self,
        value: &Organization,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()>;
    async fn find_organization(&self, id: Uuid) -> MultiAgentResult<Option<Organization>>;
    async fn find_organization_by_key(&self, key: &str) -> MultiAgentResult<Option<Organization>>;
    async fn list_organizations(&self) -> MultiAgentResult<Vec<Organization>>;

    async fn save_role(
        &self,
        value: &Role,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()>;
    async fn find_role(&self, id: Uuid) -> MultiAgentResult<Option<Role>>;
    async fn list_roles(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Role>>;

    async fn save_team(
        &self,
        value: &Team,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()>;
    async fn find_team(&self, id: Uuid) -> MultiAgentResult<Option<Team>>;
    async fn list_teams(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Team>>;

    async fn save_member(
        &self,
        value: &AgentMember,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()>;
    async fn find_member(&self, id: Uuid) -> MultiAgentResult<Option<AgentMember>>;
    async fn list_members(&self, team_id: Uuid) -> MultiAgentResult<Vec<AgentMember>>;

    async fn commit_collaboration(
        &self,
        commit: &CollaborationCommit,
        actor: &str,
    ) -> MultiAgentResult<()>;
    async fn find_collaboration(&self, id: Uuid) -> MultiAgentResult<Option<Collaboration>>;
    async fn list_collaborations(&self, team_id: Uuid) -> MultiAgentResult<Vec<Collaboration>>;
}
