use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{
    notify_safely, observation, DeterministicAgentRouter, EmbeddedMultiAgentPolicy,
    EmbeddedTeamLifecycle, InMemoryMultiAgentStore, UnavailableAgentDirectory,
    UnavailableAgentDispatcher,
};
use crate::domain::{
    validate_actor, AgentAvailability, AgentMember, AgentMessage, AssignmentRequest, Collaboration,
    CollaborationOutcome, CollaborationState, CreateTeamRequest, MemberState, Organization, Role,
    Team, TeamState,
};
use crate::error::{MultiAgentError, MultiAgentResult};
use crate::infrastructure::{
    AgentDirectory, AgentDispatcher, AgentRouter, CollaborationCommit, MultiAgentInterceptor,
    MultiAgentObserver, MultiAgentOperation, MultiAgentPolicy, MultiAgentStage, MultiAgentStore,
    RoutingCandidate, TeamLifecycle, Versioned,
};

pub struct MultiAgentManagerBuilder {
    store: Arc<dyn MultiAgentStore>,
    directory: Arc<dyn AgentDirectory>,
    router: Arc<dyn AgentRouter>,
    dispatcher: Arc<dyn AgentDispatcher>,
    policy: Arc<dyn MultiAgentPolicy>,
    lifecycle: Arc<dyn TeamLifecycle>,
    interceptors: Vec<Arc<dyn MultiAgentInterceptor>>,
    observers: Vec<Arc<dyn MultiAgentObserver>>,
}

