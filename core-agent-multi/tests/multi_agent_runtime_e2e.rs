use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use core_agent_multi::{
    AgentAvailability, AgentDescriptor, AgentDirectory, AgentDispatcher, AgentMember, AgentMessage,
    AssignmentRequest, Collaboration, CollaborationBinding, CollaborationOutcome,
    CollaborationResult, CollaborationState, CreateTeamRequest, InMemoryMultiAgentStore,
    MemberState, MultiAgentError, MultiAgentManager, MultiAgentObservation, MultiAgentObserver,
    MultiAgentResult, MultiAgentStore, Organization, Role, SqliteMultiAgentStore, TeamState,
};
use rusqlite::Connection;
use tempfile::tempdir;
use uuid::Uuid;

struct StaticDirectory {
    values: HashMap<Uuid, AgentDescriptor>,
}

#[async_trait]
impl AgentDirectory for StaticDirectory {
    async fn lookup(&self, agent_id: Uuid) -> MultiAgentResult<Option<AgentDescriptor>> {
        Ok(self.values.get(&agent_id).cloned())
    }
}

struct ScriptDispatcher {
    prepares: AtomicUsize,
    executions: AtomicUsize,
    outcomes: Mutex<VecDeque<CollaborationOutcome>>,
    dispatches: Mutex<Vec<Uuid>>,
}

impl ScriptDispatcher {
    fn new(outcomes: Vec<CollaborationOutcome>) -> Self {
        Self {
            prepares: AtomicUsize::new(0),
            executions: AtomicUsize::new(0),
            outcomes: Mutex::new(outcomes.into()),
            dispatches: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl AgentDispatcher for ScriptDispatcher {
    async fn prepare(
        &self,
        collaboration: &Collaboration,
        member: &AgentMember,
        _message: &AgentMessage,
    ) -> MultiAgentResult<CollaborationBinding> {
        self.prepares.fetch_add(1, Ordering::SeqCst);
        self.dispatches
            .lock()
            .unwrap()
            .push(collaboration.dispatch_id());
        Ok(CollaborationBinding {
            dispatch_id: collaboration.dispatch_id(),
            external_id: member.agent_id,
            external_kind: "agent".into(),
            prepared_at: Utc::now(),
        })
    }

    async fn execute(
        &self,
        _binding: &CollaborationBinding,
        _message: &AgentMessage,
    ) -> MultiAgentResult<CollaborationOutcome> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| completed("done")))
    }
}

fn completed(summary: &str) -> CollaborationOutcome {
    CollaborationOutcome::Completed(CollaborationResult {
        summary: summary.into(),
        external_state: "COMPLETED".into(),
        completed_at: Utc::now(),
    })
}

fn descriptor(agent_id: Uuid, capabilities: &[&str]) -> AgentDescriptor {
    AgentDescriptor {
        agent_id,
        capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
        availability: AgentAvailability::Available,
        workspace_id: None,
    }
}

async fn configured_manager(
    store: Arc<dyn MultiAgentStore>,
    dispatcher: Arc<ScriptDispatcher>,
    agents: &[(Uuid, &[&str])],
) -> (MultiAgentManager, Organization, Role) {
    let directory = StaticDirectory {
        values: agents
            .iter()
            .map(|(id, capabilities)| (*id, descriptor(*id, capabilities)))
            .collect(),
    };
    let manager = MultiAgentManager::builder()
        .store(store)
        .directory(Arc::new(directory))
        .dispatcher(dispatcher)
        .build();
    let organization = manager
        .create_organization(Organization::new("engineering", "Engineering", "operator"))
        .await
        .unwrap();
    let mut role = Role::new(organization.id, "coder", "Coder", "operator");
    role.required_capabilities.insert("code.write".into());
    let role = manager.create_role(role).await.unwrap();
    (manager, organization, role)
}

