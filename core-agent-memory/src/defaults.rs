use std::collections::{BTreeSet, HashMap};
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Memory, MemoryClassification, MemoryEvent, MemoryEventKind, MemoryImportance,
    MemoryIndexEntry, MemoryKind, MemoryPolicyDefinition, MemoryQuery, MemoryRecallHit,
    MemorySnapshot, MemorySourceKind, MemoryState, MemoryType,
};
use crate::error::{MemoryError, MemoryResult};
use crate::infrastructure::{
    MemoryClassifier, MemoryCommit, MemoryIndexer, MemoryLifecycle, MemoryOperation, MemoryPolicy,
    MemoryRetriever, MemoryStore,
};

pub struct DefaultMemoryClassifier;

impl MemoryClassifier for DefaultMemoryClassifier {
    fn classify(&self, event: &MemoryEvent) -> MemoryResult<MemoryClassification> {
        event.validate()?;
        let remember = event.kind != MemoryEventKind::TemporaryLog
            && !(event.source.kind == MemorySourceKind::Conversation
                && event.kind == MemoryEventKind::Observation);
        let memory_type = event.suggested_type.unwrap_or(match event.kind {
            MemoryEventKind::Outcome => MemoryType::Experience,
            MemoryEventKind::Preference => MemoryType::Preference,
            MemoryEventKind::Knowledge => MemoryType::Knowledge,
            MemoryEventKind::Fact => MemoryType::Fact,
            MemoryEventKind::Observation | MemoryEventKind::TemporaryLog => match event.source.kind
            {
                MemorySourceKind::Workspace => MemoryType::Workspace,
                _ => MemoryType::Observation,
            },
        });
        let kind = event.suggested_kind.unwrap_or(match memory_type {
            MemoryType::Experience | MemoryType::Observation => MemoryKind::Episodic,
            _ => MemoryKind::Semantic,
        });
        let importance = event.suggested_importance.unwrap_or(match event.kind {
            MemoryEventKind::Preference | MemoryEventKind::Knowledge => MemoryImportance::High,
            MemoryEventKind::Outcome | MemoryEventKind::Fact => MemoryImportance::Medium,
            MemoryEventKind::Observation => MemoryImportance::Low,
            MemoryEventKind::TemporaryLog => MemoryImportance::Temporary,
        });
        let value = MemoryClassification {
            remember,
            kind,
            memory_type,
            importance,
            confidence: if event.suggested_type.is_some() {
                0.9
            } else {
                0.75
            },
            tags: event.tags.clone(),
            reason: if remember {
                format!(
                    "{} event from {} classified as {}",
                    event.kind.as_str(),
                    event.source.kind.as_str(),
                    memory_type.as_str()
                )
            } else {
                "temporary or unclassified conversation event is not durable memory".into()
            },
        };
        value.validate()?;
        Ok(value)
    }
}

pub struct DefaultMemoryIndexer;

impl MemoryIndexer for DefaultMemoryIndexer {
    fn index(&self, memory: &Memory) -> MemoryResult<MemoryIndexEntry> {
        memory.validate()?;
        if memory.state == MemoryState::Forgotten {
            return Err(MemoryError::InvalidState(
                "forgotten memory cannot be indexed".into(),
            ));
        }
        let tag_text = memory.tags.iter().cloned().collect::<Vec<_>>().join(" ");
        let normalized_text = format!(
            "{}\n{}\n{}\n{}",
            memory.content.title, memory.content.body, memory.content.data, tag_text
        )
        .to_lowercase();
        let value = MemoryIndexEntry {
            id: memory.id,
            memory_id: memory.id,
            namespace: memory.namespace.clone(),
            normalized_text,
            kind: memory.kind,
            memory_type: memory.memory_type,
            source: memory.source.kind,
            importance: memory.importance,
            state: memory.state,
            workspace_id: memory.source.workspace_id,
            agent_id: memory.source.agent_id,
            goal_id: memory.source.goal_id,
            memory_version: memory.version,
            created_at: memory.created_at,
            updated_at: memory.updated_at,
        };
        value.validate_for(memory)?;
        Ok(value)
    }
}

pub struct StructuredMemoryRetriever;

