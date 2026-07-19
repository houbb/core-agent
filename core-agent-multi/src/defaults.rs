use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    validate_actor, AgentAvailability, AgentDescriptor, AgentMember, Collaboration,
    CollaborationBinding, CollaborationOutcome, CollaborationState, MemberState, Organization,
    Role, Team, TeamState,
};
use crate::error::{MultiAgentError, MultiAgentResult};
use crate::infrastructure::{
    AgentDirectory, AgentDispatcher, AgentRouter, CollaborationCommit, MultiAgentInterceptor,
    MultiAgentOperation, MultiAgentPolicy, MultiAgentStage, MultiAgentStore, RoutingCandidate,
    TeamLifecycle,
};

#[derive(Default)]
pub struct EmbeddedMultiAgentPolicy;

impl MultiAgentPolicy for EmbeddedMultiAgentPolicy {
    fn check(
        &self,
        _operation: MultiAgentOperation,
        _team: Option<&Team>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)
    }
}

#[derive(Default)]
pub struct EmbeddedTeamLifecycle;

impl TeamLifecycle for EmbeddedTeamLifecycle {
    fn transition(&self, from: TeamState, to: TeamState) -> MultiAgentResult<()> {
        let allowed = matches!(
            (from, to),
            (TeamState::Created, TeamState::Ready)
                | (TeamState::Ready, TeamState::Active)
                | (TeamState::Active, TeamState::Ready)
                | (TeamState::Ready, TeamState::Completed)
                | (TeamState::Active, TeamState::Completed)
                | (TeamState::Completed, TeamState::Archived)
        );
        if !allowed {
            return Err(MultiAgentError::InvalidState(format!(
                "cannot transition Team from {} to {}",
                from.as_str(),
                to.as_str()
            )));
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopMultiAgentInterceptor;
impl MultiAgentInterceptor for NoopMultiAgentInterceptor {}

#[derive(Default)]
pub struct DeterministicAgentRouter;

#[async_trait]
impl AgentRouter for DeterministicAgentRouter {
    async fn route(
        &self,
        team: &Team,
        request: &crate::domain::AssignmentRequest,
        candidates: &[RoutingCandidate],
    ) -> MultiAgentResult<Uuid> {
        let mut eligible = candidates
            .iter()
            .filter(|candidate| {
                candidate.member.state.is_available()
                    && candidate.descriptor.availability == AgentAvailability::Available
                    && request
                        .role_id
                        .is_none_or(|role_id| candidate.role.id == role_id)
                    && request
                        .required_capabilities
                        .is_subset(&candidate.member.capabilities)
                    && request
                        .required_capabilities
                        .is_subset(&candidate.descriptor.capabilities)
                    && candidate
                        .role
                        .required_capabilities
                        .is_subset(&candidate.member.capabilities)
                    && candidate
                        .role
                        .required_capabilities
                        .is_subset(&candidate.descriptor.capabilities)
                    && team.workspace_id.is_none_or(|workspace_id| {
                        candidate.descriptor.workspace_id == Some(workspace_id)
                    })
            })
            .map(|candidate| candidate.member.id)
            .collect::<Vec<_>>();
        eligible.sort_unstable();
        eligible.into_iter().next().ok_or_else(|| {
            MultiAgentError::NoRoute(format!(
                "Team {} has no available Member for the requested role and capabilities",
                team.id
            ))
        })
    }
}

#[derive(Default)]
pub struct UnavailableAgentDirectory;

#[async_trait]
impl AgentDirectory for UnavailableAgentDirectory {
    async fn lookup(&self, _agent_id: Uuid) -> MultiAgentResult<Option<AgentDescriptor>> {
        Err(MultiAgentError::Extension(
            "AgentDirectory is not configured".into(),
        ))
    }
}

#[derive(Default)]
pub struct UnavailableAgentDispatcher;

#[async_trait]
impl AgentDispatcher for UnavailableAgentDispatcher {
    async fn prepare(
        &self,
        _collaboration: &Collaboration,
        _member: &AgentMember,
        _message: &crate::domain::AgentMessage,
    ) -> MultiAgentResult<CollaborationBinding> {
        Err(MultiAgentError::Extension(
            "AgentDispatcher is not configured".into(),
        ))
    }

    async fn execute(
        &self,
        _binding: &CollaborationBinding,
        _message: &crate::domain::AgentMessage,
    ) -> MultiAgentResult<CollaborationOutcome> {
        Err(MultiAgentError::Extension(
            "AgentDispatcher is not configured".into(),
        ))
    }
}

#[derive(Clone, Default)]
struct MemoryState {
    organizations: HashMap<Uuid, Organization>,
    roles: HashMap<Uuid, Role>,
    teams: HashMap<Uuid, Team>,
    members: HashMap<Uuid, AgentMember>,
    collaborations: HashMap<Uuid, Collaboration>,
}

#[derive(Default)]
pub struct InMemoryMultiAgentStore {
    state: RwLock<MemoryState>,
}

impl InMemoryMultiAgentStore {
    fn read(&self) -> MultiAgentResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| MultiAgentError::Internal("multi-agent store lock poisoned".into()))
    }

    fn write(&self) -> MultiAgentResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| MultiAgentError::Internal("multi-agent store lock poisoned".into()))
    }
}