impl Default for MultiAgentManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryMultiAgentStore::default()),
            directory: Arc::new(UnavailableAgentDirectory),
            router: Arc::new(DeterministicAgentRouter),
            dispatcher: Arc::new(UnavailableAgentDispatcher),
            policy: Arc::new(EmbeddedMultiAgentPolicy),
            lifecycle: Arc::new(EmbeddedTeamLifecycle),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl MultiAgentManagerBuilder {
    pub fn store(mut self, value: Arc<dyn MultiAgentStore>) -> Self {
        self.store = value;
        self
    }

    pub fn directory(mut self, value: Arc<dyn AgentDirectory>) -> Self {
        self.directory = value;
        self
    }

    pub fn router(mut self, value: Arc<dyn AgentRouter>) -> Self {
        self.router = value;
        self
    }

    pub fn dispatcher(mut self, value: Arc<dyn AgentDispatcher>) -> Self {
        self.dispatcher = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn MultiAgentPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn TeamLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn MultiAgentInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn MultiAgentObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> MultiAgentManager {
        MultiAgentManager {
            store: self.store,
            directory: self.directory,
            router: self.router,
            dispatcher: self.dispatcher,
            policy: self.policy,
            lifecycle: self.lifecycle,
            interceptors: self.interceptors,
            observers: self.observers,
            live_teams: Mutex::new(HashMap::new()),
        }
    }
}

pub struct MultiAgentManager {
    store: Arc<dyn MultiAgentStore>,
    directory: Arc<dyn AgentDirectory>,
    router: Arc<dyn AgentRouter>,
    dispatcher: Arc<dyn AgentDispatcher>,
    policy: Arc<dyn MultiAgentPolicy>,
    lifecycle: Arc<dyn TeamLifecycle>,
    interceptors: Vec<Arc<dyn MultiAgentInterceptor>>,
    observers: Vec<Arc<dyn MultiAgentObserver>>,
    live_teams: Mutex<HashMap<Uuid, Uuid>>,
}

impl MultiAgentManager {
    pub fn builder() -> MultiAgentManagerBuilder {
        MultiAgentManagerBuilder::default()
    }

    pub async fn create_organization(&self, value: Organization) -> MultiAgentResult<Organization> {
        value.validate()?;
        self.policy
            .check(MultiAgentOperation::CreateOrganization, None, &value.actor)?;
        if self
            .store
            .find_organization_by_key(&value.key)
            .await?
            .is_some()
        {
            return Err(MultiAgentError::Conflict(format!(
                "Organization key {} already exists",
                value.key
            )));
        }
        self.store
            .save_organization(&value, None, &value.actor)
            .await?;
        Ok(value)
    }

    pub async fn create_role(&self, value: Role) -> MultiAgentResult<Role> {
        value.validate()?;
        self.required_organization(value.organization_id).await?;
        self.policy
            .check(MultiAgentOperation::CreateRole, None, &value.actor)?;
        self.store.save_role(&value, None, &value.actor).await?;
        Ok(value)
    }

    pub async fn create_team(&self, request: CreateTeamRequest) -> MultiAgentResult<Team> {
        validate_actor(&request.actor)?;
        self.required_organization(request.organization_id).await?;
        let mut team = Team::new(
            request.organization_id,
            request.key,
            request.name,
            request.goal,
            request.actor,
        );
        team.workspace_id = request.workspace_id;
        team.memory_scope = request.memory_scope;
        team.policy = request.policy;
        team.metadata = request.metadata;
        team.validate()?;
        self.policy
            .check(MultiAgentOperation::CreateTeam, Some(&team), &team.actor)?;
        self.store.save_team(&team, None, &team.actor).await?;
        Ok(team)
    }

    pub async fn join(
        &self,
        team_id: Uuid,
        role_id: Uuid,
        agent_id: Uuid,
        actor: &str,
    ) -> MultiAgentResult<AgentMember> {
        validate_actor(actor)?;
        let team = self.required_team(team_id).await?;
        self.policy
            .check(MultiAgentOperation::Join, Some(&team), actor)?;
        if !matches!(team.state, TeamState::Created | TeamState::Ready) {
            return Err(MultiAgentError::InvalidState(format!(
                "cannot join {} Team",
                team.state.as_str()
            )));
        }
        let role = self.required_role(role_id).await?;
        if role.organization_id != team.organization_id {
            return Err(MultiAgentError::Validation(
                "Role and Team belong to different Organizations".into(),
            ));
        }
        let active_members = self
            .store
            .list_members(team_id)
            .await?
            .into_iter()
            .filter(|member| member.state != MemberState::Left)
            .count();
        if active_members >= team.policy.max_members as usize {
            return Err(MultiAgentError::Denied(
                "Team member limit has been reached".into(),
            ));
        }
        let descriptor = self
            .directory
            .lookup(agent_id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(agent_id))?;
        descriptor.validate()?;
        if !role
            .required_capabilities
            .is_subset(&descriptor.capabilities)
        {
            return Err(MultiAgentError::Denied(
                "Agent does not satisfy Role capabilities".into(),
            ));
        }
        let member = AgentMember::new(team_id, role_id, agent_id, descriptor.capabilities, actor);
        self.store.save_member(&member, None, actor).await?;
        Ok(member)
    }

    pub async fn leave(&self, member_id: Uuid, actor: &str) -> MultiAgentResult<AgentMember> {
        validate_actor(actor)?;
        let mut member = self.required_member(member_id).await?;
        let team = self.required_team(member.team_id).await?;
        self.policy
            .check(MultiAgentOperation::Leave, Some(&team), actor)?;
        if member.state.owns_collaboration() {
            return Err(MultiAgentError::InvalidState(
                "cannot remove a Member with active Collaboration".into(),
            ));
        }
        if member.state == MemberState::Left {
            return Ok(member);
        }
        let expected = member.version;
        member.state = MemberState::Left;
        advance_member(&mut member, actor);
        self.store
            .save_member(&member, Some(expected), actor)
            .await?;
        Ok(member)
    }

    pub async fn activate_team(&self, id: Uuid, actor: &str) -> MultiAgentResult<Team> {
        validate_actor(actor)?;
        let mut team = self.required_team(id).await?;
        self.policy
            .check(MultiAgentOperation::Activate, Some(&team), actor)?;
        if self
            .store
            .list_members(id)
            .await?
            .into_iter()
            .all(|member| member.state == MemberState::Left)
        {
            return Err(MultiAgentError::Validation(
                "Team requires at least one active Member".into(),
            ));
        }
        self.transition_team(&mut team, TeamState::Ready, actor)?;
        let expected = team.version - 1;
        self.store.save_team(&team, Some(expected), actor).await?;
        Ok(team)
    }

    pub async fn assign(&self, mut request: AssignmentRequest) -> MultiAgentResult<Collaboration> {
        request.validate()?;
        let original_team = request.team_id;
        let original_actor = request.actor.clone();
        let team = self.required_team(request.team_id).await?;
        self.policy
            .check(MultiAgentOperation::Assign, Some(&team), &request.actor)?;
        if team.state != TeamState::Ready {
            return Err(MultiAgentError::InvalidState(format!(
                "cannot assign work while Team is {}",
                team.state.as_str()
            )));
        }
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.before_assignment(&team, &mut request)
            }))
            .map_err(|_| MultiAgentError::Extension("multi-agent interceptor panicked".into()))??;
        }
        request.validate()?;
        if request.team_id != original_team || request.actor != original_actor {
            return Err(MultiAgentError::Validation(
                "interceptor changed assignment Team or actor".into(),
            ));
        }
        let candidates = self.routing_candidates(&team).await?;
        let member_id = self.router.route(&team, &request, &candidates).await?;
        let member = candidates
            .into_iter()
            .find(|candidate| candidate.member.id == member_id)
            .map(|candidate| candidate.member)
            .ok_or_else(|| {
                MultiAgentError::Validation("AgentRouter returned an unknown Member".into())
            })?;
        self.enter_team(team.id, Uuid::nil())?;
        let result = self.create_and_drive(team, member, request).await;
        self.leave_team(original_team)?;
        result
    }

    async fn create_and_drive(
        &self,
        mut team: Team,
        mut member: AgentMember,
        request: AssignmentRequest,
    ) -> MultiAgentResult<Collaboration> {
        let mut collaboration = Collaboration::new(
            team.id,
            request.role_id,
            request.source_member_id,
            member.id,
            request.goal,
            request.required_capabilities,
            request.priority,
            request.actor.clone(),
        )?;
        self.replace_live_collaboration(team.id, collaboration.id)?;
        let team_expected = team.version;
        self.transition_team(&mut team, TeamState::Active, &request.actor)?;
        let member_expected = member.version;
        member.state = MemberState::Assigned;
        member.current_collaboration_id = Some(collaboration.id);
        advance_member(&mut member, &request.actor);
        self.store
            .commit_collaboration(
                &CollaborationCommit {
                    team: Versioned::update(team.clone(), team_expected),
                    collaboration: Versioned::create(collaboration.clone()),
                    members: vec![Versioned::update(member.clone(), member_expected)],
                },
                &request.actor,
            )
            .await?;
        self.notify(
            MultiAgentOperation::Assign,
            MultiAgentStage::Persistence,
            true,
            &team,
            Some(&collaboration),
            Some(&member),
            &request.actor,
            None,
        );
        self.drive(&mut team, &mut member, &mut collaboration, &request.actor)
            .await
    }

    pub async fn resume(&self, id: Uuid, actor: &str) -> MultiAgentResult<Collaboration> {
        validate_actor(actor)?;
        let mut collaboration = self.required_collaboration(id).await?;
        if !matches!(
            collaboration.state,
            CollaborationState::Assigned
                | CollaborationState::Working
                | CollaborationState::Waiting
                | CollaborationState::OutcomeUnknown
        ) {
            return Err(MultiAgentError::InvalidState(format!(
                "cannot resume {} Collaboration",
                collaboration.state.as_str()
            )));
        }
        let mut team = self.required_team(collaboration.team_id).await?;
        self.policy
            .check(MultiAgentOperation::Resume, Some(&team), actor)?;
        let mut member = self.required_member(collaboration.target_member_id).await?;
        self.enter_team(team.id, collaboration.id)?;
        let result = self
            .drive(&mut team, &mut member, &mut collaboration, actor)
            .await;
        self.leave_team(team.id)?;
        result
    }

    async fn drive(
        &self,
        team: &mut Team,
        member: &mut AgentMember,
        collaboration: &mut Collaboration,
        actor: &str,
    ) -> MultiAgentResult<Collaboration> {
        if collaboration.binding.is_none() {
            let message = collaboration.assignment_message()?.clone();
            let binding = match self
                .dispatcher
                .prepare(collaboration, member, &message)
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    self.finish_failed(team, member, collaboration, actor, &error.to_string())
                        .await?;
                    return Err(error);
                }
            };
            if let Err(error) = binding.validate() {
                self.finish_failed(team, member, collaboration, actor, &error.to_string())
                    .await?;
                return Err(error);
            }
            if binding.dispatch_id != collaboration.dispatch_id()
                || binding.external_id != member.agent_id
            {
                let error = MultiAgentError::Validation(
                    "AgentDispatcher returned a binding for another dispatch or Agent".into(),
                );
                self.finish_failed(team, member, collaboration, actor, &error.to_string())
                    .await?;
                return Err(error);
            }
            let team_expected = team.version;
            advance_team(team, actor);
            let member_expected = member.version;
            member.state = MemberState::Working;
            advance_member(member, actor);
            let collaboration_expected = collaboration.version;
            collaboration.binding = Some(binding);
            collaboration.state = CollaborationState::Working;
            advance_collaboration(collaboration, actor);
            self.commit_existing(
                team,
                team_expected,
                collaboration,
                collaboration_expected,
                vec![(member.clone(), member_expected)],
                actor,
            )
            .await?;
        } else if matches!(
            collaboration.state,
            CollaborationState::Waiting | CollaborationState::OutcomeUnknown
        ) {
            let team_expected = team.version;
            advance_team(team, actor);
            let member_expected = member.version;
            member.state = MemberState::Working;
            advance_member(member, actor);
            let collaboration_expected = collaboration.version;
            collaboration.state = CollaborationState::Working;
            collaboration.error = None;
            advance_collaboration(collaboration, actor);
            self.commit_existing(
                team,
                team_expected,
                collaboration,
                collaboration_expected,
                vec![(member.clone(), member_expected)],
                actor,
            )
            .await?;
        }
        let binding = collaboration.binding.clone().ok_or_else(|| {
            MultiAgentError::Internal("working Collaboration has no binding".into())
        })?;
        let message = collaboration.assignment_message()?.clone();
        let outcome = match self.dispatcher.execute(&binding, &message).await {
            Ok(value) => value,
            Err(error) => CollaborationOutcome::OutcomeUnknown(error.to_string()),
        };
        self.apply_outcome(team, member, collaboration, actor, outcome)
            .await
    }

    async fn apply_outcome(
        &self,
        team: &mut Team,
        member: &mut AgentMember,
        collaboration: &mut Collaboration,
        actor: &str,
        outcome: CollaborationOutcome,
    ) -> MultiAgentResult<Collaboration> {
        let team_expected = team.version;
        let member_expected = member.version;
        let collaboration_expected = collaboration.version;
        let mut unknown = None;
        match outcome {
            CollaborationOutcome::Completed(result) => {
                collaboration.state = CollaborationState::Completed;
                collaboration.result = Some(result);
                collaboration.error = None;
                collaboration.completed_at = Some(Utc::now());
                member.state = MemberState::Completed;
                member.current_collaboration_id = None;
                self.transition_team(team, TeamState::Ready, actor)?;
            }
            CollaborationOutcome::Waiting(reason) => {
                collaboration.state = CollaborationState::Waiting;
                collaboration.error = Some(bounded_reason(&reason));
                member.state = MemberState::Waiting;
                advance_team(team, actor);
            }
            CollaborationOutcome::Failed(reason) => {
                collaboration.state = CollaborationState::Failed;
                collaboration.error = Some(bounded_reason(&reason));
                collaboration.completed_at = Some(Utc::now());
                member.state = MemberState::Available;
                member.current_collaboration_id = None;
                self.transition_team(team, TeamState::Ready, actor)?;
            }
            CollaborationOutcome::OutcomeUnknown(reason) => {
                let reason = bounded_reason(&reason);
                collaboration.state = CollaborationState::OutcomeUnknown;
                collaboration.error = Some(reason.clone());
                member.state = MemberState::Working;
                advance_team(team, actor);
                unknown = Some(reason);
            }
        }
        advance_member(member, actor);
        advance_collaboration(collaboration, actor);
        self.commit_existing(
            team,
            team_expected,
            collaboration,
            collaboration_expected,
            vec![(member.clone(), member_expected)],
            actor,
        )
        .await?;
        self.notify(
            MultiAgentOperation::Assign,
            MultiAgentStage::Outcome,
            unknown.is_none(),
            team,
            Some(collaboration),
            Some(member),
            actor,
            unknown.clone(),
        );
        if let Some(reason) = unknown {
            return Err(MultiAgentError::OutcomeUnknown(reason));
        }
        Ok(collaboration.clone())
    }

    async fn finish_failed(
        &self,
        team: &mut Team,
        member: &mut AgentMember,
        collaboration: &mut Collaboration,
        actor: &str,
        reason: &str,
    ) -> MultiAgentResult<()> {
        let team_expected = team.version;
        self.transition_team(team, TeamState::Ready, actor)?;
        let member_expected = member.version;
        member.state = MemberState::Available;
        member.current_collaboration_id = None;
        advance_member(member, actor);
        let collaboration_expected = collaboration.version;
        collaboration.state = CollaborationState::Failed;
        collaboration.error = Some(bounded_reason(reason));
        collaboration.completed_at = Some(Utc::now());
        advance_collaboration(collaboration, actor);
        self.commit_existing(
            team,
            team_expected,
            collaboration,
            collaboration_expected,
            vec![(member.clone(), member_expected)],
            actor,
        )
        .await
    }

    pub async fn handover(
        &self,
        id: Uuid,
        target_member_id: Uuid,
        actor: &str,
    ) -> MultiAgentResult<Collaboration> {
        validate_actor(actor)?;
        let mut collaboration = self.required_collaboration(id).await?;
        if !matches!(
            collaboration.state,
            CollaborationState::Waiting | CollaborationState::Failed
        ) {
            return Err(MultiAgentError::InvalidState(
                "handover requires a Waiting or Failed Collaboration".into(),
            ));
        }
        let mut team = self.required_team(collaboration.team_id).await?;
        self.policy
            .check(MultiAgentOperation::Handover, Some(&team), actor)?;
        if !team.policy.allow_handover || target_member_id == collaboration.target_member_id {
            return Err(MultiAgentError::Denied(
                "Team policy or target rejects handover".into(),
            ));
        }
        let mut target = self.required_member(target_member_id).await?;
        if target.team_id != team.id || !target.state.is_available() {
            return Err(MultiAgentError::NoRoute(
                "handover target is not an available Member of the Team".into(),
            ));
        }
        let role = self.required_role(target.role_id).await?;
        let descriptor = self
            .directory
            .lookup(target.agent_id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(target.agent_id))?;
        if descriptor.agent_id != target.agent_id
            || descriptor.availability != AgentAvailability::Available
            || !collaboration
                .required_capabilities
                .is_subset(&descriptor.capabilities)
            || !role
                .required_capabilities
                .is_subset(&descriptor.capabilities)
            || collaboration.role_id.is_some_and(|id| id != target.role_id)
        {
            return Err(MultiAgentError::NoRoute(
                "handover target does not satisfy live Role or capability requirements".into(),
            ));
        }
        self.enter_team(team.id, collaboration.id)?;
        let old_id = collaboration.target_member_id;
        let mut old = self.required_member(old_id).await?;
        let team_expected = team.version;
        if team.state == TeamState::Ready {
            self.transition_team(&mut team, TeamState::Active, actor)?;
        } else {
            advance_team(&mut team, actor);
        }
        let collaboration_expected = collaboration.version;
        collaboration.target_member_id = target.id;
        collaboration.handover_count += 1;
        collaboration.state = CollaborationState::Assigned;
        collaboration.binding = None;
        collaboration.result = None;
        collaboration.error = None;
        collaboration.completed_at = None;
        collaboration.actor = actor.into();
        collaboration.messages.push(AgentMessage {
            id: Uuid::new_v4(),
            correlation_id: collaboration.id,
            source_member_id: Some(old.id),
            target_member_id: target.id,
            intent: "team.handover".into(),
            payload: serde_json::json!({ "goal": collaboration.goal }),
            context_references: Default::default(),
            priority: collaboration.priority,
            actor: actor.into(),
            created_at: Utc::now(),
        });
        advance_collaboration(&mut collaboration, actor);
        let target_expected = target.version;
        target.state = MemberState::Assigned;
        target.current_collaboration_id = Some(collaboration.id);
        advance_member(&mut target, actor);
        let mut members = vec![(target.clone(), target_expected)];
        if old.current_collaboration_id == Some(collaboration.id) {
            let old_expected = old.version;
            old.state = MemberState::Available;
            old.current_collaboration_id = None;
            advance_member(&mut old, actor);
            members.push((old, old_expected));
        }
        let commit_members = members
            .iter()
            .map(|(member, expected)| Versioned::update(member.clone(), *expected))
            .collect();
        let result = async {
            self.store
                .commit_collaboration(
                    &CollaborationCommit {
                        team: Versioned::update(team.clone(), team_expected),
                        collaboration: Versioned::update(
                            collaboration.clone(),
                            collaboration_expected,
                        ),
                        members: commit_members,
                    },
                    actor,
                )
                .await?;
            self.drive(&mut team, &mut target, &mut collaboration, actor)
                .await
        }
        .await;
        let release = self.leave_team(team.id);
        match (result, release) {
            (result, Ok(())) => result,
            (_, Err(error)) => Err(error),
        }
    }

    pub async fn complete_team(&self, id: Uuid, actor: &str) -> MultiAgentResult<Team> {
        validate_actor(actor)?;
        let mut team = self.required_team(id).await?;
        self.policy
            .check(MultiAgentOperation::Complete, Some(&team), actor)?;
        if team.state != TeamState::Ready {
            return Err(MultiAgentError::InvalidState(
                "Team can complete only from Ready".into(),
            ));
        }
        let expected = team.version;
        self.transition_team(&mut team, TeamState::Completed, actor)?;
        self.store.save_team(&team, Some(expected), actor).await?;
        Ok(team)
    }

    pub async fn archive_team(&self, id: Uuid, actor: &str) -> MultiAgentResult<Team> {
        validate_actor(actor)?;
        let mut team = self.required_team(id).await?;
        self.policy
            .check(MultiAgentOperation::Archive, Some(&team), actor)?;
        let expected = team.version;
        self.transition_team(&mut team, TeamState::Archived, actor)?;
        self.store.save_team(&team, Some(expected), actor).await?;
        Ok(team)
    }

    pub async fn find_organization(&self, id: Uuid) -> MultiAgentResult<Option<Organization>> {
        self.store.find_organization(id).await
    }

    pub async fn list_organizations(&self) -> MultiAgentResult<Vec<Organization>> {
        self.store.list_organizations().await
    }

    pub async fn find_role(&self, id: Uuid) -> MultiAgentResult<Option<Role>> {
        self.store.find_role(id).await
    }

    pub async fn list_roles(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Role>> {
        self.store.list_roles(organization_id).await
    }

    pub async fn find_team(&self, id: Uuid) -> MultiAgentResult<Option<Team>> {
        self.store.find_team(id).await
    }

    pub async fn list_teams(&self, organization_id: Uuid) -> MultiAgentResult<Vec<Team>> {
        self.store.list_teams(organization_id).await
    }

    pub async fn find_member(&self, id: Uuid) -> MultiAgentResult<Option<AgentMember>> {
        self.store.find_member(id).await
    }

    pub async fn list_members(&self, team_id: Uuid) -> MultiAgentResult<Vec<AgentMember>> {
        self.store.list_members(team_id).await
    }

    pub async fn find_collaboration(&self, id: Uuid) -> MultiAgentResult<Option<Collaboration>> {
        self.store.find_collaboration(id).await
    }

    pub async fn list_collaborations(&self, team_id: Uuid) -> MultiAgentResult<Vec<Collaboration>> {
        self.store.list_collaborations(team_id).await
    }

    async fn routing_candidates(&self, team: &Team) -> MultiAgentResult<Vec<RoutingCandidate>> {
        let members = self.store.list_members(team.id).await?;
        let mut candidates = Vec::new();
        for member in members {
            if !member.state.is_available() {
                continue;
            }
            let role = self.required_role(member.role_id).await?;
            let descriptor = self
                .directory
                .lookup(member.agent_id)
                .await?
                .ok_or_else(|| MultiAgentError::not_found(member.agent_id))?;
            descriptor.validate()?;
            if descriptor.agent_id != member.agent_id {
                return Err(MultiAgentError::Validation(
                    "AgentDirectory returned a descriptor for another Agent".into(),
                ));
            }
            candidates.push(RoutingCandidate {
                member,
                role,
                descriptor,
            });
        }
        Ok(candidates)
    }

    async fn commit_existing(
        &self,
        team: &Team,
        team_expected: u64,
        collaboration: &Collaboration,
        collaboration_expected: u64,
        members: Vec<(AgentMember, u64)>,
        actor: &str,
    ) -> MultiAgentResult<()> {
        self.store
            .commit_collaboration(
                &CollaborationCommit {
                    team: Versioned::update(team.clone(), team_expected),
                    collaboration: Versioned::update(collaboration.clone(), collaboration_expected),
                    members: members
                        .into_iter()
                        .map(|(member, expected)| Versioned::update(member, expected))
                        .collect(),
                },
                actor,
            )
            .await
    }

    fn transition_team(
        &self,
        team: &mut Team,
        state: TeamState,
        actor: &str,
    ) -> MultiAgentResult<()> {
        self.lifecycle.transition(team.state, state)?;
        team.state = state;
        if state.is_terminal() {
            team.completed_at = Some(Utc::now());
        }
        advance_team(team, actor);
        Ok(())
    }

    fn enter_team(&self, team_id: Uuid, collaboration_id: Uuid) -> MultiAgentResult<()> {
        let mut live = self
            .live_teams
            .lock()
            .map_err(|_| MultiAgentError::Internal("live Team lock poisoned".into()))?;
        match live.entry(team_id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(collaboration_id);
                Ok(())
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                Err(MultiAgentError::Conflict(format!(
                    "Team {team_id} is already driving Collaboration {}",
                    entry.get()
                )))
            }
        }
    }

    fn replace_live_collaboration(
        &self,
        team_id: Uuid,
        collaboration_id: Uuid,
    ) -> MultiAgentResult<()> {
        let mut live = self
            .live_teams
            .lock()
            .map_err(|_| MultiAgentError::Internal("live Team lock poisoned".into()))?;
        let value = live
            .get_mut(&team_id)
            .ok_or_else(|| MultiAgentError::Internal("Team live ownership was lost".into()))?;
        *value = collaboration_id;
        Ok(())
    }

    fn leave_team(&self, team_id: Uuid) -> MultiAgentResult<()> {
        self.live_teams
            .lock()
            .map_err(|_| MultiAgentError::Internal("live Team lock poisoned".into()))?
            .remove(&team_id);
        Ok(())
    }

    async fn required_organization(&self, id: Uuid) -> MultiAgentResult<Organization> {
        self.store
            .find_organization(id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(id))
    }

    async fn required_role(&self, id: Uuid) -> MultiAgentResult<Role> {
        self.store
            .find_role(id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(id))
    }

    async fn required_team(&self, id: Uuid) -> MultiAgentResult<Team> {
        self.store
            .find_team(id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(id))
    }

    async fn required_member(&self, id: Uuid) -> MultiAgentResult<AgentMember> {
        self.store
            .find_member(id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(id))
    }

    async fn required_collaboration(&self, id: Uuid) -> MultiAgentResult<Collaboration> {
        self.store
            .find_collaboration(id)
            .await?
            .ok_or_else(|| MultiAgentError::not_found(id))
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        operation: MultiAgentOperation,
        stage: MultiAgentStage,
        success: bool,
        team: &Team,
        collaboration: Option<&Collaboration>,
        member: Option<&AgentMember>,
        actor: &str,
        message: Option<String>,
    ) {
        notify_safely(
            &self.observers,
            &observation(
                operation,
                stage,
                success,
                Some(team.id),
                collaboration.map(|value| value.id),
                member.map(|value| value.id),
                actor,
                message,
            ),
        );
    }
}

fn advance_team(value: &mut Team, actor: &str) {
    value.version = value.version.saturating_add(1);
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at);
}

fn advance_member(value: &mut AgentMember, actor: &str) {
    value.version = value.version.saturating_add(1);
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at);
}

fn advance_collaboration(value: &mut Collaboration, actor: &str) {
    value.version = value.version.saturating_add(1);
    value.actor = actor.into();
    value.updated_at = Utc::now().max(value.updated_at);
}

fn bounded_reason(value: &str) -> String {
    let value = value.trim();
    let result = value.chars().take(1024).collect::<String>();
    if result.is_empty() {
        "Multi-Agent operation failed without a reason".into()
    } else {
        result
    }
}