#[tokio::test]
async fn deterministic_team_routes_executes_and_persists_protocol() {
    let first = Uuid::from_u128(1);
    let second = Uuid::from_u128(2);
    let dispatcher = Arc::new(ScriptDispatcher::new(vec![completed("implemented")]));
    let (manager, organization, role) = configured_manager(
        Arc::new(InMemoryMultiAgentStore::default()),
        dispatcher.clone(),
        &[(first, &["code.write"]), (second, &["code.write"])],
    )
    .await;
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "coding",
            "Coding",
            "Ship feature",
            "lead",
        ))
        .await
        .unwrap();
    let first_member = manager.join(team.id, role.id, first, "lead").await.unwrap();
    let second_member = manager
        .join(team.id, role.id, second, "lead")
        .await
        .unwrap();
    manager.activate_team(team.id, "lead").await.unwrap();
    let mut request = AssignmentRequest::new(team.id, "Implement API", "lead");
    request.role_id = Some(role.id);
    request.required_capabilities.insert("code.write".into());
    let collaboration = manager.assign(request).await.unwrap();

    let selected_member_id = first_member.id.min(second_member.id);
    assert_eq!(collaboration.target_member_id, selected_member_id);
    assert_eq!(collaboration.state, CollaborationState::Completed);
    assert_eq!(collaboration.messages.len(), 1);
    assert_eq!(collaboration.messages[0].intent, "team.assignment");
    assert_eq!(dispatcher.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(dispatcher.executions.load(Ordering::SeqCst), 1);
    assert_eq!(
        manager.find_team(team.id).await.unwrap().unwrap().state,
        TeamState::Ready
    );
    assert_eq!(
        manager
            .find_member(selected_member_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        MemberState::Completed
    );
}

#[tokio::test]
async fn waiting_resume_reuses_persisted_binding() {
    let agent = Uuid::new_v4();
    let dispatcher = Arc::new(ScriptDispatcher::new(vec![
        CollaborationOutcome::Waiting("review needed".into()),
        completed("reviewed"),
    ]));
    let (manager, organization, role) = configured_manager(
        Arc::new(InMemoryMultiAgentStore::default()),
        dispatcher.clone(),
        &[(agent, &["code.write"])],
    )
    .await;
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "review",
            "Review",
            "Review code",
            "lead",
        ))
        .await
        .unwrap();
    manager.join(team.id, role.id, agent, "lead").await.unwrap();
    manager.activate_team(team.id, "lead").await.unwrap();
    let waiting = manager
        .assign(AssignmentRequest::new(team.id, "Review patch", "lead"))
        .await
        .unwrap();
    assert_eq!(waiting.state, CollaborationState::Waiting);
    let dispatch = waiting.binding.as_ref().unwrap().dispatch_id;
    let completed = manager.resume(waiting.id, "lead").await.unwrap();
    assert_eq!(completed.state, CollaborationState::Completed);
    assert_eq!(completed.binding.unwrap().dispatch_id, dispatch);
    assert_eq!(dispatcher.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(dispatcher.executions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn explicit_handover_changes_dispatch_and_releases_old_member() {
    let first = Uuid::from_u128(10);
    let second = Uuid::from_u128(20);
    let dispatcher = Arc::new(ScriptDispatcher::new(vec![
        CollaborationOutcome::Waiting("handover".into()),
        completed("finished by backup"),
    ]));
    let (manager, organization, role) = configured_manager(
        Arc::new(InMemoryMultiAgentStore::default()),
        dispatcher.clone(),
        &[(first, &["code.write"]), (second, &["code.write"])],
    )
    .await;
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "handover",
            "Handover",
            "Complete task",
            "lead",
        ))
        .await
        .unwrap();
    let first_member = manager.join(team.id, role.id, first, "lead").await.unwrap();
    let second_member = manager
        .join(team.id, role.id, second, "lead")
        .await
        .unwrap();
    manager.activate_team(team.id, "lead").await.unwrap();
    let waiting = manager
        .assign(AssignmentRequest::new(team.id, "Implement", "lead"))
        .await
        .unwrap();
    let (old, backup) = if waiting.target_member_id == first_member.id {
        (first_member, second_member)
    } else {
        (second_member, first_member)
    };
    let first_dispatch = waiting.binding.as_ref().unwrap().dispatch_id;
    let completed = manager
        .handover(waiting.id, backup.id, "lead")
        .await
        .unwrap();
    assert_eq!(completed.target_member_id, backup.id);
    assert_eq!(completed.state, CollaborationState::Completed);
    assert_eq!(completed.messages.len(), 2);
    assert_ne!(completed.binding.unwrap().dispatch_id, first_dispatch);
    assert_eq!(
        manager.find_member(old.id).await.unwrap().unwrap().state,
        MemberState::Available
    );
    assert_eq!(dispatcher.prepares.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn unknown_outcome_is_durable_and_resume_does_not_prepare_again() {
    let agent = Uuid::new_v4();
    let dispatcher = Arc::new(ScriptDispatcher::new(vec![
        CollaborationOutcome::OutcomeUnknown("connection lost".into()),
        completed("reconciled"),
    ]));
    let (manager, organization, role) = configured_manager(
        Arc::new(InMemoryMultiAgentStore::default()),
        dispatcher.clone(),
        &[(agent, &["code.write"])],
    )
    .await;
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "unknown",
            "Unknown",
            "Recover safely",
            "lead",
        ))
        .await
        .unwrap();
    manager.join(team.id, role.id, agent, "lead").await.unwrap();
    manager.activate_team(team.id, "lead").await.unwrap();
    assert!(matches!(
        manager
            .assign(AssignmentRequest::new(team.id, "Run once", "lead"))
            .await,
        Err(MultiAgentError::OutcomeUnknown(_))
    ));
    let unknown = manager
        .list_collaborations(team.id)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(unknown.state, CollaborationState::OutcomeUnknown);
    let completed = manager.resume(unknown.id, "lead").await.unwrap();
    assert_eq!(completed.state, CollaborationState::Completed);
    assert_eq!(dispatcher.prepares.load(Ordering::SeqCst), 1);
}

