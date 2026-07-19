use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    aggregate_state, validate_actor, validate_timeline, WorkItemState, WorkflowAction,
    WorkflowActionOutcome, WorkflowBinding, WorkflowDefinition, WorkflowIdentity, WorkflowInstance,
    WorkflowSnapshot, WorkflowState, WorkflowStateRecord,
};
use crate::error::{WorkflowError, WorkflowResult};
use crate::infrastructure::{
    WorkflowActionContext, WorkflowControl, WorkflowCursor, WorkflowEngine, WorkflowInstanceCommit,
    WorkflowLifecycle, WorkflowOperation, WorkflowPolicy, WorkflowRegistrationCommit,
    WorkflowRegistry, WorkflowScheduler, WorkflowStore,
};

#[derive(Default)]
pub struct InMemoryWorkflowRegistry {
    definitions: RwLock<HashMap<(Uuid, u64), WorkflowDefinition>>,
}

impl WorkflowRegistry for InMemoryWorkflowRegistry {
    fn register(&self, definition: WorkflowDefinition) -> WorkflowResult<()> {
        definition.validate()?;
        let key = (definition.workflow_id, definition.version);
        let mut values = self
            .definitions
            .write()
            .map_err(|_| WorkflowError::Internal("workflow registry lock poisoned".into()))?;
        if let Some(current) = values.get(&key) {
            if current == &definition {
                return Ok(());
            }
            return Err(WorkflowError::Conflict(
                "workflow Definition version is already registered".into(),
            ));
        }
        if values
            .values()
            .any(|value| value.key == definition.key && value.workflow_id != definition.workflow_id)
        {
            return Err(WorkflowError::Conflict(
                "workflow key belongs to another identity".into(),
            ));
        }
        values.insert(key, definition);
        Ok(())
    }

    fn find(&self, workflow_id: Uuid, version: u64) -> WorkflowResult<Option<WorkflowDefinition>> {
        Ok(self
            .definitions
            .read()
            .map_err(|_| WorkflowError::Internal("workflow registry lock poisoned".into()))?
            .get(&(workflow_id, version))
            .cloned())
    }

    fn find_current(&self, key: &str) -> WorkflowResult<Option<WorkflowDefinition>> {
        Ok(self
            .definitions
            .read()
            .map_err(|_| WorkflowError::Internal("workflow registry lock poisoned".into()))?
            .values()
            .filter(|value| value.key == key)
            .max_by_key(|value| value.version)
            .cloned())
    }

    fn list(&self) -> WorkflowResult<Vec<WorkflowDefinition>> {
        let mut values = self
            .definitions
            .read()
            .map_err(|_| WorkflowError::Internal("workflow registry lock poisoned".into()))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.version));
        Ok(values)
    }
}

pub struct SequentialWorkflowScheduler;

impl WorkflowScheduler for SequentialWorkflowScheduler {
    fn next(&self, instance: &WorkflowInstance) -> WorkflowResult<Option<WorkflowCursor>> {
        instance.validate()?;
        if instance.state.is_terminal() {
            return Ok(None);
        }
        for (stage_index, stage) in instance.progress.iter().enumerate() {
            for (activity_index, activity) in stage.activities.iter().enumerate() {
                for (action_index, action) in activity.actions.iter().enumerate() {
                    if action.state == WorkItemState::Completed {
                        continue;
                    }
                    if matches!(
                        action.state,
                        WorkItemState::Failed | WorkItemState::Cancelled
                    ) {
                        return Ok(None);
                    }
                    return Ok(Some(WorkflowCursor {
                        stage_index,
                        activity_index,
                        action_index,
                        stage_id: stage.stage_id,
                        activity_id: activity.activity_id,
                        action_id: action.action_id,
                    }));
                }
            }
        }
        Ok(None)
    }
}

pub struct UnavailableWorkflowEngine;

#[async_trait]
impl WorkflowEngine for UnavailableWorkflowEngine {
    async fn prepare(
        &self,
        _action: &WorkflowAction,
        _context: &WorkflowActionContext,
    ) -> WorkflowResult<WorkflowBinding> {
        Err(WorkflowError::Engine(
            "WorkflowEngine must be provided by the composition runtime".into(),
        ))
    }

