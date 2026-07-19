use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    Memory, MemoryClassification, MemoryEvent, MemoryIndexEntry, MemoryPolicyDefinition,
    MemoryQuery, MemoryRecallHit, MemorySnapshot, MemoryState, MemoryUpdate,
};
use crate::error::{MemoryError, MemoryResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryOperation {
    Remember,
    Recall,
    Update,
    Archive,
    Forget,
    Snapshot,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryStage {
    Classification,
    Policy,
    Persistence,
    Retrieval,
    Lifecycle,
    Snapshot,
}

#[derive(Debug, Clone)]
pub struct MemoryObservation {
    pub operation: MemoryOperation,
    pub stage: MemoryStage,
    pub success: bool,
    pub memory_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub namespace: String,
    pub state: Option<MemoryState>,
    pub actor: String,
    pub reason: String,
    pub occurred_at: DateTime<Utc>,
}

pub trait MemoryObserver: Send + Sync {
    fn on_observation(&self, observation: &MemoryObservation);
}

pub trait MemoryInterceptor: Send + Sync {
    fn before_remember(&self, _event: &mut MemoryEvent) -> MemoryResult<()> {
        Ok(())
    }

    fn before_update(&self, _memory: &Memory, _request: &mut MemoryUpdate) -> MemoryResult<()> {
        Ok(())
    }

    fn after_recall(
        &self,
        _query: &MemoryQuery,
        _hits: &mut Vec<MemoryRecallHit>,
    ) -> MemoryResult<()> {
        Ok(())
    }
}

pub trait MemoryClassifier: Send + Sync {
    fn classify(&self, event: &MemoryEvent) -> MemoryResult<MemoryClassification>;
}

pub trait MemoryIndexer: Send + Sync {
    fn index(&self, memory: &Memory) -> MemoryResult<MemoryIndexEntry>;
}

pub trait MemoryRetriever: Send + Sync {
    fn retrieve(
        &self,
        query: &MemoryQuery,
        candidates: Vec<Memory>,
    ) -> MemoryResult<Vec<MemoryRecallHit>>;
}

pub trait MemoryLifecycle: Send + Sync {
    fn transition(
        &self,
        memory: &mut Memory,
        next: MemoryState,
        actor: &str,
        reason: &str,
    ) -> MemoryResult<()>;
}

pub trait MemoryPolicy: Send + Sync {
    fn evaluate(
        &self,
        operation: MemoryOperation,
        memory: Option<&Memory>,
        event: Option<&MemoryEvent>,
        definition: Option<&MemoryPolicyDefinition>,
        actor: &str,
    ) -> MemoryResult<()>;
}

#[derive(Debug, Clone)]
pub struct MemoryCommit {
    pub memory: Memory,
    pub expected_version: Option<u64>,
    pub index: Option<MemoryIndexEntry>,
}

impl MemoryCommit {
    pub fn create(memory: Memory, index: MemoryIndexEntry) -> Self {
        Self {
            memory,
            expected_version: None,
            index: Some(index),
        }
    }

    pub fn update(memory: Memory, expected_version: u64, index: MemoryIndexEntry) -> Self {
        Self {
            memory,
            expected_version: Some(expected_version),
            index: Some(index),
        }
    }

    pub fn forget(memory: Memory, expected_version: u64) -> Self {
        Self {
            memory,
            expected_version: Some(expected_version),
            index: None,
        }
    }

    pub fn validate(&self) -> MemoryResult<()> {
        self.memory.validate()?;
        match (&self.index, self.memory.state) {
            (Some(index), state) if state != MemoryState::Forgotten => {
                index.validate_for(&self.memory)?
            }
            (None, MemoryState::Forgotten) => {}
            _ => {
                return Err(MemoryError::Validation(
                    "memory commit index does not match lifecycle state".into(),
                ))
            }
        }
        if let Some(expected) = self.expected_version {
            if self.memory.version != expected.saturating_add(1) {
                return Err(MemoryError::Validation(
                    "memory update version must advance exactly once".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait MemoryStore: Send + Sync {
    async fn commit_batch(&self, commits: &[MemoryCommit], actor: &str) -> MemoryResult<()>;

    async fn commit(&self, commit: &MemoryCommit, actor: &str) -> MemoryResult<()> {
        self.commit_batch(std::slice::from_ref(commit), actor).await
    }

    async fn forget(&self, commit: &MemoryCommit, actor: &str) -> MemoryResult<()>;
    async fn find_memory(&self, id: Uuid) -> MemoryResult<Option<Memory>>;
    async fn find_by_event(&self, event_id: Uuid) -> MemoryResult<Option<Memory>>;
    async fn list_namespace(&self, namespace: &str) -> MemoryResult<Vec<Memory>>;

    async fn save_snapshot(&self, snapshot: &MemorySnapshot, actor: &str) -> MemoryResult<()>;
    async fn find_snapshot(&self, id: Uuid) -> MemoryResult<Option<MemorySnapshot>>;
    async fn list_snapshots(&self, memory_id: Uuid) -> MemoryResult<Vec<MemorySnapshot>>;

    async fn save_policy(&self, policy: &MemoryPolicyDefinition, actor: &str) -> MemoryResult<()>;
    async fn find_policy(&self, id: Uuid) -> MemoryResult<Option<MemoryPolicyDefinition>>;
    async fn list_policies(&self) -> MemoryResult<Vec<MemoryPolicyDefinition>>;
}

pub type SharedMemoryStore = Arc<dyn MemoryStore>;
