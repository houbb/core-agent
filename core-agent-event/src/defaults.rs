use std::collections::{BTreeSet, HashMap};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use futures::FutureExt;
use uuid::Uuid;

use crate::domain::{
    validate_actor, DeliveryState, EventDeadLetter, EventDefinition, EventEnvelope,
    EventPolicyDefinition, EventReplayRecord, EventState, EventSubscription, EventVisibility,
    ReplayState,
};
use crate::error::{EventError, EventResult};
use crate::infrastructure::{
    DeadLetterQueue, EventBus, EventCommit, EventDeliveryContext, EventDispatcher, EventHandler,
    EventLifecycle, EventOperation, EventPolicy, EventRegistry, EventRouter, EventStore,
};

#[derive(Default)]
pub struct InMemoryEventRegistry {
    definitions: RwLock<HashMap<String, EventDefinition>>,
}

impl EventRegistry for InMemoryEventRegistry {
    fn register(&self, definition: EventDefinition) -> EventResult<()> {
        definition.validate()?;
        let mut values = self
            .definitions
            .write()
            .map_err(|_| EventError::Internal("event registry lock poisoned".into()))?;
        if let Some(current) = values.get(&definition.key) {
            if current.category == definition.category
                && current.payload_type == definition.payload_type
                && current.schema_version == definition.schema_version
                && current.description == definition.description
                && current.active == definition.active
            {
                return Ok(());
            }
            return Err(EventError::Conflict(format!(
                "event type {} is already registered",
                definition.key
            )));
        }
        values.insert(definition.key.clone(), definition);
        Ok(())
    }

    fn find(&self, key: &str) -> EventResult<Option<EventDefinition>> {
        Ok(self
            .definitions
            .read()
            .map_err(|_| EventError::Internal("event registry lock poisoned".into()))?
            .get(key)
            .cloned())
    }

    fn list(&self) -> EventResult<Vec<EventDefinition>> {
        let mut values = self
            .definitions
            .read()
            .map_err(|_| EventError::Internal("event registry lock poisoned".into()))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }
}

#[derive(Default)]
pub struct InMemoryEventBus {
    handlers: RwLock<HashMap<Uuid, Arc<dyn EventHandler>>>,
}

impl EventBus for InMemoryEventBus {
    fn bind(&self, subscription_id: Uuid, handler: Arc<dyn EventHandler>) -> EventResult<()> {
        let mut handlers = self
            .handlers
            .write()
            .map_err(|_| EventError::Internal("event bus lock poisoned".into()))?;
        if handlers.contains_key(&subscription_id) {
            return Err(EventError::Conflict(format!(
                "subscription {subscription_id} already has a live handler"
            )));
        }
        handlers.insert(subscription_id, handler);
        Ok(())
    }

    fn unbind(&self, subscription_id: Uuid) -> EventResult<()> {
        self.handlers
            .write()
            .map_err(|_| EventError::Internal("event bus lock poisoned".into()))?
            .remove(&subscription_id);
        Ok(())
    }

    fn handler(&self, subscription_id: Uuid) -> EventResult<Option<Arc<dyn EventHandler>>> {
        Ok(self
            .handlers
            .read()
            .map_err(|_| EventError::Internal("event bus lock poisoned".into()))?
            .get(&subscription_id)
            .cloned())
    }
}

pub struct DefaultEventRouter;

impl EventRouter for DefaultEventRouter {
    fn route(
        &self,
        event: &EventEnvelope,
        subscriptions: Vec<EventSubscription>,
    ) -> EventResult<Vec<EventSubscription>> {
        event.validate()?;
        let mut values = subscriptions
            .into_iter()
            .filter(|subscription| subscription.matches(event))
            .collect::<Vec<_>>();
        for value in &values {
            value.validate()?;
        }
        values.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.key.cmp(&right.key))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(values)
    }
}

pub struct LocalEventDispatcher;

#[async_trait]
impl EventDispatcher for LocalEventDispatcher {
    async fn dispatch(
        &self,
        handler: Arc<dyn EventHandler>,
        event: &EventEnvelope,
        context: &EventDeliveryContext,
    ) -> EventResult<()> {
        AssertUnwindSafe(handler.handle(event, context))
            .catch_unwind()
            .await
            .map_err(|_| EventError::Handler("event handler panicked".into()))?
    }
}