    async fn execute(
        &self,
        _binding: &WorkflowBinding,
        _action: &WorkflowAction,
        _context: &WorkflowActionContext,
        _control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowActionOutcome> {
        Err(WorkflowError::Engine(
            "WorkflowEngine must be provided by the composition runtime".into(),
        ))
    }

    async fn cancel(&self, _binding: &WorkflowBinding, _actor: &str) -> WorkflowResult<bool> {
        Err(WorkflowError::Engine(
            "WorkflowEngine must be provided by the composition runtime".into(),
        ))
    }
}

pub struct EmbeddedWorkflowPolicy;

impl WorkflowPolicy for EmbeddedWorkflowPolicy {
    fn check(
        &self,
        operation: WorkflowOperation,
        definition: &WorkflowDefinition,
        _instance: Option<&WorkflowInstance>,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        definition.validate()?;
        let allowed = match operation {
            WorkflowOperation::Pause => definition.policy.allow_pause,
            WorkflowOperation::Resume => definition.policy.allow_resume,
            WorkflowOperation::Cancel => definition.policy.allow_cancel,
            _ => true,
        };
        if !allowed {
            return Err(WorkflowError::PolicyDenied(format!(
                "{} is disabled by Workflow Policy",
                format_operation(operation)
            )));
        }
        Ok(())
    }
}

fn format_operation(value: WorkflowOperation) -> &'static str {
    match value {
        WorkflowOperation::Register => "register",
        WorkflowOperation::Start => "start",
        WorkflowOperation::Run => "run",
        WorkflowOperation::Pause => "pause",
        WorkflowOperation::Resume => "resume",
        WorkflowOperation::Cancel => "cancel",
        WorkflowOperation::Snapshot => "snapshot",
        WorkflowOperation::Restore => "restore",
        WorkflowOperation::Archive => "archive",
    }
}

pub struct DefaultWorkflowLifecycle;

impl WorkflowLifecycle for DefaultWorkflowLifecycle {
    fn transition(
        &self,
        instance: &mut WorkflowInstance,
        next: WorkflowState,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        let allowed = matches!(
            (instance.state, next),
            (
                WorkflowState::Created,
                WorkflowState::Scheduled | WorkflowState::Cancelled
            ) | (
                WorkflowState::Scheduled,
                WorkflowState::Running | WorkflowState::Paused | WorkflowState::Cancelled
            ) | (
                WorkflowState::Running,
                WorkflowState::Waiting
                    | WorkflowState::Paused
                    | WorkflowState::Completed
                    | WorkflowState::Failed
                    | WorkflowState::Cancelled
            ) | (
                WorkflowState::Waiting,
                WorkflowState::Running
                    | WorkflowState::Paused
                    | WorkflowState::Failed
                    | WorkflowState::Cancelled
            ) | (
                WorkflowState::Paused,
                WorkflowState::Running | WorkflowState::Cancelled
            ) | (
                WorkflowState::Completed | WorkflowState::Failed | WorkflowState::Cancelled,
                WorkflowState::Archived
            )
        );
        if !allowed {
            return Err(WorkflowError::InvalidState(format!(
                "cannot transition {} Workflow to {}",
                instance.state.as_str(),
                next.as_str()
            )));
        }
        instance.state = next;
        instance.version = instance.version.saturating_add(1);
        instance.actor = actor.into();
        instance.updated_at = Utc::now().max(instance.updated_at);
        if next == WorkflowState::Running && instance.started_at.is_none() {
            instance.started_at = Some(instance.updated_at);
        }
        if matches!(
            next,
            WorkflowState::Completed
                | WorkflowState::Failed
                | WorkflowState::Cancelled
                | WorkflowState::Archived
        ) {
            instance.completed_at = Some(instance.updated_at);
        }
        instance.validate()
    }
}

#[derive(Clone, Default)]
struct InMemoryState {
    workflows: HashMap<Uuid, WorkflowIdentity>,
    workflow_keys: HashMap<String, Uuid>,
    definitions: HashMap<(Uuid, u64), WorkflowDefinition>,
    instances: HashMap<Uuid, WorkflowInstance>,
    states: HashMap<Uuid, WorkflowStateRecord>,
    snapshots: HashMap<Uuid, WorkflowSnapshot>,
}

#[derive(Default)]
pub struct InMemoryWorkflowStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryWorkflowStore {
    fn read(&self) -> WorkflowResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| WorkflowError::Internal("workflow store lock poisoned".into()))
    }

    fn write(&self) -> WorkflowResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| WorkflowError::Internal("workflow store lock poisoned".into()))
    }
}