impl MemoryRetriever for StructuredMemoryRetriever {
    fn retrieve(
        &self,
        query: &MemoryQuery,
        candidates: Vec<Memory>,
    ) -> MemoryResult<Vec<MemoryRecallHit>> {
        query.validate()?;
        let now = Utc::now();
        let needle = query.text.as_ref().map(|value| value.to_lowercase());
        let mut hits = Vec::new();
        for memory in candidates {
            memory.validate()?;
            if memory.namespace != query.namespace
                || memory.is_expired_at(now)
                || !(memory.state.is_recallable()
                    || query.include_archived && memory.state == MemoryState::Archived)
                || !query.kinds.is_empty() && !query.kinds.contains(&memory.kind)
                || !query.types.is_empty() && !query.types.contains(&memory.memory_type)
                || !query.sources.is_empty() && !query.sources.contains(&memory.source.kind)
                || query
                    .minimum_importance
                    .is_some_and(|minimum| memory.importance.rank() < minimum.rank())
                || !query.tags.is_subset(&memory.tags)
                || query
                    .workspace_id
                    .is_some_and(|id| memory.source.workspace_id != Some(id))
                || query
                    .agent_id
                    .is_some_and(|id| memory.source.agent_id != Some(id))
                || query
                    .goal_id
                    .is_some_and(|id| memory.source.goal_id != Some(id))
                || query
                    .created_after
                    .is_some_and(|time| memory.created_at < time)
                || query
                    .created_before
                    .is_some_and(|time| memory.created_at > time)
            {
                continue;
            }
            let mut score = memory.importance.rank() * 100 + (memory.confidence * 100.0) as i64;
            let mut matched_by = Vec::new();
            if let Some(needle) = &needle {
                let title = memory.content.title.to_lowercase();
                let body = memory.content.body.to_lowercase();
                let data = memory.content.data.to_string().to_lowercase();
                let tag_match = memory.tags.iter().any(|tag| tag.contains(needle));
                if title.contains(needle) {
                    score += 100;
                    matched_by.push("title".into());
                }
                if body.contains(needle) {
                    score += 60;
                    matched_by.push("body".into());
                }
                if data.contains(needle) {
                    score += 30;
                    matched_by.push("data".into());
                }
                if tag_match {
                    score += 120;
                    matched_by.push("tag".into());
                }
                if matched_by.is_empty() {
                    continue;
                }
            } else {
                matched_by.push("structured-filter".into());
            }
            let age = now
                .signed_duration_since(memory.updated_at)
                .num_days()
                .max(0);
            score += 30 - age.min(30);
            hits.push(MemoryRecallHit {
                memory,
                score,
                matched_by,
            });
        }
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| right.memory.updated_at.cmp(&left.memory.updated_at))
                .then_with(|| left.memory.id.cmp(&right.memory.id))
        });
        hits.truncate(query.limit);
        Ok(hits)
    }
}

pub struct DefaultMemoryLifecycle;

impl MemoryLifecycle for DefaultMemoryLifecycle {
    fn transition(
        &self,
        memory: &mut Memory,
        next: MemoryState,
        actor: &str,
        reason: &str,
    ) -> MemoryResult<()> {
        validate_actor(actor)?;
        if reason.trim().is_empty() || reason.len() > 2048 {
            return Err(MemoryError::Validation(
                "memory lifecycle reason is invalid".into(),
            ));
        }
        let allowed = matches!(
            (memory.state, next),
            (MemoryState::Created, MemoryState::Verified)
                | (MemoryState::Verified, MemoryState::Indexed)
                | (
                    MemoryState::Indexed | MemoryState::Recalled | MemoryState::Updated,
                    MemoryState::Recalled
                        | MemoryState::Updated
                        | MemoryState::Archived
                        | MemoryState::Forgotten
                )
                | (
                    MemoryState::Archived,
                    MemoryState::Updated | MemoryState::Forgotten
                )
        );
        if !allowed {
            return Err(MemoryError::InvalidState(format!(
                "cannot transition {} memory to {}",
                memory.state.as_str(),
                next.as_str()
            )));
        }
        let now = Utc::now();
        memory.state = next;
        memory.version = memory.version.saturating_add(1);
        memory.updated_at = now.max(memory.updated_at);
        memory.actor = actor.into();
        memory.reason = reason.into();
        if next == MemoryState::Recalled {
            memory.recall_count = memory.recall_count.saturating_add(1);
            memory.last_recalled_at = Some(now);
        }
        memory.validate()
    }
}