#[async_trait]
impl MultiAgentStore for InMemoryMultiAgentStore {
    async fn save_organization(
        &self,
        value: &Organization,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        validate_organization_write(&state, value, expected_version)?;
        state.organizations.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_organization(&self, id: Uuid) -> MultiAgentResult<Option<Organization>> {
        Ok(self.read()?.organizations.get(&id).cloned())
    }

    async fn find_organization_by_key(&self, key: &str) -> MultiAgentResult<Option<Organization>> {
        Ok(self
            .read()?
            .organizations
            .values()
            .find(|value| value.key == key)
            .cloned())
    }

    async fn list_organizations(&self) -> MultiAgentResult<Vec<Organization>> {
        let mut values = self
            .read()?
            .organizations
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn save_role(
        &self,
        value: &Role,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        validate_role_write(&state, value, expected_version)?;
        state.roles.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_role(&self, id: Uuid) -> MultiAgentResult<Option<Role>> {
        Ok(self.read()?.roles.get(&id).cloned())
    }

    async fn list_roles(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Role>> {
        let mut values = self
            .read()?
            .roles
            .values()
            .filter(|value| value.organization_id == organization_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn save_team(
        &self,
        value: &Team,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        validate_team_write(&state, value, expected_version)?;
        state.teams.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_team(&self, id: Uuid) -> MultiAgentResult<Option<Team>> {
        Ok(self.read()?.teams.get(&id).cloned())
    }

    async fn list_teams(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Team>> {
        let mut values = self
            .read()?
            .teams
            .values()
            .filter(|value| value.organization_id == organization_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn save_member(
        &self,
        value: &AgentMember,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        validate_member_write(&state, value, expected_version)?;
        state.members.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_member(&self, id: Uuid) -> MultiAgentResult<Option<AgentMember>> {
        Ok(self.read()?.members.get(&id).cloned())
    }

    async fn list_members(&self, team_id: Uuid) -> MultiAgentResult<Vec<AgentMember>> {
        let mut values = self
            .read()?
            .members
            .values()
            .filter(|value| value.team_id == team_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.role_id, value.id));
        Ok(values)
    }

    async fn commit_collaboration(
        &self,
        commit: &CollaborationCommit,
        actor: &str,
    ) -> MultiAgentResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        validate_team_write(&next, &commit.team.value, commit.team.expected_version)?;
        validate_collaboration_write(
            &next,
            &commit.collaboration.value,
            commit.collaboration.expected_version,
        )?;
        for member in &commit.members {
            validate_member_write(&next, &member.value, member.expected_version)?;
        }
        validate_collaboration_relations(&next, commit)?;
        next.teams
            .insert(commit.team.value.id, commit.team.value.clone());
        next.collaborations.insert(
            commit.collaboration.value.id,
            commit.collaboration.value.clone(),
        );
        for member in &commit.members {
            next.members.insert(member.value.id, member.value.clone());
        }
        *state = next;
        Ok(())
    }

    async fn find_collaboration(&self, id: Uuid) -> MultiAgentResult<Option<Collaboration>> {
        Ok(self.read()?.collaborations.get(&id).cloned())
    }

    async fn list_collaborations(&self, team_id: Uuid) -> MultiAgentResult<Vec<Collaboration>> {
        let mut values = self
            .read()?
            .collaborations
            .values()
            .filter(|value| value.team_id == team_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.created_at), value.id));
        Ok(values)
    }
}

fn validate_create_or_update<T>(
    current: Option<&T>,
    expected_version: Option<u64>,
    next_version: u64,
) -> MultiAgentResult<()>
where
    T: VersionedValue,
{
    match (current, expected_version) {
        (None, None) if next_version == 1 => Ok(()),
        (Some(current), Some(expected))
            if current.version() == expected && next_version == expected.saturating_add(1) =>
        {
            Ok(())
        }
        _ => Err(MultiAgentError::Conflict(
            "multi-agent optimistic version conflict".into(),
        )),
    }
}

trait VersionedValue {
    fn version(&self) -> u64;
}

macro_rules! impl_versioned {
    ($($name:ty),+ $(,)?) => {$(
        impl VersionedValue for $name {
            fn version(&self) -> u64 { self.version }
        }
    )+};
}
impl_versioned!(Organization, Role, Team, AgentMember, Collaboration);

fn validate_organization_write(
    state: &MemoryState,
    value: &Organization,
    expected: Option<u64>,
) -> MultiAgentResult<()> {
    validate_create_or_update(state.organizations.get(&value.id), expected, value.version)?;
    if state
        .organizations
        .values()
        .any(|current| current.id != value.id && current.key == value.key)
    {
        return Err(MultiAgentError::Conflict(
            "organization key already exists".into(),
        ));
    }
    if let Some(current) = state.organizations.get(&value.id) {
        if current.id != value.id
            || current.key != value.key
            || current.created_at != value.created_at
        {
            return Err(MultiAgentError::Conflict(
                "organization immutable identity changed".into(),
            ));
        }
    }
    Ok(())
}

fn validate_role_write(
    state: &MemoryState,
    value: &Role,
    expected: Option<u64>,
) -> MultiAgentResult<()> {
    if !state.organizations.contains_key(&value.organization_id) {
        return Err(MultiAgentError::not_found(value.organization_id));
    }
    validate_create_or_update(state.roles.get(&value.id), expected, value.version)?;
    if state.roles.values().any(|current| {
        current.id != value.id
            && current.organization_id == value.organization_id
            && current.key == value.key
    }) {
        return Err(MultiAgentError::Conflict("role key already exists".into()));
    }
    if let Some(current) = state.roles.get(&value.id) {
        if current.organization_id != value.organization_id
            || current.key != value.key
            || current.created_at != value.created_at
        {
            return Err(MultiAgentError::Conflict(
                "role immutable identity changed".into(),
            ));
        }
    }
    Ok(())
}

fn validate_team_write(
    state: &MemoryState,
    value: &Team,
    expected: Option<u64>,
) -> MultiAgentResult<()> {
    if !state.organizations.contains_key(&value.organization_id) {
        return Err(MultiAgentError::not_found(value.organization_id));
    }
    validate_create_or_update(state.teams.get(&value.id), expected, value.version)?;
    if state.teams.values().any(|current| {
        current.id != value.id
            && current.organization_id == value.organization_id
            && current.key == value.key
    }) {
        return Err(MultiAgentError::Conflict("team key already exists".into()));
    }
    if let Some(current) = state.teams.get(&value.id) {
        if current.organization_id != value.organization_id
            || current.key != value.key
            || current.created_at != value.created_at
            || current.workspace_id != value.workspace_id
            || current.memory_scope != value.memory_scope
        {
            return Err(MultiAgentError::Conflict(
                "team immutable identity or shared references changed".into(),
            ));
        }
    }
    Ok(())
}

fn validate_member_write(
    state: &MemoryState,
    value: &AgentMember,
    expected: Option<u64>,
) -> MultiAgentResult<()> {
    let team = state
        .teams
        .get(&value.team_id)
        .ok_or_else(|| MultiAgentError::not_found(value.team_id))?;
    let role = state
        .roles
        .get(&value.role_id)
        .ok_or_else(|| MultiAgentError::not_found(value.role_id))?;
    if role.organization_id != team.organization_id {
        return Err(MultiAgentError::Validation(
            "member Role and Team belong to different Organizations".into(),
        ));
    }
    validate_create_or_update(state.members.get(&value.id), expected, value.version)?;
    if state.members.values().any(|current| {
        current.id != value.id
            && current.team_id == value.team_id
            && current.agent_id == value.agent_id
    }) {
        return Err(MultiAgentError::Conflict(
            "Agent is already a Team Member".into(),
        ));
    }
    if let Some(current) = state.members.get(&value.id) {
        if current.team_id != value.team_id
            || current.role_id != value.role_id
            || current.agent_id != value.agent_id
            || current.created_at != value.created_at
            || current.capabilities != value.capabilities
        {
            return Err(MultiAgentError::Conflict(
                "member immutable identity or capability snapshot changed".into(),
            ));
        }
    }
    Ok(())
}

fn validate_collaboration_write(
    state: &MemoryState,
    value: &Collaboration,
    expected: Option<u64>,
) -> MultiAgentResult<()> {
    validate_create_or_update(state.collaborations.get(&value.id), expected, value.version)?;
    if let Some(current) = state.collaborations.get(&value.id) {
        if current.team_id != value.team_id
            || current.created_at != value.created_at
            || current.goal != value.goal
            || current.required_capabilities != value.required_capabilities
            || current.source_member_id != value.source_member_id
        {
            return Err(MultiAgentError::Conflict(
                "collaboration immutable request changed".into(),
            ));
        }
    }
    Ok(())
}

fn validate_collaboration_relations(
    state: &MemoryState,
    commit: &CollaborationCommit,
) -> MultiAgentResult<()> {
    let team = &commit.team.value;
    let collaboration = &commit.collaboration.value;
    if collaboration.team_id != team.id {
        return Err(MultiAgentError::Validation(
            "collaboration does not belong to committed Team".into(),
        ));
    }
    let target = commit
        .members
        .iter()
        .find(|member| member.value.id == collaboration.target_member_id)
        .map(|member| &member.value)
        .ok_or_else(|| {
            MultiAgentError::Validation(
                "collaboration commit does not include its target Member".into(),
            )
        })?;
    let expected_owner = if collaboration.state.is_terminal() {
        None
    } else {
        Some(collaboration.id)
    };
    if target.team_id != team.id
        || collaboration.role_id.is_some_and(|id| id != target.role_id)
        || target.current_collaboration_id != expected_owner
        || collaboration.source_member_id.is_some_and(|id| {
            state
                .members
                .get(&id)
                .is_none_or(|member| member.team_id != team.id)
        })
    {
        return Err(MultiAgentError::Validation(
            "collaboration Team, Role, source or target ownership is invalid".into(),
        ));
    }
    let expected_member = match collaboration.state {
        CollaborationState::Assigned => MemberState::Assigned,
        CollaborationState::Working | CollaborationState::OutcomeUnknown => MemberState::Working,
        CollaborationState::Waiting => MemberState::Waiting,
        CollaborationState::Completed => MemberState::Completed,
        CollaborationState::Failed | CollaborationState::Cancelled => MemberState::Available,
    };
    let expected_team = if matches!(
        collaboration.state,
        CollaborationState::Completed | CollaborationState::Failed | CollaborationState::Cancelled
    ) {
        TeamState::Ready
    } else {
        TeamState::Active
    };
    if target.state != expected_member || team.state != expected_team {
        return Err(MultiAgentError::Validation(
            "Team or Member state does not match Collaboration state".into(),
        ));
    }
    Ok(())
}

pub(crate) fn notify_safely(
    observers: &[std::sync::Arc<dyn crate::infrastructure::MultiAgentObserver>],
    observation: &crate::infrastructure::MultiAgentObservation,
) {
    for observer in observers {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            observer.on_observation(observation)
        }));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn observation(
    operation: MultiAgentOperation,
    stage: MultiAgentStage,
    success: bool,
    team_id: Option<Uuid>,
    collaboration_id: Option<Uuid>,
    member_id: Option<Uuid>,
    actor: &str,
    message: Option<String>,
) -> crate::infrastructure::MultiAgentObservation {
    crate::infrastructure::MultiAgentObservation {
        operation,
        stage,
        success,
        team_id,
        collaboration_id,
        member_id,
        actor: actor.into(),
        message,
    }
}