#[async_trait]
impl WorkflowStore for InMemoryWorkflowStore {
    async fn save_registration(
        &self,
        commit: &WorkflowRegistrationCommit,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        match commit.expected_identity_version {
            None => {
                if next.workflows.contains_key(&commit.identity.id)
                    || next.workflow_keys.contains_key(&commit.identity.key)
                    || commit.identity.version != 1
                    || commit.definition.version != 1
                {
                    return Err(WorkflowError::Conflict(
                        "workflow identity or key already exists".into(),
                    ));
                }
            }
            Some(expected) => {
                let current = next
                    .workflows
                    .get(&commit.identity.id)
                    .ok_or_else(|| WorkflowError::NotFound(commit.identity.id.to_string()))?;
                validate_identity_update(current, &commit.identity, &commit.definition, expected)?;
            }
        }
        if next
            .definitions
            .contains_key(&(commit.definition.workflow_id, commit.definition.version))
        {
            return Err(WorkflowError::Conflict(
                "workflow Definition version already exists".into(),
            ));
        }
        next.workflow_keys
            .insert(commit.identity.key.clone(), commit.identity.id);
        next.workflows
            .insert(commit.identity.id, commit.identity.clone());
        next.definitions.insert(
            (commit.definition.workflow_id, commit.definition.version),
            commit.definition.clone(),
        );
        *state = next;
        Ok(())
    }

    async fn find_workflow(&self, id: Uuid) -> WorkflowResult<Option<WorkflowIdentity>> {
        Ok(self.read()?.workflows.get(&id).cloned())
    }

    async fn find_workflow_by_key(&self, key: &str) -> WorkflowResult<Option<WorkflowIdentity>> {
        let state = self.read()?;
        Ok(state
            .workflow_keys
            .get(key)
            .and_then(|id| state.workflows.get(id))
            .cloned())
    }

    async fn list_workflows(&self) -> WorkflowResult<Vec<WorkflowIdentity>> {
        let mut values = self.read()?.workflows.values().cloned().collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn find_definition(
        &self,
        workflow_id: Uuid,
        version: u64,
    ) -> WorkflowResult<Option<WorkflowDefinition>> {
        Ok(self
            .read()?
            .definitions
            .get(&(workflow_id, version))
            .cloned())
    }

    async fn list_definitions(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowDefinition>> {
        let mut values = self
            .read()?
            .definitions
            .iter()
            .filter(|((id, _), _)| *id == workflow_id)
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>();
        values.sort_by_key(|value| value.version);
        Ok(values)
    }

    async fn commit_instance(
        &self,
        commit: &WorkflowInstanceCommit,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        match commit.expected_version {
            None => {
                if next.instances.contains_key(&commit.instance.id) || commit.state_record.is_none()
                {
                    return Err(WorkflowError::Conflict(
                        "workflow Instance already exists or lacks initial state".into(),
                    ));
                }
            }
            Some(expected) => {
                let current = next
                    .instances
                    .get(&commit.instance.id)
                    .ok_or_else(|| WorkflowError::NotFound(commit.instance.id.to_string()))?;
                validate_instance_update(current, &commit.instance, expected)?;
                validate_state_record_change(current, commit)?;
            }
        }
        let definition = next
            .definitions
            .get(&(
                commit.instance.workflow_id,
                commit.instance.definition_version,
            ))
            .ok_or_else(|| WorkflowError::NotFound(commit.instance.definition_id.to_string()))?;
        if definition != &commit.instance.definition
            || definition.id != commit.instance.definition_id
        {
            return Err(WorkflowError::Validation(
                "workflow Instance Definition snapshot does not match Catalog".into(),
            ));
        }
        if let Some(record) = &commit.state_record {
            if next.states.contains_key(&record.id)
                || next.states.values().any(|value| {
                    value.instance_id == record.instance_id && value.sequence == record.sequence
                })
            {
                return Err(WorkflowError::Conflict(
                    "workflow state record already exists".into(),
                ));
            }
            next.states.insert(record.id, record.clone());
        }
        next.instances
            .insert(commit.instance.id, commit.instance.clone());
        *state = next;
        Ok(())
    }

    async fn find_instance(&self, id: Uuid) -> WorkflowResult<Option<WorkflowInstance>> {
        Ok(self.read()?.instances.get(&id).cloned())
    }

    async fn list_instances(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowInstance>> {
        let mut values = self
            .read()?
            .instances
            .values()
            .filter(|value| value.workflow_id == workflow_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.created_at), value.id));
        Ok(values)
    }

    async fn list_states(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowStateRecord>> {
        let state = self.read()?;
        let instance = state
            .instances
            .get(&instance_id)
            .ok_or_else(|| WorkflowError::NotFound(instance_id.to_string()))?;
        let mut values = state
            .states
            .values()
            .filter(|value| value.instance_id == instance_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.sequence, value.id));
        validate_timeline(instance, &values)?;
        Ok(values)
    }

    async fn save_snapshot(&self, value: &WorkflowSnapshot, actor: &str) -> WorkflowResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        let instance = state
            .instances
            .get(&value.instance_id)
            .ok_or_else(|| WorkflowError::NotFound(value.instance_id.to_string()))?;
        if value.sequence > instance.version
            || value.content.workflow_id != instance.workflow_id
            || value.content.definition_id != instance.definition_id
            || value.content.definition_version != instance.definition_version
            || value.content.created_at != instance.created_at
            || state.snapshots.contains_key(&value.id)
        {
            return Err(WorkflowError::Conflict(
                "workflow Snapshot sequence or identity is invalid".into(),
            ));
        }
        state.snapshots.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> WorkflowResult<Option<WorkflowSnapshot>> {
        Ok(self.read()?.snapshots.get(&id).cloned())
    }

    async fn list_snapshots(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowSnapshot>> {
        let mut values = self
            .read()?
            .snapshots
            .values()
            .filter(|value| value.instance_id == instance_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.sequence), value.id));
        Ok(values)
    }
}