pub struct EmbeddedMemoryPolicy;

impl MemoryPolicy for EmbeddedMemoryPolicy {
    fn evaluate(
        &self,
        operation: MemoryOperation,
        memory: Option<&Memory>,
        event: Option<&MemoryEvent>,
        definition: Option<&MemoryPolicyDefinition>,
        actor: &str,
    ) -> MemoryResult<()> {
        validate_actor(actor)?;
        if let Some(definition) = definition {
            definition.validate()?;
        }
        if operation == MemoryOperation::Remember {
            if event.is_some_and(|value| value.sensitive)
                && !definition.is_some_and(|value| value.allow_sensitive)
            {
                return Err(MemoryError::PolicyDenied(
                    "sensitive Memory Event is not allowed".into(),
                ));
            }
            if let (Some(memory), Some(definition)) = (memory, definition) {
                if memory.confidence < definition.min_confidence {
                    return Err(MemoryError::PolicyDenied(
                        "memory confidence is below policy threshold".into(),
                    ));
                }
                if !definition.allowed_types.is_empty()
                    && !definition.allowed_types.contains(&memory.memory_type)
                {
                    return Err(MemoryError::PolicyDenied(
                        "memory type is not allowed by policy".into(),
                    ));
                }
                if !definition.allowed_sources.is_empty()
                    && !definition.allowed_sources.contains(&memory.source.kind)
                {
                    return Err(MemoryError::PolicyDenied(
                        "memory source is not allowed by policy".into(),
                    ));
                }
            }
        }
        if let Some(memory) = memory {
            memory.validate()?;
            if memory.state == MemoryState::Forgotten {
                return Err(MemoryError::InvalidState(
                    "forgotten memory is immutable".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct InMemoryState {
    memories: HashMap<Uuid, Memory>,
    events: HashMap<Uuid, Uuid>,
    indexes: HashMap<Uuid, MemoryIndexEntry>,
    snapshots: HashMap<Uuid, MemorySnapshot>,
    policies: HashMap<Uuid, MemoryPolicyDefinition>,
}

#[derive(Default)]
pub struct InMemoryMemoryStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryMemoryStore {
    fn read(&self) -> MemoryResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| MemoryError::Internal("memory store lock poisoned".into()))
    }

    fn write(&self) -> MemoryResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| MemoryError::Internal("memory store lock poisoned".into()))
    }
}

#[async_trait]
impl MemoryStore for InMemoryMemoryStore {
    async fn commit_batch(&self, commits: &[MemoryCommit], actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        if commits.is_empty() {
            return Ok(());
        }
        let mut state = self.write()?;
        let mut next = state.clone();
        let mut seen = BTreeSet::new();
        for commit in commits {
            commit.validate()?;
            if !seen.insert(commit.memory.id) {
                return Err(MemoryError::Conflict(
                    "memory batch contains duplicate identity".into(),
                ));
            }
            match commit.expected_version {
                None => {
                    if next.memories.contains_key(&commit.memory.id)
                        || next.events.contains_key(&commit.memory.event_id)
                    {
                        return Err(MemoryError::Conflict(
                            "memory or event identity already exists".into(),
                        ));
                    }
                }
                Some(expected) => {
                    let current = next
                        .memories
                        .get(&commit.memory.id)
                        .ok_or_else(|| MemoryError::NotFound(commit.memory.id.to_string()))?;
                    validate_update_identity(current, &commit.memory)?;
                    if current.version != expected {
                        return Err(MemoryError::Conflict(format!(
                            "memory {} expected version {expected}, found {}",
                            current.id, current.version
                        )));
                    }
                }
            }
            next.events.insert(commit.memory.event_id, commit.memory.id);
            next.memories
                .insert(commit.memory.id, commit.memory.clone());
            if let Some(index) = &commit.index {
                next.indexes.insert(commit.memory.id, index.clone());
            } else {
                next.indexes.remove(&commit.memory.id);
            }
            if commit.memory.state == MemoryState::Forgotten {
                next.snapshots
                    .retain(|_, snapshot| snapshot.memory_id != commit.memory.id);
            }
        }
        *state = next;
        Ok(())
    }

    async fn forget(&self, commit: &MemoryCommit, actor: &str) -> MemoryResult<()> {
        if commit.memory.state != MemoryState::Forgotten {
            return Err(MemoryError::Validation(
                "forget commit must contain a tombstone".into(),
            ));
        }
        self.commit_batch(std::slice::from_ref(commit), actor).await
    }

    async fn find_memory(&self, id: Uuid) -> MemoryResult<Option<Memory>> {
        Ok(self.read()?.memories.get(&id).cloned())
    }

    async fn find_by_event(&self, event_id: Uuid) -> MemoryResult<Option<Memory>> {
        let state = self.read()?;
        Ok(state
            .events
            .get(&event_id)
            .and_then(|id| state.memories.get(id))
            .cloned())
    }

    async fn list_namespace(&self, namespace: &str) -> MemoryResult<Vec<Memory>> {
        let mut values = self
            .read()?
            .memories
            .values()
            .filter(|memory| memory.namespace == namespace)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|memory| (std::cmp::Reverse(memory.updated_at), memory.id));
        Ok(values)
    }

    async fn save_snapshot(&self, snapshot: &MemorySnapshot, actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        snapshot.validate()?;
        let mut state = self.write()?;
        let current = state
            .memories
            .get(&snapshot.memory_id)
            .ok_or_else(|| MemoryError::NotFound(snapshot.memory_id.to_string()))?;
        if current != &snapshot.content {
            return Err(MemoryError::Conflict(
                "snapshot must match the current memory version".into(),
            ));
        }
        if state.snapshots.contains_key(&snapshot.id)
            || state.snapshots.values().any(|value| {
                value.memory_id == snapshot.memory_id
                    && value.memory_version == snapshot.memory_version
            })
        {
            return Err(MemoryError::Conflict(
                "memory snapshot identity or version already exists".into(),
            ));
        }
        state.snapshots.insert(snapshot.id, snapshot.clone());
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> MemoryResult<Option<MemorySnapshot>> {
        Ok(self.read()?.snapshots.get(&id).cloned())
    }

    async fn list_snapshots(&self, memory_id: Uuid) -> MemoryResult<Vec<MemorySnapshot>> {
        let mut values = self
            .read()?
            .snapshots
            .values()
            .filter(|value| value.memory_id == memory_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.created_at), value.id));
        Ok(values)
    }

