use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{
    expires_at, DefaultMemoryClassifier, DefaultMemoryIndexer, DefaultMemoryLifecycle,
    EmbeddedMemoryPolicy, InMemoryMemoryStore, StructuredMemoryRetriever,
};
use crate::domain::{
    validate_actor, Memory, MemoryEvent, MemoryIndexEntry, MemoryPolicyDefinition, MemoryQuery,
    MemoryRecallHit, MemorySnapshot, MemoryState, MemoryUpdate, RememberResult,
};
use crate::error::{MemoryError, MemoryResult};
use crate::infrastructure::{
    MemoryClassifier, MemoryCommit, MemoryIndexer, MemoryInterceptor, MemoryLifecycle,
    MemoryObservation, MemoryObserver, MemoryOperation, MemoryPolicy, MemoryRetriever, MemoryStage,
    MemoryStore,
};

pub struct MemoryManagerBuilder {
    store: Arc<dyn MemoryStore>,
    classifier: Arc<dyn MemoryClassifier>,
    indexer: Arc<dyn MemoryIndexer>,
    retriever: Arc<dyn MemoryRetriever>,
    lifecycle: Arc<dyn MemoryLifecycle>,
    policy: Arc<dyn MemoryPolicy>,
    interceptors: Vec<Arc<dyn MemoryInterceptor>>,
    observers: Vec<Arc<dyn MemoryObserver>>,
}