pub struct EmbeddedEventPolicy;

impl EventPolicy for EmbeddedEventPolicy {
    fn check(
        &self,
        operation: EventOperation,
        event: Option<&EventEnvelope>,
        _subscription: Option<&EventSubscription>,
        definition: Option<&EventPolicyDefinition>,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        if let Some(definition) = definition {
            definition.validate()?;
        }
        if operation == EventOperation::Replay
            && definition.is_some_and(|value| !value.allow_replay)
        {
            return Err(EventError::PolicyDenied(
                "event replay is disabled by policy".into(),
            ));
        }
        if let Some(event) = event {
            event.validate()?;
            if event.sensitive
                && event.visibility == EventVisibility::External
                && !definition.is_some_and(|value| value.allow_sensitive_external)
            {
                return Err(EventError::PolicyDenied(
                    "sensitive event cannot be delivered externally".into(),
                ));
            }
            if let Some(definition) = definition {
                if !definition.allowed_categories.is_empty()
                    && !definition.allowed_categories.contains(&event.category)
                {
                    return Err(EventError::PolicyDenied(
                        "event category is not allowed by policy".into(),
                    ));
                }
                if !definition.allowed_sources.is_empty()
                    && !definition.allowed_sources.contains(&event.source.kind)
                {
                    return Err(EventError::PolicyDenied(
                        "event source is not allowed by policy".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

pub struct DefaultEventLifecycle;

impl EventLifecycle for DefaultEventLifecycle {
    fn transition(
        &self,
        event: &mut EventEnvelope,
        next: EventState,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        let allowed = matches!(
            (event.state, next),
            (EventState::Created, EventState::Published)
                | (
                    EventState::Published,
                    EventState::Dispatched | EventState::Archived
                )
                | (
                    EventState::Dispatched,
                    EventState::Delivered | EventState::Archived
                )
                | (
                    EventState::Delivered,
                    EventState::Handled | EventState::Archived
                )
                | (EventState::Handled, EventState::Archived)
        );
        if !allowed {
            return Err(EventError::InvalidState(format!(
                "cannot transition {} event to {}",
                event.state.as_str(),
                next.as_str()
            )));
        }
        event.state = next;
        event.version = event.version.saturating_add(1);
        event.actor = actor.into();
        event.updated_at = Utc::now().max(event.updated_at);
        event.validate()
    }
}

#[derive(Clone, Default)]
struct InMemoryState {
    events: HashMap<Uuid, EventEnvelope>,
    subscriptions: HashMap<Uuid, EventSubscription>,
    subscription_keys: HashMap<String, Uuid>,
    replays: HashMap<Uuid, EventReplayRecord>,
    policies: HashMap<Uuid, EventPolicyDefinition>,
    policy_keys: HashMap<String, Uuid>,
    dead_letters: HashMap<Uuid, EventDeadLetter>,
}

#[derive(Default)]
pub struct InMemoryEventStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryEventStore {
    fn read(&self) -> EventResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| EventError::Internal("event store lock poisoned".into()))
    }

    fn write(&self) -> EventResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| EventError::Internal("event store lock poisoned".into()))
    }
}

#[async_trait]
impl DeadLetterQueue for InMemoryEventStore {
    async fn save_dead_letter(&self, value: &EventDeadLetter, actor: &str) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        validate_dead_letter_ownership(&state, value)?;
        if state.dead_letters.contains_key(&value.id) {
            return Err(EventError::Conflict(format!(
                "dead letter {} already exists",
                value.id
            )));
        }
        state.dead_letters.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_dead_letter(&self, id: Uuid) -> EventResult<Option<EventDeadLetter>> {
        Ok(self.read()?.dead_letters.get(&id).cloned())
    }

    async fn list_dead_letters(&self, event_id: Uuid) -> EventResult<Vec<EventDeadLetter>> {
        let mut values = self
            .read()?
            .dead_letters
            .values()
            .filter(|value| value.event_id == event_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.created_at, value.id));
        Ok(values)
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn commit_event(
        &self,
        commit: &EventCommit,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        match commit.expected_version {
            None => {
                if next.events.contains_key(&commit.event.id) {
                    return Err(EventError::Conflict(format!(
                        "event {} already exists",
                        commit.event.id
                    )));
                }
            }
            Some(expected) => {
                let current = next
                    .events
                    .get(&commit.event.id)
                    .ok_or_else(|| EventError::NotFound(commit.event.id.to_string()))?;
                validate_event_update(current, &commit.event)?;
                if current.version != expected {
                    return Err(EventError::Conflict(format!(
                        "event {} expected version {expected}, found {}",
                        current.id, current.version
                    )));
                }
            }
        }
        next.events.insert(commit.event.id, commit.event.clone());
        for value in dead_letters {
            value.validate()?;
            validate_dead_letter_ownership(&next, value)?;
            if value.event_id != commit.event.id
                || value.replay_id.is_some()
                || next.dead_letters.contains_key(&value.id)
            {
                return Err(EventError::Conflict(
                    "event dead-letter identity is invalid or duplicated".into(),
                ));
            }
            next.dead_letters.insert(value.id, value.clone());
        }
        *state = next;
        Ok(())
    }

    async fn find_event(&self, id: Uuid) -> EventResult<Option<EventEnvelope>> {
        Ok(self.read()?.events.get(&id).cloned())
    }

    async fn list_events(&self, namespace: &str) -> EventResult<Vec<EventEnvelope>> {
        let mut values = self
            .read()?
            .events
            .values()
            .filter(|value| value.namespace == namespace)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (std::cmp::Reverse(value.occurred_at), value.id));
        Ok(values)
    }

    async fn save_subscription(
        &self,
        value: &EventSubscription,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        match expected_version {
            None => {
                if state.subscriptions.contains_key(&value.id)
                    || state.subscription_keys.contains_key(&value.key)
                {
                    return Err(EventError::Conflict(
                        "event subscription identity already exists".into(),
                    ));
                }
            }
            Some(expected) => {
                let current = state
                    .subscriptions
                    .get(&value.id)
                    .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
                validate_subscription_update(current, value, expected)?;
            }
        }
        state.subscription_keys.insert(value.key.clone(), value.id);
        state.subscriptions.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_subscription(&self, id: Uuid) -> EventResult<Option<EventSubscription>> {
        Ok(self.read()?.subscriptions.get(&id).cloned())
    }

    async fn list_subscriptions(&self, namespace: &str) -> EventResult<Vec<EventSubscription>> {
        let mut values = self
            .read()?
            .subscriptions
            .values()
            .filter(|value| value.namespace == namespace)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| {
            (
                std::cmp::Reverse(value.priority),
                value.key.clone(),
                value.id,
            )
        });
        Ok(values)
    }

    async fn save_replay(
        &self,
        value: &EventReplayRecord,
        expected_version: Option<u64>,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        match expected_version {
            None if next.replays.contains_key(&value.id) => {
                return Err(EventError::Conflict(format!(
                    "event replay {} already exists",
                    value.id
                )))
            }
            Some(expected) => {
                let current = next
                    .replays
                    .get(&value.id)
                    .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
                validate_replay_update(current, value, expected)?;
            }
            None => {}
        }
        next.replays.insert(value.id, value.clone());
        for dead_letter in dead_letters {
            dead_letter.validate()?;
            validate_dead_letter_ownership(&next, dead_letter)?;
            if dead_letter.replay_id != Some(value.id)
                || next.dead_letters.contains_key(&dead_letter.id)
            {
                return Err(EventError::Conflict(
                    "event replay dead-letter identity is invalid or duplicated".into(),
                ));
            }
            next.dead_letters
                .insert(dead_letter.id, dead_letter.clone());
        }
        *state = next;
        Ok(())
    }

    async fn find_replay(&self, id: Uuid) -> EventResult<Option<EventReplayRecord>> {
        Ok(self.read()?.replays.get(&id).cloned())
    }

    async fn list_replays(&self, event_id: Uuid) -> EventResult<Vec<EventReplayRecord>> {
        let mut values = self
            .read()?
            .replays
            .values()
            .filter(|value| value.event_id == event_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.created_at, value.id));
        Ok(values)
    }

    async fn save_policy(
        &self,
        value: &EventPolicyDefinition,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let mut state = self.write()?;
        match expected_version {
            None => {
                if state.policies.contains_key(&value.id)
                    || state.policy_keys.contains_key(&value.key)
                {
                    return Err(EventError::Conflict(
                        "event policy identity already exists".into(),
                    ));
                }
            }
            Some(expected) => {
                let current = state
                    .policies
                    .get(&value.id)
                    .ok_or_else(|| EventError::NotFound(value.id.to_string()))?;
                validate_policy_update(current, value, expected)?;
            }
        }
        state.policy_keys.insert(value.key.clone(), value.id);
        state.policies.insert(value.id, value.clone());
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> EventResult<Option<EventPolicyDefinition>> {
        Ok(self.read()?.policies.get(&id).cloned())
    }

    async fn list_policies(&self) -> EventResult<Vec<EventPolicyDefinition>> {
        let mut values = self.read()?.policies.values().cloned().collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }
}

fn validate_event_update(current: &EventEnvelope, next: &EventEnvelope) -> EventResult<()> {
    if current.id != next.id
        || current.event_type != next.event_type
        || current.category != next.category
        || current.namespace != next.namespace
        || current.source != next.source
        || current.target != next.target
        || current.payload != next.payload
        || current.payload_type != next.payload_type
        || current.metadata != next.metadata
        || current.priority != next.priority
        || current.visibility != next.visibility
        || current.sensitive != next.sensitive
        || current.schema_version != next.schema_version
        || current.policy_id != next.policy_id
        || current.occurred_at != next.occurred_at
        || current.created_at != next.created_at
    {
        return Err(EventError::Validation(
            "event update changed immutable identity, scope or payload".into(),
        ));
    }
    Ok(())
}

fn validate_dead_letter_ownership(
    state: &InMemoryState,
    value: &EventDeadLetter,
) -> EventResult<()> {
    let event = state
        .events
        .get(&value.event_id)
        .ok_or_else(|| EventError::NotFound(value.event_id.to_string()))?;
    if event.payload_hash()? != value.payload_hash
        || !state.subscriptions.contains_key(&value.subscription_id)
        || value.replay_id.is_some_and(|id| {
            state
                .replays
                .get(&id)
                .is_none_or(|replay| replay.event_id != value.event_id)
        })
    {
        return Err(EventError::Validation(
            "event dead letter has an invalid event, subscription or replay owner".into(),
        ));
    }
    Ok(())
}

fn validate_subscription_update(
    current: &EventSubscription,
    next: &EventSubscription,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.namespace != next.namespace
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(EventError::Conflict(
            "event subscription identity or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_replay_update(
    current: &EventReplayRecord,
    next: &EventReplayRecord,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.event_id != next.event_id
        || current.subscription_ids != next.subscription_ids
        || current.reason != next.reason
        || current.actor != next.actor
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(EventError::Conflict(
            "event replay identity or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_policy_update(
    current: &EventPolicyDefinition,
    next: &EventPolicyDefinition,
    expected: u64,
) -> EventResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.updated_at <= current.updated_at
    {
        return Err(EventError::Conflict(
            "event policy identity or version conflict".into(),
        ));
    }
    Ok(())
}

pub(crate) fn transition_replay(
    value: &mut EventReplayRecord,
    next: ReplayState,
) -> EventResult<u64> {
    let allowed = matches!(
        (value.state, next),
        (ReplayState::Requested, ReplayState::Running)
            | (
                ReplayState::Running,
                ReplayState::Completed | ReplayState::Failed
            )
    );
    if !allowed {
        return Err(EventError::InvalidState(format!(
            "cannot transition {} replay to {}",
            value.state.as_str(),
            next.as_str()
        )));
    }
    let expected = value.version;
    value.state = next;
    value.version = value.version.saturating_add(1);
    value.updated_at = Utc::now().max(value.updated_at);
    value.validate()?;
    Ok(expected)
}

pub(crate) fn mark_delivery(
    value: &mut crate::domain::EventDelivery,
    state: DeliveryState,
    error: Option<String>,
) -> EventResult<()> {
    let now = Utc::now();
    match state {
        DeliveryState::Delivered => {
            value.attempts = value.attempts.saturating_add(1);
            value.delivered_at = Some(now);
            value.handled_at = None;
        }
        DeliveryState::Handled => value.handled_at = Some(now),
        DeliveryState::Failed | DeliveryState::DeadLettered | DeliveryState::Pending => {}
    }
    value.state = state;
    value.last_error = error;
    value.updated_at = now;
    value.validate()
}

pub(crate) fn routed_ids(values: &[EventSubscription]) -> BTreeSet<Uuid> {
    values.iter().map(|value| value.id).collect()
}