struct PanickingObserver;
impl MultiAgentObserver for PanickingObserver {
    fn on_observation(&self, _observation: &MultiAgentObservation) {
        panic!("observer failure")
    }
}

#[tokio::test]
async fn observer_panic_does_not_change_collaboration_outcome() {
    let agent = Uuid::new_v4();
    let directory = StaticDirectory {
        values: [(agent, descriptor(agent, &["code.write"]))]
            .into_iter()
            .collect(),
    };
    let manager = MultiAgentManager::builder()
        .directory(Arc::new(directory))
        .dispatcher(Arc::new(ScriptDispatcher::new(Vec::new())))
        .observer(Arc::new(PanickingObserver))
        .build();
    let organization = manager
        .create_organization(Organization::new("org", "Org", "operator"))
        .await
        .unwrap();
    let role = manager
        .create_role(Role::new(organization.id, "coder", "Coder", "operator"))
        .await
        .unwrap();
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "team",
            "Team",
            "Goal",
            "operator",
        ))
        .await
        .unwrap();
    manager
        .join(team.id, role.id, agent, "operator")
        .await
        .unwrap();
    manager.activate_team(team.id, "operator").await.unwrap();
    assert_eq!(
        manager
            .assign(AssignmentRequest::new(team.id, "Work", "operator"))
            .await
            .unwrap()
            .state,
        CollaborationState::Completed
    );
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("multi-agent.db");
    let store = Arc::new(SqliteMultiAgentStore::new(&path).unwrap());
    let agent = Uuid::new_v4();
    let dispatcher = Arc::new(ScriptDispatcher::new(Vec::new()));
    let (manager, organization, role) =
        configured_manager(store.clone(), dispatcher, &[(agent, &["code.write"])]).await;
    let team = manager
        .create_team(CreateTeamRequest::new(
            organization.id,
            "sqlite",
            "SQLite",
            "Persist",
            "operator",
        ))
        .await
        .unwrap();
    manager
        .join(team.id, role.id, agent, "operator")
        .await
        .unwrap();
    manager.activate_team(team.id, "operator").await.unwrap();
    let collaboration = manager
        .assign(AssignmentRequest::new(team.id, "Persist task", "operator"))
        .await
        .unwrap();
    drop(manager);
    drop(store);

    let reopened = SqliteMultiAgentStore::new(&path).unwrap();
    assert_eq!(
        reopened
            .find_collaboration(collaboration.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        CollaborationState::Completed
    );
    let connection = Connection::open(&path).unwrap();
    for table in [
        "organization",
        "team",
        "agent_member",
        "role",
        "collaboration",
    ] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        for required in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.contains(required), "{table} lacks {required}");
        }
        let foreign_keys: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0);
    }
    connection
        .execute(
            "UPDATE collaboration SET state='FAILED' WHERE id=?1",
            [collaboration.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        reopened.find_collaboration(collaboration.id).await,
        Err(MultiAgentError::Validation(_))
    ));
}