impl Default for MemoryManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryMemoryStore::default()),
            classifier: Arc::new(DefaultMemoryClassifier),
            indexer: Arc::new(DefaultMemoryIndexer),
            retriever: Arc::new(StructuredMemoryRetriever),
            lifecycle: Arc::new(DefaultMemoryLifecycle),
            policy: Arc::new(EmbeddedMemoryPolicy),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl MemoryManagerBuilder {
    pub fn store(mut self, value: Arc<dyn MemoryStore>) -> Self {
        self.store = value;
        self
    }

    pub fn classifier(mut self, value: Arc<dyn MemoryClassifier>) -> Self {
        self.classifier = value;
        self
    }

    pub fn indexer(mut self, value: Arc<dyn MemoryIndexer>) -> Self {
        self.indexer = value;
        self
    }

    pub fn retriever(mut self, value: Arc<dyn MemoryRetriever>) -> Self {
        self.retriever = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn MemoryLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn MemoryPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn MemoryInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn MemoryObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> MemoryManager {
        MemoryManager {
            store: self.store,
            classifier: self.classifier,
            indexer: self.indexer,
            retriever: self.retriever,
            lifecycle: self.lifecycle,
            policy: self.policy,
            interceptors: self.interceptors,
            observers: self.observers,
        }
    }
}

pub struct MemoryManager {
    store: Arc<dyn MemoryStore>,
    classifier: Arc<dyn MemoryClassifier>,
    indexer: Arc<dyn MemoryIndexer>,
    retriever: Arc<dyn MemoryRetriever>,
    lifecycle: Arc<dyn MemoryLifecycle>,
    policy: Arc<dyn MemoryPolicy>,
    interceptors: Vec<Arc<dyn MemoryInterceptor>>,
    observers: Vec<Arc<dyn MemoryObserver>>,
}

impl MemoryManager {
    pub fn builder() -> MemoryManagerBuilder {
        MemoryManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn MemoryStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn remember(&self, mut event: MemoryEvent) -> MemoryResult<RememberResult> {
        event.validate()?;
        if let Some(existing) = self.store.find_by_event(event.id).await? {
            return Ok(RememberResult {
                event_id: event.id,
                memory: Some(existing),
                reason: "event was already remembered".into(),
            });
        }
        let identity = (
            event.id,
            event.namespace.clone(),
            event.source.clone(),
            event.occurred_at,
            event.actor.clone(),
        );
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_remember(&mut event)))
                .map_err(|_| MemoryError::Extension("memory interceptor panicked".into()))??;
        }
        event.validate()?;
        if identity
            != (
                event.id,
                event.namespace.clone(),
                event.source.clone(),
                event.occurred_at,
                event.actor.clone(),
            )
        {
            return Err(MemoryError::Validation(
                "memory interceptor changed event identity, scope, source or actor".into(),
            ));
        }
        let definition = match event.policy_id {
            Some(id) => Some(
                self.store
                    .find_policy(id)
                    .await?
                    .ok_or_else(|| MemoryError::NotFound(id.to_string()))?,
            ),
            None => None,
        };
        self.policy.evaluate(
            MemoryOperation::Remember,
            None,
            Some(&event),
            definition.as_ref(),
            &event.actor,
        )?;
        let classification = self.classifier.classify(&event)?;
        self.notify(
            MemoryOperation::Remember,
            MemoryStage::Classification,
            true,
            None,
            Some(event.id),
            &event.namespace,
            None,
            &event.actor,
            &classification.reason,
        );
        if !classification.remember {
            return Ok(RememberResult {
                event_id: event.id,
                memory: None,
                reason: classification.reason,
            });
        }
        let expiry = expires_at(&event, classification.importance, definition.as_ref());
        let mut memory =
            Memory::from_event(event.clone(), classification, definition.clone(), expiry)?;
        self.policy.evaluate(
            MemoryOperation::Remember,
            Some(&memory),
            Some(&event),
            definition.as_ref(),
            &event.actor,
        )?;
        self.transition_memory(
            &mut memory,
            MemoryState::Verified,
            &event.actor,
            "memory classification verified",
        )?;
        self.transition_memory(
            &mut memory,
            MemoryState::Indexed,
            &event.actor,
            "memory indexed for structured recall",
        )?;
        let index = self.indexer.index(&memory)?;
        let commit = MemoryCommit::create(memory.clone(), index);
        if let Err(error) = self.store.commit(&commit, &event.actor).await {
            if matches!(error, MemoryError::Conflict(_)) {
                if let Some(existing) = self.store.find_by_event(event.id).await? {
                    return Ok(RememberResult {
                        event_id: event.id,
                        memory: Some(existing),
                        reason: "concurrent event delivery was idempotently reused".into(),
                    });
                }
            }
            return Err(error);
        }
        self.notify_memory(
            MemoryOperation::Remember,
            MemoryStage::Persistence,
            true,
            &memory,
            "memory stored",
        );
        Ok(RememberResult {
            event_id: event.id,
            memory: Some(memory),
            reason: "memory stored".into(),
        })
    }

    pub async fn recall(&self, query: MemoryQuery) -> MemoryResult<Vec<MemoryRecallHit>> {
        query.validate()?;
        let candidates = self.store.list_namespace(&query.namespace).await?;
        let originals = candidates
            .iter()
            .cloned()
            .map(|memory| (memory.id, memory))
            .collect::<HashMap<_, _>>();
        let mut hits = self.retriever.retrieve(&query, candidates)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.after_recall(&query, &mut hits)
            }))
            .map_err(|_| MemoryError::Extension("memory interceptor panicked".into()))??;
        }
        if hits.len() > query.limit {
            return Err(MemoryError::Validation(
                "memory interceptor exceeded recall limit".into(),
            ));
        }
        let mut seen = HashSet::new();
        for hit in &hits {
            hit.memory.validate()?;
            if !seen.insert(hit.memory.id)
                || originals.get(&hit.memory.id) != Some(&hit.memory)
                || hit.memory.namespace != query.namespace
            {
                return Err(MemoryError::Validation(
                    "memory interceptor changed recall identity or content".into(),
                ));
            }
            self.policy.evaluate(
                MemoryOperation::Recall,
                Some(&hit.memory),
                None,
                hit.memory.policy.as_ref(),
                &query.actor,
            )?;
        }
        let mut commits = Vec::new();
        for hit in &mut hits {
            if hit.memory.state.is_recallable() {
                let expected = hit.memory.version;
                self.transition_memory(
                    &mut hit.memory,
                    MemoryState::Recalled,
                    &query.actor,
                    "memory recalled by structured query",
                )?;
                let index = self.indexer.index(&hit.memory)?;
                commits.push(MemoryCommit::update(hit.memory.clone(), expected, index));
            }
        }
        self.store.commit_batch(&commits, &query.actor).await?;
        self.notify(
            MemoryOperation::Recall,
            MemoryStage::Retrieval,
            true,
            None,
            None,
            &query.namespace,
            None,
            &query.actor,
            &format!("{} memories recalled", hits.len()),
        );
        Ok(hits)
    }

    pub async fn update(&self, id: Uuid, mut request: MemoryUpdate) -> MemoryResult<Memory> {
        request.validate()?;
        let current = self.required(id).await?;
        self.policy.evaluate(
            MemoryOperation::Update,
            Some(&current),
            None,
            current.policy.as_ref(),
            &request.actor,
        )?;
        let operation_identity = (request.expected_version, request.actor.clone());
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.before_update(&current, &mut request)
            }))
            .map_err(|_| MemoryError::Extension("memory interceptor panicked".into()))??;
        }
        request.validate()?;
        if operation_identity != (request.expected_version, request.actor.clone()) {
            return Err(MemoryError::Validation(
                "memory interceptor changed update version or actor".into(),
            ));
        }
        if current.version != request.expected_version {
            return Err(MemoryError::Conflict(format!(
                "memory {id} expected version {}, found {}",
                request.expected_version, current.version
            )));
        }
        let mut memory = current;
        if let Some(value) = request.content {
            memory.content = value;
        }
        if let Some(value) = request.memory_type {
            memory.memory_type = value;
        }
        if let Some(value) = request.kind {
            memory.kind = value;
        }
        if let Some(value) = request.importance {
            memory.importance = value;
        }
        if let Some(value) = request.confidence {
            memory.confidence = value;
        }
        if let Some(value) = request.tags {
            memory.tags = value;
        }
        if let Some(value) = request.metadata {
            memory.metadata = value;
        }
        self.transition_memory(
            &mut memory,
            MemoryState::Updated,
            &request.actor,
            &request.reason,
        )?;
        self.commit_update(memory, request.expected_version, MemoryOperation::Update)
            .await
    }

    pub async fn archive(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> MemoryResult<Memory> {
        let mut memory = self.required(id).await?;
        self.policy.evaluate(
            MemoryOperation::Archive,
            Some(&memory),
            None,
            memory.policy.as_ref(),
            actor,
        )?;
        if memory.version != expected_version {
            return Err(MemoryError::Conflict(format!(
                "memory {id} expected version {expected_version}, found {}",
                memory.version
            )));
        }
        self.transition_memory(&mut memory, MemoryState::Archived, actor, "memory archived")?;
        self.commit_update(memory, expected_version, MemoryOperation::Archive)
            .await
    }

    pub async fn forget(
        &self,
        id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> MemoryResult<Memory> {
        let mut memory = self.required(id).await?;
        self.policy.evaluate(
            MemoryOperation::Forget,
            Some(&memory),
            None,
            memory.policy.as_ref(),
            actor,
        )?;
        if memory.version != expected_version {
            return Err(MemoryError::Conflict(format!(
                "memory {id} expected version {expected_version}, found {}",
                memory.version
            )));
        }
        memory.content = crate::domain::MemoryContent::forgotten();
        memory.metadata.clear();
        memory.tags.clear();
        memory.confidence = 0.0;
        memory.expires_at = None;
        self.transition_memory(
            &mut memory,
            MemoryState::Forgotten,
            actor,
            "memory content forgotten and indexes purged",
        )?;
        let commit = MemoryCommit::forget(memory.clone(), expected_version);
        self.store.forget(&commit, actor).await?;
        self.notify_memory(
            MemoryOperation::Forget,
            MemoryStage::Persistence,
            true,
            &memory,
            "memory forgotten",
        );
        Ok(memory)
    }

    pub async fn save_snapshot(
        &self,
        id: Uuid,
        label: impl Into<String>,
        actor: &str,
    ) -> MemoryResult<MemorySnapshot> {
        let memory = self.required(id).await?;
        self.policy.evaluate(
            MemoryOperation::Snapshot,
            Some(&memory),
            None,
            memory.policy.as_ref(),
            actor,
        )?;
        let snapshot = MemorySnapshot::new(&memory, label)?;
        self.store.save_snapshot(&snapshot, actor).await?;
        self.notify_memory(
            MemoryOperation::Snapshot,
            MemoryStage::Snapshot,
            true,
            &memory,
            "memory snapshot saved",
        );
        Ok(snapshot)
    }

    pub async fn restore_snapshot(
        &self,
        snapshot_id: Uuid,
        expected_version: u64,
        actor: &str,
    ) -> MemoryResult<Memory> {
        let snapshot = self
            .store
            .find_snapshot(snapshot_id)
            .await?
            .ok_or_else(|| MemoryError::NotFound(snapshot_id.to_string()))?;
        snapshot.validate()?;
        let current = self.required(snapshot.memory_id).await?;
        self.policy.evaluate(
            MemoryOperation::Restore,
            Some(&current),
            None,
            current.policy.as_ref(),
            actor,
        )?;
        if current.version != expected_version {
            return Err(MemoryError::Conflict(format!(
                "memory {} expected version {expected_version}, found {}",
                current.id, current.version
            )));
        }
        let mut restored = current;
        restored.kind = snapshot.content.kind;
        restored.memory_type = snapshot.content.memory_type;
        restored.content = snapshot.content.content;
        restored.metadata = snapshot.content.metadata;
        restored.importance = snapshot.content.importance;
        restored.confidence = snapshot.content.confidence;
        restored.tags = snapshot.content.tags;
        restored.expires_at = snapshot.content.expires_at;
        self.transition_memory(
            &mut restored,
            MemoryState::Updated,
            actor,
            "memory restored from snapshot as a new version",
        )?;
        self.commit_update(restored, expected_version, MemoryOperation::Restore)
            .await
    }

    pub async fn register_policy(
        &self,
        policy: MemoryPolicyDefinition,
        actor: &str,
    ) -> MemoryResult<MemoryPolicyDefinition> {
        validate_actor(actor)?;
        policy.validate()?;
        self.store.save_policy(&policy, actor).await?;
        Ok(policy)
    }

    pub async fn find(&self, id: Uuid) -> MemoryResult<Option<Memory>> {
        self.store.find_memory(id).await
    }

    pub async fn list(&self, namespace: &str) -> MemoryResult<Vec<Memory>> {
        crate::domain::validate_key("memory namespace", namespace)?;
        self.store.list_namespace(namespace).await
    }

    pub async fn list_snapshots(&self, id: Uuid) -> MemoryResult<Vec<MemorySnapshot>> {
        self.store.list_snapshots(id).await
    }

    pub async fn find_policy(&self, id: Uuid) -> MemoryResult<Option<MemoryPolicyDefinition>> {
        self.store.find_policy(id).await
    }

    pub async fn list_policies(&self) -> MemoryResult<Vec<MemoryPolicyDefinition>> {
        self.store.list_policies().await
    }

    async fn required(&self, id: Uuid) -> MemoryResult<Memory> {
        self.store
            .find_memory(id)
            .await?
            .ok_or_else(|| MemoryError::NotFound(id.to_string()))
    }

    async fn commit_update(
        &self,
        memory: Memory,
        expected_version: u64,
        operation: MemoryOperation,
    ) -> MemoryResult<Memory> {
        let actor = memory.actor.clone();
        let index: MemoryIndexEntry = self.indexer.index(&memory)?;
        let commit = MemoryCommit::update(memory.clone(), expected_version, index);
        self.store.commit(&commit, &actor).await?;
        self.notify_memory(
            operation,
            MemoryStage::Persistence,
            true,
            &memory,
            "memory change committed",
        );
        Ok(memory)
    }

    fn transition_memory(
        &self,
        memory: &mut Memory,
        next: MemoryState,
        actor: &str,
        reason: &str,
    ) -> MemoryResult<()> {
        let before = memory.clone();
        self.lifecycle.transition(memory, next, actor, reason)?;
        let expected_version = before
            .version
            .checked_add(1)
            .ok_or_else(|| MemoryError::Validation("memory version is exhausted".into()))?;
        let recall_changed_correctly = if next == MemoryState::Recalled {
            memory.recall_count == before.recall_count.saturating_add(1)
                && memory.last_recalled_at.is_some()
        } else {
            memory.recall_count == before.recall_count
                && memory.last_recalled_at == before.last_recalled_at
        };
        if memory.id != before.id
            || memory.event_id != before.event_id
            || memory.namespace != before.namespace
            || memory.kind != before.kind
            || memory.memory_type != before.memory_type
            || memory.content != before.content
            || memory.metadata != before.metadata
            || memory.source != before.source
            || memory.importance != before.importance
            || memory.confidence != before.confidence
            || memory.tags != before.tags
            || memory.policy != before.policy
            || memory.expires_at != before.expires_at
            || memory.created_at != before.created_at
            || memory.state != next
            || memory.version != expected_version
            || memory.updated_at < before.updated_at
            || memory.actor != actor
            || memory.reason != reason
            || !recall_changed_correctly
        {
            return Err(MemoryError::Validation(
                "memory lifecycle changed fields outside its transition ownership".into(),
            ));
        }
        memory.validate()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        operation: MemoryOperation,
        stage: MemoryStage,
        success: bool,
        memory_id: Option<Uuid>,
        event_id: Option<Uuid>,
        namespace: &str,
        state: Option<MemoryState>,
        actor: &str,
        reason: &str,
    ) {
        let observation = MemoryObservation {
            operation,
            stage,
            success,
            memory_id,
            event_id,
            namespace: namespace.into(),
            state,
            actor: actor.into(),
            reason: reason.into(),
            occurred_at: Utc::now(),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }

    fn notify_memory(
        &self,
        operation: MemoryOperation,
        stage: MemoryStage,
        success: bool,
        memory: &Memory,
        reason: &str,
    ) {
        self.notify(
            operation,
            stage,
            success,
            Some(memory.id),
            Some(memory.event_id),
            &memory.namespace,
            Some(memory.state),
            &memory.actor,
            reason,
        );
    }
}