    async fn save_policy(&self, policy: &MemoryPolicyDefinition, actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        policy.validate()?;
        let mut state = self.write()?;
        if let Some(current) = state.policies.get(&policy.id) {
            validate_policy_update(current, policy)?;
        } else if state.policies.values().any(|value| value.key == policy.key) {
            return Err(MemoryError::Conflict(format!(
                "memory policy key {} already exists",
                policy.key
            )));
        }
        state.policies.insert(policy.id, policy.clone());
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> MemoryResult<Option<MemoryPolicyDefinition>> {
        Ok(self.read()?.policies.get(&id).cloned())
    }

    async fn list_policies(&self) -> MemoryResult<Vec<MemoryPolicyDefinition>> {
        let mut values = self.read()?.policies.values().cloned().collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }
}

fn validate_update_identity(current: &Memory, next: &Memory) -> MemoryResult<()> {
    if current.id != next.id
        || current.event_id != next.event_id
        || current.namespace != next.namespace
        || current.source != next.source
        || current.policy != next.policy
        || current.created_at != next.created_at
    {
        return Err(MemoryError::Validation(
            "memory update changed immutable identity or ownership".into(),
        ));
    }
    Ok(())
}

fn validate_policy_update(
    current: &MemoryPolicyDefinition,
    next: &MemoryPolicyDefinition,
) -> MemoryResult<()> {
    if current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.version != current.version.saturating_add(1)
        || next.updated_at <= current.updated_at
    {
        return Err(MemoryError::Validation(
            "memory policy update changed identity or version sequence".into(),
        ));
    }
    Ok(())
}

pub(crate) fn expires_at(
    event: &MemoryEvent,
    importance: MemoryImportance,
    policy: Option<&MemoryPolicyDefinition>,
) -> Option<chrono::DateTime<Utc>> {
    let days = policy
        .map(|value| value.retention_for(importance))
        .unwrap_or(match importance {
            MemoryImportance::Critical => None,
            MemoryImportance::Temporary => Some(7),
            _ => Some(90),
        });
    days.map(|days| event.occurred_at + Duration::days(i64::from(days)))
}