fn validate_identity_update(
    current: &WorkflowIdentity,
    next: &WorkflowIdentity,
    definition: &WorkflowDefinition,
    expected: u64,
) -> WorkflowResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || definition.version != current.current_definition_version.saturating_add(1)
        || next.current_definition_version != definition.version
    {
        return Err(WorkflowError::Conflict(
            "workflow identity or Definition version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_instance_update(
    current: &WorkflowInstance,
    next: &WorkflowInstance,
    expected: u64,
) -> WorkflowResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.workflow_id != next.workflow_id
        || current.definition_id != next.definition_id
        || current.definition_version != next.definition_version
        || current.definition != next.definition
        || current.variables != next.variables
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(WorkflowError::Conflict(
            "workflow Instance identity, Definition, Variables or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_state_record_change(
    current: &WorkflowInstance,
    commit: &WorkflowInstanceCommit,
) -> WorkflowResult<()> {
    let changed = current.state != commit.instance.state;
    if changed != commit.state_record.is_some()
        || commit.state_record.as_ref().is_some_and(|record| {
            record.from_state != Some(current.state) || record.to_state != commit.instance.state
        })
    {
        return Err(WorkflowError::Validation(
            "workflow lifecycle changes require one matching Timeline record".into(),
        ));
    }
    Ok(())
}

pub(crate) fn refresh_progress(instance: &mut WorkflowInstance) {
    for stage in &mut instance.progress {
        for activity in &mut stage.activities {
            activity.state = aggregate_state(activity.actions.iter().map(|value| value.state));
        }
        stage.state = aggregate_state(stage.activities.iter().map(|value| value.state));
    }
}

pub(crate) fn action_progress_mut(
    instance: &mut WorkflowInstance,
    cursor: WorkflowCursor,
) -> WorkflowResult<&mut crate::domain::ActionProgress> {
    instance
        .progress
        .get_mut(cursor.stage_index)
        .and_then(|stage| stage.activities.get_mut(cursor.activity_index))
        .and_then(|activity| activity.actions.get_mut(cursor.action_index))
        .filter(|action| action.action_id == cursor.action_id)
        .ok_or_else(|| WorkflowError::Internal("workflow cursor no longer matches progress".into()))
}

pub(crate) fn action_definition(
    instance: &WorkflowInstance,
    cursor: WorkflowCursor,
) -> WorkflowResult<&WorkflowAction> {
    instance
        .definition
        .stages
        .get(cursor.stage_index)
        .and_then(|stage| stage.activities.get(cursor.activity_index))
        .and_then(|activity| activity.actions.get(cursor.action_index))
        .filter(|action| action.id == cursor.action_id)
        .ok_or_else(|| {
            WorkflowError::Internal("workflow cursor no longer matches Definition".into())
        })
}

pub(crate) fn initial_state_record(instance: &WorkflowInstance) -> WorkflowStateRecord {
    WorkflowStateRecord {
        id: Uuid::new_v4(),
        instance_id: instance.id,
        sequence: instance.version,
        from_state: None,
        to_state: instance.state,
        reason: "workflow instance created".into(),
        actor: instance.actor.clone(),
        created_at: instance.created_at,
    }
}
