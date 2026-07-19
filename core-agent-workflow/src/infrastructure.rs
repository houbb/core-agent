use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::{
    WorkflowAction, WorkflowActionOutcome, WorkflowBinding, WorkflowDefinition, WorkflowIdentity,
    WorkflowInstance, WorkflowSnapshot, WorkflowState, WorkflowStateRecord, WorkflowVariables,
};
use crate::error::{WorkflowError, WorkflowResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowOperation {
    Register,
    Start,
    Run,
    Pause,
    Resume,
    Cancel,
    Snapshot,
    Restore,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowStage {
    Registry,
    Scheduling,
    Lifecycle,
    Engine,
    Persistence,
    Recovery,
}

#[derive(Debug, Clone)]
pub struct WorkflowObservation {
    pub operation: WorkflowOperation,
    pub stage: WorkflowStage,
    pub success: bool,
    pub workflow_id: Option<Uuid>,
    pub definition_id: Option<Uuid>,
    pub instance_id: Option<Uuid>,
    pub action_id: Option<Uuid>,
    pub actor: String,
    pub reason: String,
    pub occurred_at: DateTime<Utc>,
}

pub trait WorkflowObserver: Send + Sync {
    fn on_observation(&self, value: &WorkflowObservation);
}

pub trait WorkflowInterceptor: Send + Sync {
    fn before_start(
        &self,
        _definition: &WorkflowDefinition,
        _variables: &mut WorkflowVariables,
    ) -> WorkflowResult<()> {
        Ok(())
    }
}

pub trait WorkflowRegistry: Send + Sync {
    fn register(&self, definition: WorkflowDefinition) -> WorkflowResult<()>;
    fn find(&self, workflow_id: Uuid, version: u64) -> WorkflowResult<Option<WorkflowDefinition>>;
    fn find_current(&self, key: &str) -> WorkflowResult<Option<WorkflowDefinition>>;
    fn list(&self) -> WorkflowResult<Vec<WorkflowDefinition>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowCursor {
    pub stage_index: usize,
    pub activity_index: usize,
    pub action_index: usize,
    pub stage_id: Uuid,
    pub activity_id: Uuid,
    pub action_id: Uuid,
}

pub trait WorkflowScheduler: Send + Sync {
    fn next(&self, instance: &WorkflowInstance) -> WorkflowResult<Option<WorkflowCursor>>;
}

#[derive(Debug, Clone)]
pub struct WorkflowActionContext {
    pub instance_id: Uuid,
    pub workflow_id: Uuid,
    pub definition_id: Uuid,
    pub definition_version: u64,
    pub stage_id: Uuid,
    pub activity_id: Uuid,
    pub action_id: Uuid,
    pub dispatch_id: Uuid,
    pub attempt: u32,
    pub variables: WorkflowVariables,
    pub actor: String,
}

#[derive(Clone)]
pub struct WorkflowControl {
    cancellation: CancellationToken,
    pause: CancellationToken,
    cancellation_actor: Arc<RwLock<Option<String>>>,
    pause_actor: Arc<RwLock<Option<String>>>,
}

impl Default for WorkflowControl {
    fn default() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            pause: CancellationToken::new(),
            cancellation_actor: Arc::new(RwLock::new(None)),
            pause_actor: Arc::new(RwLock::new(None)),
        }
    }
}

impl WorkflowControl {
    pub fn cancel_as(&self, actor: impl Into<String>) {
        if let Ok(mut value) = self.cancellation_actor.write() {
            *value = Some(actor.into());
        }
        self.cancellation.cancel();
    }

    pub fn request_pause_as(&self, actor: impl Into<String>) {
        if let Ok(mut value) = self.pause_actor.write() {
            *value = Some(actor.into());
        }
        self.pause.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn is_pause_requested(&self) -> bool {
        self.pause.is_cancelled()
    }

    pub fn cancellation_actor(&self) -> Option<String> {
        self.cancellation_actor
            .read()
            .ok()
            .and_then(|value| value.clone())
    }

    pub fn pause_actor(&self) -> Option<String> {
        self.pause_actor.read().ok().and_then(|value| value.clone())
    }

    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }

    pub async fn pause_requested(&self) {
        self.pause.cancelled().await;
    }
}

#[async_trait]
pub trait WorkflowEngine: Send + Sync {
    async fn prepare(
        &self,
        action: &WorkflowAction,
        context: &WorkflowActionContext,
    ) -> WorkflowResult<WorkflowBinding>;

    async fn execute(
        &self,
        binding: &WorkflowBinding,
        action: &WorkflowAction,
        context: &WorkflowActionContext,
        control: &WorkflowControl,
    ) -> WorkflowResult<WorkflowActionOutcome>;

    /// Returns `true` only after terminal external cancellation is durably visible.
    /// `false` means another terminal outcome won; an unconfirmed request must
    /// return `WorkflowError::OutcomeUnknown`.
    async fn cancel(&self, binding: &WorkflowBinding, actor: &str) -> WorkflowResult<bool>;
}

pub trait WorkflowPolicy: Send + Sync {
    fn check(
        &self,
        operation: WorkflowOperation,
        definition: &WorkflowDefinition,
        instance: Option<&WorkflowInstance>,
        actor: &str,
    ) -> WorkflowResult<()>;
}

pub trait WorkflowLifecycle: Send + Sync {
    fn transition(
        &self,
        instance: &mut WorkflowInstance,
        next: WorkflowState,
        actor: &str,
    ) -> WorkflowResult<()>;
}

#[derive(Debug, Clone)]
pub struct WorkflowRegistrationCommit {
    pub identity: WorkflowIdentity,
    pub definition: WorkflowDefinition,
    pub expected_identity_version: Option<u64>,
}

impl WorkflowRegistrationCommit {
    pub fn validate(&self) -> WorkflowResult<()> {
        self.identity.validate()?;
        self.definition.validate()?;
        if self.identity.id != self.definition.workflow_id
            || self.identity.key != self.definition.key
            || self.identity.current_definition_id != self.definition.id
            || self.identity.current_definition_version != self.definition.version
            || self
                .expected_identity_version
                .is_some_and(|expected| self.identity.version != expected.saturating_add(1))
        {
            return Err(WorkflowError::Validation(
                "workflow registration identity and Definition are inconsistent".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowInstanceCommit {
    pub instance: WorkflowInstance,
    pub expected_version: Option<u64>,
    pub state_record: Option<WorkflowStateRecord>,
}

impl WorkflowInstanceCommit {
    pub fn create(instance: WorkflowInstance, state_record: WorkflowStateRecord) -> Self {
        Self {
            instance,
            expected_version: None,
            state_record: Some(state_record),
        }
    }

    pub fn update(
        instance: WorkflowInstance,
        expected_version: u64,
        state_record: Option<WorkflowStateRecord>,
    ) -> Self {
        Self {
            instance,
            expected_version: Some(expected_version),
            state_record,
        }
    }

    pub fn validate(&self) -> WorkflowResult<()> {
        self.instance.validate()?;
        if self.expected_version.is_none()
            && (self.instance.version != 1
                || self.instance.state != WorkflowState::Created
                || self
                    .state_record
                    .as_ref()
                    .is_none_or(|record| record.from_state.is_some()))
        {
            return Err(WorkflowError::Validation(
                "workflow instance creation requires an initial None -> Created state record"
                    .into(),
            ));
        }
        if let Some(expected) = self.expected_version {
            if self.instance.version != expected.saturating_add(1) {
                return Err(WorkflowError::Validation(
                    "workflow instance update must advance exactly one version".into(),
                ));
            }
        }
        if let Some(record) = &self.state_record {
            record.validate()?;
            if record.instance_id != self.instance.id
                || record.sequence != self.instance.version
                || record.to_state != self.instance.state
            {
                return Err(WorkflowError::Validation(
                    "workflow state record does not match Instance commit".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn save_registration(
        &self,
        commit: &WorkflowRegistrationCommit,
        actor: &str,
    ) -> WorkflowResult<()>;
    async fn find_workflow(&self, id: Uuid) -> WorkflowResult<Option<WorkflowIdentity>>;
    async fn find_workflow_by_key(&self, key: &str) -> WorkflowResult<Option<WorkflowIdentity>>;
    async fn list_workflows(&self) -> WorkflowResult<Vec<WorkflowIdentity>>;
    async fn find_definition(
        &self,
        workflow_id: Uuid,
        version: u64,
    ) -> WorkflowResult<Option<WorkflowDefinition>>;
    async fn list_definitions(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowDefinition>>;

    async fn commit_instance(
        &self,
        commit: &WorkflowInstanceCommit,
        actor: &str,
    ) -> WorkflowResult<()>;
    async fn find_instance(&self, id: Uuid) -> WorkflowResult<Option<WorkflowInstance>>;
    async fn list_instances(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowInstance>>;
    async fn list_states(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowStateRecord>>;

    async fn save_snapshot(&self, value: &WorkflowSnapshot, actor: &str) -> WorkflowResult<()>;
    async fn find_snapshot(&self, id: Uuid) -> WorkflowResult<Option<WorkflowSnapshot>>;
    async fn list_snapshots(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowSnapshot>>;
}

pub trait WorkflowSnapshotStore: WorkflowStore {}
impl<T: WorkflowStore + ?Sized> WorkflowSnapshotStore for T {}

#[async_trait]
pub trait WorkflowVariableStore: Send + Sync {
    async fn load(&self, instance_id: Uuid) -> WorkflowResult<WorkflowVariables>;
    async fn save(
        &self,
        instance_id: Uuid,
        values: &WorkflowVariables,
        actor: &str,
    ) -> WorkflowResult<()>;
}

pub trait WorkflowDsl: Send + Sync {
    fn parse(&self, source: &str) -> WorkflowResult<WorkflowDefinition>;
}

#[derive(Default)]
pub struct InMemoryWorkflowVariableStore {
    values: RwLock<BTreeMap<Uuid, WorkflowVariables>>,
}

#[async_trait]
impl WorkflowVariableStore for InMemoryWorkflowVariableStore {
    async fn load(&self, instance_id: Uuid) -> WorkflowResult<WorkflowVariables> {
        Ok(self
            .values
            .read()
            .map_err(|_| WorkflowError::Internal("variable store lock poisoned".into()))?
            .get(&instance_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn save(
        &self,
        instance_id: Uuid,
        values: &WorkflowVariables,
        _actor: &str,
    ) -> WorkflowResult<()> {
        self.values
            .write()
            .map_err(|_| WorkflowError::Internal("variable store lock poisoned".into()))?
            .insert(instance_id, values.clone());
        Ok(())
    }
}

pub type SharedWorkflowStore = Arc<dyn WorkflowStore>;
pub type WorkflowExtensionData = BTreeMap<String, Value>;
