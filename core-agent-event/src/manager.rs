use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{
    mark_delivery, routed_ids, transition_replay, DefaultEventLifecycle, DefaultEventRouter,
    EmbeddedEventPolicy, InMemoryEventBus, InMemoryEventRegistry, InMemoryEventStore,
    LocalEventDispatcher,
};
use crate::domain::{
    validate_actor, DeliveryState, EventDeadLetter, EventDefinition, EventDelivery, EventEnvelope,
    EventPolicyDefinition, EventReplayRecord, EventState, EventSubscription, PublishOutcome,
    ReplayRequest, ReplayState,
};
use crate::error::{EventError, EventResult};
use crate::infrastructure::{
    EventBus, EventCommit, EventDeliveryContext, EventDispatcher, EventHandler, EventInterceptor,
    EventLifecycle, EventObservation, EventObserver, EventOperation, EventPolicy, EventRegistry,
    EventReplay, EventRouter, EventStage, EventStore,
};

pub struct EventManagerBuilder {
    store: Arc<dyn EventStore>,
    registry: Arc<dyn EventRegistry>,
    bus: Arc<dyn EventBus>,
    router: Arc<dyn EventRouter>,
    dispatcher: Arc<dyn EventDispatcher>,
    policy: Arc<dyn EventPolicy>,
    lifecycle: Arc<dyn EventLifecycle>,
    interceptors: Vec<Arc<dyn EventInterceptor>>,
    observers: Vec<Arc<dyn EventObserver>>,
}

impl Default for EventManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryEventStore::default()),
            registry: Arc::new(InMemoryEventRegistry::default()),
            bus: Arc::new(InMemoryEventBus::default()),
            router: Arc::new(DefaultEventRouter),
            dispatcher: Arc::new(LocalEventDispatcher),
            policy: Arc::new(EmbeddedEventPolicy),
            lifecycle: Arc::new(DefaultEventLifecycle),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl EventManagerBuilder {
    pub fn store(mut self, value: Arc<dyn EventStore>) -> Self {
        self.store = value;
        self
    }

    pub fn registry(mut self, value: Arc<dyn EventRegistry>) -> Self {
        self.registry = value;
        self
    }

    pub fn bus(mut self, value: Arc<dyn EventBus>) -> Self {
        self.bus = value;
        self
    }

    pub fn router(mut self, value: Arc<dyn EventRouter>) -> Self {
        self.router = value;
        self
    }

    pub fn dispatcher(mut self, value: Arc<dyn EventDispatcher>) -> Self {
        self.dispatcher = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn EventPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn EventLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn EventInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn EventObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> EventManager {
        EventManager {
            store: self.store,
            registry: self.registry,
            bus: self.bus,
            router: self.router,
            dispatcher: self.dispatcher,
            policy: self.policy,
            lifecycle: self.lifecycle,
            interceptors: self.interceptors,
            observers: self.observers,
        }
    }
}

pub struct EventManager {
    store: Arc<dyn EventStore>,
    registry: Arc<dyn EventRegistry>,
    bus: Arc<dyn EventBus>,
    router: Arc<dyn EventRouter>,
    dispatcher: Arc<dyn EventDispatcher>,
    policy: Arc<dyn EventPolicy>,
    lifecycle: Arc<dyn EventLifecycle>,
    interceptors: Vec<Arc<dyn EventInterceptor>>,
    observers: Vec<Arc<dyn EventObserver>>,
}

impl EventManager {
    pub fn builder() -> EventManagerBuilder {
        EventManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn EventStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub fn register_type(&self, definition: EventDefinition) -> EventResult<EventDefinition> {
        definition.validate()?;
        self.registry.register(definition.clone())?;
        self.notify(
            EventOperation::Register,
            EventStage::Registry,
            true,
            None,
            None,
            None,
            "registry",
            "system",
            &format!("event type {} registered", definition.key),
        );
        Ok(definition)
    }

    pub async fn register_policy(
        &self,
        value: EventPolicyDefinition,
        actor: &str,
    ) -> EventResult<EventPolicyDefinition> {
        validate_actor(actor)?;
        value.validate()?;
        let expected = self
            .store
            .find_policy(value.id)
            .await?
            .map(|item| item.version);
        self.store.save_policy(&value, expected, actor).await?;
        Ok(value)
    }

    pub async fn subscribe(
        &self,
        value: EventSubscription,
        handler: Arc<dyn EventHandler>,
    ) -> EventResult<EventSubscription> {
        value.validate()?;
        for event_type in &value.event_types {
            self.registry
                .find(event_type)?
                .ok_or_else(|| EventError::NotFound(event_type.clone()))?;
        }
        let definition = self.load_policy(value.policy_id).await?;
        self.policy.check(
            EventOperation::Subscribe,
            None,
            Some(&value),
            definition.as_ref(),
            &value.actor,
        )?;
        self.bus.bind(value.id, handler)?;
        if let Err(error) = self
            .store
            .save_subscription(&value, None, &value.actor)
            .await
        {
            let _ = self.bus.unbind(value.id);
            return Err(error);
        }
        self.notify(
            EventOperation::Subscribe,
            EventStage::Registry,
            true,
            None,
            Some(value.id),
            None,
            &value.namespace,
            &value.actor,
            "event subscription registered",
        );
        Ok(value)
    }

    pub async fn bind_existing(
        &self,
        subscription_id: Uuid,
        handler: Arc<dyn EventHandler>,
    ) -> EventResult<EventSubscription> {
        let value = self
            .store
            .find_subscription(subscription_id)
            .await?
            .ok_or_else(|| EventError::NotFound(subscription_id.to_string()))?;
        if !value.enabled {
            return Err(EventError::InvalidState(
                "disabled event subscription cannot bind a handler".into(),
            ));
        }
        self.bus.bind(subscription_id, handler)?;
        Ok(value)
    }

    pub async fn unsubscribe(&self, id: Uuid, actor: &str) -> EventResult<EventSubscription> {
        validate_actor(actor)?;
        let mut value = self
            .store
            .find_subscription(id)
            .await?
            .ok_or_else(|| EventError::NotFound(id.to_string()))?;
        let definition = self.load_policy(value.policy_id).await?;
        self.policy.check(
            EventOperation::Unsubscribe,
            None,
            Some(&value),
            definition.as_ref(),
            actor,
        )?;
        let expected = value.version;
        value.enabled = false;
        value.version = value
            .version
            .checked_add(1)
            .ok_or_else(|| EventError::Validation("subscription version is exhausted".into()))?;
        value.updated_at = Utc::now().max(value.updated_at);
        value.actor = actor.into();
        value.validate()?;
        self.store
            .save_subscription(&value, Some(expected), actor)
            .await?;
        self.bus.unbind(id)?;
        self.notify(
            EventOperation::Unsubscribe,
            EventStage::Registry,
            true,
            None,
            Some(id),
            None,
            &value.namespace,
            actor,
            "event subscription disabled",
        );
        Ok(value)
    }

    pub async fn publish(&self, mut event: EventEnvelope) -> EventResult<PublishOutcome> {
        event.validate()?;
        if event.state != EventState::Created || !event.deliveries.is_empty() {
            return Err(EventError::InvalidState(
                "only a new Created event can be published".into(),
            ));
        }
        if let Some(existing) = self.store.find_event(event.id).await? {
            ensure_same_event(&event, &existing)?;
            return self.resume_event(existing, &event.actor, true).await;
        }
        let identity = publish_identity(&event);
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_publish(&mut event)))
                .map_err(|_| EventError::Extension("event interceptor panicked".into()))??;
        }
        event.validate()?;
        if identity != publish_identity(&event) {
            return Err(EventError::Validation(
                "event interceptor changed identity, scope, source, target or actor".into(),
            ));
        }
        let type_definition = self
            .registry
            .find(&event.event_type)?
            .ok_or_else(|| EventError::NotFound(event.event_type.clone()))?;
        type_definition.validate_event(&event)?;
        let policy_definition = self.load_policy(event.policy_id).await?;
        self.policy.check(
            EventOperation::Publish,
            Some(&event),
            None,
            policy_definition.as_ref(),
            &event.actor,
        )?;
        let publish_actor = event.actor.clone();
        let routed = self
            .route_and_authorize(&event, policy_definition.as_ref(), &publish_actor)
            .await?;
        self.transition_event(&mut event, EventState::Published, &publish_actor)?;
        let create = EventCommit::create(event.clone());
        if let Err(error) = self.store.commit_event(&create, &[], &publish_actor).await {
            if matches!(error, EventError::Conflict(_)) {
                if let Some(existing) = self.store.find_event(event.id).await? {
                    ensure_same_event(&event, &existing)?;
                    return self.resume_event(existing, &publish_actor, true).await;
                }
            }
            return Err(error);
        }
        if routed.is_empty() {
            self.transition_and_commit(&mut event, EventState::Archived, &publish_actor)
                .await?;
            return Ok(PublishOutcome {
                event,
                handled: 0,
                dead_letters: Vec::new(),
                idempotent: false,
            });
        }
        event.deliveries = routed
            .iter()
            .map(|subscription| EventDelivery::new(subscription.id, None))
            .collect();
        self.transition_and_commit(&mut event, EventState::Dispatched, &publish_actor)
            .await?;
        self.resume_event(event, &publish_actor, false).await
    }

    pub async fn resume(&self, event_id: Uuid, actor: &str) -> EventResult<PublishOutcome> {
        validate_actor(actor)?;
        let event = self
            .store
            .find_event(event_id)
            .await?
            .ok_or_else(|| EventError::NotFound(event_id.to_string()))?;
        self.resume_event(event, actor, true).await
    }

    async fn route_and_authorize(
        &self,
        event: &EventEnvelope,
        policy_definition: Option<&EventPolicyDefinition>,
        actor: &str,
    ) -> EventResult<Vec<EventSubscription>> {
        let candidates = self.store.list_subscriptions(&event.namespace).await?;
        let originals = candidates
            .iter()
            .cloned()
            .map(|value| (value.id, value))
            .collect::<HashMap<_, _>>();
        let mut routed = self.router.route(event, candidates)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.after_route(event, &mut routed)
            }))
            .map_err(|_| EventError::Extension("event interceptor panicked".into()))??;
        }
        validate_routes(event, &routed, &originals)?;
        for subscription in &routed {
            self.policy.check(
                EventOperation::Deliver,
                Some(event),
                Some(subscription),
                policy_definition,
                actor,
            )?;
        }
        self.notify(
            EventOperation::Publish,
            EventStage::Routing,
            true,
            Some(event.id),
            None,
            None,
            &event.namespace,
            actor,
            &format!("event routed to {} subscriptions", routed.len()),
        );
        Ok(routed)
    }

    async fn resume_event(
        &self,
        mut event: EventEnvelope,
        actor: &str,
        idempotent: bool,
    ) -> EventResult<PublishOutcome> {
        validate_actor(actor)?;
        event.validate()?;
        let type_definition = self
            .registry
            .find(&event.event_type)?
            .ok_or_else(|| EventError::NotFound(event.event_type.clone()))?;
        type_definition.validate_event(&event)?;
        let policy_definition = self.load_policy(event.policy_id).await?;
        self.policy.check(
            EventOperation::Publish,
            Some(&event),
            None,
            policy_definition.as_ref(),
            actor,
        )?;

        match event.state {
            EventState::Created => {
                return Err(EventError::InvalidState(
                    "a stored Created event cannot be resumed".into(),
                ));
            }
            EventState::Published => {
                let routed = self
                    .route_and_authorize(&event, policy_definition.as_ref(), actor)
                    .await?;
                if routed.is_empty() {
                    self.transition_and_commit(&mut event, EventState::Archived, actor)
                        .await?;
                    return self.publish_outcome(event, idempotent).await;
                }
                event.deliveries = routed
                    .iter()
                    .map(|subscription| EventDelivery::new(subscription.id, None))
                    .collect();
                self.transition_and_commit(&mut event, EventState::Dispatched, actor)
                    .await?;
            }
            EventState::Handled => {
                self.transition_and_commit(&mut event, EventState::Archived, actor)
                    .await?;
                return self.publish_outcome(event, idempotent).await;
            }
            EventState::Archived => return self.publish_outcome(event, idempotent).await,
            EventState::Dispatched | EventState::Delivered => {}
        }

        let subscription_ids = event
            .deliveries
            .iter()
            .map(|delivery| delivery.subscription_id)
            .collect::<Vec<_>>();
        for subscription_id in subscription_ids {
            let mut delivery = event
                .deliveries
                .iter()
                .find(|delivery| delivery.subscription_id == subscription_id)
                .cloned()
                .ok_or_else(|| EventError::Internal("planned event delivery disappeared".into()))?;
            if matches!(
                delivery.state,
                DeliveryState::Handled | DeliveryState::DeadLettered
            ) {
                continue;
            }
            let subscription = self
                .store
                .find_subscription(subscription_id)
                .await?
                .ok_or_else(|| EventError::NotFound(subscription_id.to_string()))?;
            self.policy.check(
                EventOperation::Deliver,
                Some(&event),
                Some(&subscription),
                policy_definition.as_ref(),
                actor,
            )?;
            let max_attempts = policy_definition
                .as_ref()
                .map(|value| value.max_attempts)
                .unwrap_or(3)
                .min(subscription.max_attempts)
                .max(1);
            loop {
                if delivery.state == DeliveryState::Failed && delivery.attempts >= max_attempts {
                    let message = delivery
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "event delivery exhausted its retry budget".into());
                    self.dead_letter_delivery(&mut event, &mut delivery, actor, message)
                        .await?;
                    break;
                }
                if matches!(
                    delivery.state,
                    DeliveryState::Pending | DeliveryState::Failed
                ) {
                    mark_delivery(&mut delivery, DeliveryState::Delivered, None)?;
                    upsert_event_delivery(&mut event, delivery.clone());
                    if event.state == EventState::Dispatched {
                        self.transition_and_commit(&mut event, EventState::Delivered, actor)
                            .await?;
                    } else {
                        self.touch_and_commit(&mut event, actor, &[]).await?;
                    }
                }
                let context = EventDeliveryContext {
                    event_id: event.id,
                    subscription_id,
                    delivery_id: delivery.id,
                    replay_id: None,
                    attempt: delivery.attempts,
                    actor: actor.into(),
                };
                let result = match self.bus.handler(subscription_id)? {
                    Some(handler) => self.dispatcher.dispatch(handler, &event, &context).await,
                    None => Err(EventError::Handler(
                        "event subscription has no live handler".into(),
                    )),
                };
                match result {
                    Ok(()) => {
                        mark_delivery(&mut delivery, DeliveryState::Handled, None)?;
                        upsert_event_delivery(&mut event, delivery.clone());
                        self.touch_and_commit(&mut event, actor, &[]).await?;
                        self.notify_delivery(&event, &delivery, true, "event handled");
                        break;
                    }
                    Err(error) if delivery.attempts < max_attempts => {
                        let message = bounded_error(&error);
                        mark_delivery(&mut delivery, DeliveryState::Failed, Some(message.clone()))?;
                        upsert_event_delivery(&mut event, delivery.clone());
                        self.touch_and_commit(&mut event, actor, &[]).await?;
                        self.notify(
                            EventOperation::Deliver,
                            EventStage::Retry,
                            false,
                            Some(event.id),
                            Some(subscription_id),
                            None,
                            &event.namespace,
                            actor,
                            &message,
                        );
                    }
                    Err(error) => {
                        let message = bounded_error(&error);
                        self.dead_letter_delivery(&mut event, &mut delivery, actor, message)
                            .await?;
                        break;
                    }
                }
            }
        }
        let has_dead_letter = event
            .deliveries
            .iter()
            .any(|delivery| delivery.state == DeliveryState::DeadLettered);
        if !has_dead_letter {
            self.transition_and_commit(&mut event, EventState::Handled, actor)
                .await?;
        }
        self.transition_and_commit(&mut event, EventState::Archived, actor)
            .await?;
        self.publish_outcome(event, idempotent).await
    }

    async fn dead_letter_delivery(
        &self,
        event: &mut EventEnvelope,
        delivery: &mut EventDelivery,
        actor: &str,
        message: String,
    ) -> EventResult<()> {
        mark_delivery(delivery, DeliveryState::DeadLettered, Some(message.clone()))?;
        let dead_letter = EventDeadLetter::new(event, delivery, message.clone(), actor)?;
        upsert_event_delivery(event, delivery.clone());
        self.touch_and_commit(event, actor, std::slice::from_ref(&dead_letter))
            .await?;
        self.notify_delivery(event, delivery, false, &message);
        Ok(())
    }

    async fn publish_outcome(
        &self,
        event: EventEnvelope,
        idempotent: bool,
    ) -> EventResult<PublishOutcome> {
        let handled = event
            .deliveries
            .iter()
            .filter(|delivery| delivery.state == DeliveryState::Handled)
            .count();
        let dead_letters = self
            .store
            .list_dead_letters(event.id)
            .await?
            .into_iter()
            .filter(|dead_letter| dead_letter.replay_id.is_none())
            .collect();
        Ok(PublishOutcome {
            event,
            handled,
            dead_letters,
            idempotent,
        })
    }

    pub async fn replay(&self, request: ReplayRequest) -> EventResult<EventReplayRecord> {
        request.validate()?;
        let event = self
            .store
            .find_event(request.event_id)
            .await?
            .ok_or_else(|| EventError::NotFound(request.event_id.to_string()))?;
        if event.state != EventState::Archived {
            return Err(EventError::InvalidState(
                "only an Archived event can be replayed".into(),
            ));
        }
        let type_definition = self
            .registry
            .find(&event.event_type)?
            .ok_or_else(|| EventError::NotFound(event.event_type.clone()))?;
        type_definition.validate_event(&event)?;
        let policy_definition = self.load_policy(event.policy_id).await?;
        self.policy.check(
            EventOperation::Replay,
            Some(&event),
            None,
            policy_definition.as_ref(),
            &request.actor,
        )?;
        let requested = (!request.subscription_ids.is_empty()).then_some(&request.subscription_ids);
        let routed = self
            .route_replay(
                &event,
                requested,
                policy_definition.as_ref(),
                &request.actor,
            )
            .await?;
        let mut resolved_request = request.clone();
        resolved_request.subscription_ids = routed_ids(&routed);
        let mut replay = EventReplayRecord::new(event.id, &resolved_request)?;
        self.store
            .save_replay(&replay, None, &[], &request.actor)
            .await?;
        replay.deliveries = routed
            .iter()
            .map(|subscription| EventDelivery::new(subscription.id, Some(replay.id)))
            .collect();
        let expected = transition_replay(&mut replay, ReplayState::Running)?;
        self.store
            .save_replay(&replay, Some(expected), &[], &request.actor)
            .await?;
        self.resume_replay_record(replay, event, &request.actor)
            .await
    }

    pub async fn resume_replay(
        &self,
        replay_id: Uuid,
        actor: &str,
    ) -> EventResult<EventReplayRecord> {
        validate_actor(actor)?;
        let replay = self
            .store
            .find_replay(replay_id)
            .await?
            .ok_or_else(|| EventError::NotFound(replay_id.to_string()))?;
        let event = self
            .store
            .find_event(replay.event_id)
            .await?
            .ok_or_else(|| EventError::NotFound(replay.event_id.to_string()))?;
        self.resume_replay_record(replay, event, actor).await
    }

    async fn route_replay(
        &self,
        event: &EventEnvelope,
        requested: Option<&std::collections::BTreeSet<Uuid>>,
        policy_definition: Option<&EventPolicyDefinition>,
        actor: &str,
    ) -> EventResult<Vec<EventSubscription>> {
        let candidates = self.store.list_subscriptions(&event.namespace).await?;
        let originals = candidates
            .iter()
            .cloned()
            .map(|value| (value.id, value))
            .collect::<HashMap<_, _>>();
        let mut routed = self.router.route(event, candidates)?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.after_route(event, &mut routed)
            }))
            .map_err(|_| EventError::Extension("event interceptor panicked".into()))??;
        }
        validate_routes(event, &routed, &originals)?;
        if let Some(requested) = requested {
            routed.retain(|value| requested.contains(&value.id));
            if routed_ids(&routed) != *requested {
                return Err(EventError::Validation(
                    "event replay requested an unavailable or non-matching subscription".into(),
                ));
            }
        }
        for subscription in &routed {
            self.policy.check(
                EventOperation::Deliver,
                Some(event),
                Some(subscription),
                policy_definition,
                actor,
            )?;
        }
        Ok(routed)
    }

    async fn resume_replay_record(
        &self,
        mut replay: EventReplayRecord,
        event: EventEnvelope,
        actor: &str,
    ) -> EventResult<EventReplayRecord> {
        validate_actor(actor)?;
        replay.validate()?;
        event.validate()?;
        if event.state != EventState::Archived {
            return Err(EventError::InvalidState(
                "only an Archived event can continue replay".into(),
            ));
        }
        let type_definition = self
            .registry
            .find(&event.event_type)?
            .ok_or_else(|| EventError::NotFound(event.event_type.clone()))?;
        type_definition.validate_event(&event)?;
        let policy_definition = self.load_policy(event.policy_id).await?;
        self.policy.check(
            EventOperation::Replay,
            Some(&event),
            None,
            policy_definition.as_ref(),
            actor,
        )?;
        if matches!(replay.state, ReplayState::Completed | ReplayState::Failed) {
            return Ok(replay);
        }
        if replay.state == ReplayState::Requested || replay.deliveries.is_empty() {
            let routed = self
                .load_planned_replay_routes(
                    &event,
                    &replay.subscription_ids,
                    policy_definition.as_ref(),
                    actor,
                )
                .await?;
            replay.deliveries = routed
                .iter()
                .map(|subscription| EventDelivery::new(subscription.id, Some(replay.id)))
                .collect();
            if replay.state == ReplayState::Requested {
                let expected = transition_replay(&mut replay, ReplayState::Running)?;
                self.store
                    .save_replay(&replay, Some(expected), &[], actor)
                    .await?;
            } else {
                self.touch_and_save_replay(&mut replay, actor, &[]).await?;
            }
        }
        let subscription_ids = replay
            .deliveries
            .iter()
            .map(|delivery| delivery.subscription_id)
            .collect::<Vec<_>>();
        for subscription_id in subscription_ids {
            let mut delivery = replay
                .deliveries
                .iter()
                .find(|delivery| delivery.subscription_id == subscription_id)
                .cloned()
                .ok_or_else(|| {
                    EventError::Internal("planned replay delivery disappeared".into())
                })?;
            if matches!(
                delivery.state,
                DeliveryState::Handled | DeliveryState::DeadLettered
            ) {
                continue;
            }
            let subscription = self
                .store
                .find_subscription(subscription_id)
                .await?
                .ok_or_else(|| EventError::NotFound(subscription_id.to_string()))?;
            self.policy.check(
                EventOperation::Deliver,
                Some(&event),
                Some(&subscription),
                policy_definition.as_ref(),
                actor,
            )?;
            let max_attempts = policy_definition
                .as_ref()
                .map(|value| value.max_attempts)
                .unwrap_or(3)
                .min(subscription.max_attempts)
                .max(1);
            loop {
                if delivery.state == DeliveryState::Failed && delivery.attempts >= max_attempts {
                    let message = delivery
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "event replay exhausted its retry budget".into());
                    self.dead_letter_replay_delivery(
                        &event,
                        &mut replay,
                        &mut delivery,
                        actor,
                        message,
                    )
                    .await?;
                    break;
                }
                if matches!(
                    delivery.state,
                    DeliveryState::Pending | DeliveryState::Failed
                ) {
                    mark_delivery(&mut delivery, DeliveryState::Delivered, None)?;
                    upsert_replay_delivery(&mut replay, delivery.clone());
                    self.touch_and_save_replay(&mut replay, actor, &[]).await?;
                }
                let context = EventDeliveryContext {
                    event_id: event.id,
                    subscription_id,
                    delivery_id: delivery.id,
                    replay_id: Some(replay.id),
                    attempt: delivery.attempts,
                    actor: actor.into(),
                };
                let result = match self.bus.handler(subscription_id)? {
                    Some(handler) => self.dispatcher.dispatch(handler, &event, &context).await,
                    None => Err(EventError::Handler(
                        "event subscription has no live handler".into(),
                    )),
                };
                match result {
                    Ok(()) => {
                        mark_delivery(&mut delivery, DeliveryState::Handled, None)?;
                        upsert_replay_delivery(&mut replay, delivery.clone());
                        self.touch_and_save_replay(&mut replay, actor, &[]).await?;
                        break;
                    }
                    Err(error) if delivery.attempts < max_attempts => {
                        mark_delivery(
                            &mut delivery,
                            DeliveryState::Failed,
                            Some(bounded_error(&error)),
                        )?;
                        upsert_replay_delivery(&mut replay, delivery.clone());
                        self.touch_and_save_replay(&mut replay, actor, &[]).await?;
                    }
                    Err(error) => {
                        let message = bounded_error(&error);
                        self.dead_letter_replay_delivery(
                            &event,
                            &mut replay,
                            &mut delivery,
                            actor,
                            message,
                        )
                        .await?;
                        break;
                    }
                }
            }
        }
        let failed = replay
            .deliveries
            .iter()
            .any(|delivery| delivery.state == DeliveryState::DeadLettered);
        let next = if failed {
            ReplayState::Failed
        } else {
            ReplayState::Completed
        };
        let expected = transition_replay(&mut replay, next)?;
        self.store
            .save_replay(&replay, Some(expected), &[], actor)
            .await?;
        self.notify(
            EventOperation::Replay,
            EventStage::Replay,
            !failed,
            Some(event.id),
            None,
            Some(replay.id),
            &event.namespace,
            actor,
            if failed {
                "event replay completed with dead letters"
            } else {
                "event replay completed"
            },
        );
        Ok(replay)
    }

    async fn load_planned_replay_routes(
        &self,
        event: &EventEnvelope,
        subscription_ids: &std::collections::BTreeSet<Uuid>,
        policy_definition: Option<&EventPolicyDefinition>,
        actor: &str,
    ) -> EventResult<Vec<EventSubscription>> {
        let mut routed = Vec::with_capacity(subscription_ids.len());
        for subscription_id in subscription_ids {
            let subscription = self
                .store
                .find_subscription(*subscription_id)
                .await?
                .ok_or_else(|| EventError::NotFound(subscription_id.to_string()))?;
            subscription.validate()?;
            if subscription.namespace != event.namespace {
                return Err(EventError::Validation(
                    "planned replay subscription changed namespace".into(),
                ));
            }
            self.policy.check(
                EventOperation::Deliver,
                Some(event),
                Some(&subscription),
                policy_definition,
                actor,
            )?;
            routed.push(subscription);
        }
        routed.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.key.cmp(&right.key))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(routed)
    }

    async fn dead_letter_replay_delivery(
        &self,
        event: &EventEnvelope,
        replay: &mut EventReplayRecord,
        delivery: &mut EventDelivery,
        actor: &str,
        message: String,
    ) -> EventResult<()> {
        mark_delivery(delivery, DeliveryState::DeadLettered, Some(message.clone()))?;
        let dead_letter = EventDeadLetter::new(event, delivery, message, actor)?;
        upsert_replay_delivery(replay, delivery.clone());
        self.touch_and_save_replay(replay, actor, std::slice::from_ref(&dead_letter))
            .await
    }

    pub async fn find_event(&self, id: Uuid) -> EventResult<Option<EventEnvelope>> {
        self.store.find_event(id).await
    }

    pub async fn list_events(&self, namespace: &str) -> EventResult<Vec<EventEnvelope>> {
        crate::domain::validate_key("event namespace", namespace)?;
        self.store.list_events(namespace).await
    }

    pub async fn find_subscription(&self, id: Uuid) -> EventResult<Option<EventSubscription>> {
        self.store.find_subscription(id).await
    }

    pub async fn list_subscriptions(&self, namespace: &str) -> EventResult<Vec<EventSubscription>> {
        crate::domain::validate_key("event namespace", namespace)?;
        self.store.list_subscriptions(namespace).await
    }

    pub async fn find_replay(&self, id: Uuid) -> EventResult<Option<EventReplayRecord>> {
        self.store.find_replay(id).await
    }

    pub async fn list_replays(&self, event_id: Uuid) -> EventResult<Vec<EventReplayRecord>> {
        self.store.list_replays(event_id).await
    }

    pub async fn list_dead_letters(&self, event_id: Uuid) -> EventResult<Vec<EventDeadLetter>> {
        self.store.list_dead_letters(event_id).await
    }

    pub async fn find_policy(&self, id: Uuid) -> EventResult<Option<EventPolicyDefinition>> {
        self.store.find_policy(id).await
    }

    pub async fn list_policies(&self) -> EventResult<Vec<EventPolicyDefinition>> {
        self.store.list_policies().await
    }

    async fn load_policy(&self, id: Option<Uuid>) -> EventResult<Option<EventPolicyDefinition>> {
        match id {
            Some(id) => Ok(Some(
                self.store
                    .find_policy(id)
                    .await?
                    .ok_or_else(|| EventError::NotFound(id.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    fn transition_event(
        &self,
        event: &mut EventEnvelope,
        next: EventState,
        actor: &str,
    ) -> EventResult<()> {
        let before = event.clone();
        self.lifecycle.transition(event, next, actor)?;
        let expected_version = before
            .version
            .checked_add(1)
            .ok_or_else(|| EventError::Validation("event version is exhausted".into()))?;
        if publish_content(&before)? != publish_content(event)?
            || before.deliveries != event.deliveries
            || event.state != next
            || event.version != expected_version
            || event.updated_at < before.updated_at
            || event.actor != actor
        {
            return Err(EventError::Validation(
                "event lifecycle changed fields outside transition ownership".into(),
            ));
        }
        event.validate()
    }

    async fn transition_and_commit(
        &self,
        event: &mut EventEnvelope,
        next: EventState,
        actor: &str,
    ) -> EventResult<()> {
        let expected = event.version;
        self.transition_event(event, next, actor)?;
        self.store
            .commit_event(&EventCommit::update(event.clone(), expected), &[], actor)
            .await
    }

    async fn touch_and_commit(
        &self,
        event: &mut EventEnvelope,
        actor: &str,
        dead_letters: &[EventDeadLetter],
    ) -> EventResult<()> {
        validate_actor(actor)?;
        let expected = event.version;
        event.version = event
            .version
            .checked_add(1)
            .ok_or_else(|| EventError::Validation("event version is exhausted".into()))?;
        event.updated_at = Utc::now().max(event.updated_at);
        event.actor = actor.into();
        event.validate()?;
        self.store
            .commit_event(
                &EventCommit::update(event.clone(), expected),
                dead_letters,
                actor,
            )
            .await
    }

    async fn touch_and_save_replay(
        &self,
        replay: &mut EventReplayRecord,
        actor: &str,
        dead_letters: &[EventDeadLetter],
    ) -> EventResult<()> {
        let expected = replay.version;
        replay.version = replay
            .version
            .checked_add(1)
            .ok_or_else(|| EventError::Validation("event replay version is exhausted".into()))?;
        replay.updated_at = Utc::now().max(replay.updated_at);
        replay.validate()?;
        self.store
            .save_replay(replay, Some(expected), dead_letters, actor)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        operation: EventOperation,
        stage: EventStage,
        success: bool,
        event_id: Option<Uuid>,
        subscription_id: Option<Uuid>,
        replay_id: Option<Uuid>,
        namespace: &str,
        actor: &str,
        reason: &str,
    ) {
        let value = EventObservation {
            operation,
            stage,
            success,
            event_id,
            subscription_id,
            replay_id,
            namespace: namespace.into(),
            actor: actor.into(),
            reason: reason.into(),
            occurred_at: Utc::now(),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&value)));
        }
    }

    fn notify_delivery(
        &self,
        event: &EventEnvelope,
        delivery: &EventDelivery,
        success: bool,
        reason: &str,
    ) {
        self.notify(
            if delivery.state == DeliveryState::DeadLettered {
                EventOperation::DeadLetter
            } else {
                EventOperation::Deliver
            },
            if delivery.state == DeliveryState::DeadLettered {
                EventStage::DeadLetter
            } else {
                EventStage::Delivery
            },
            success,
            Some(event.id),
            Some(delivery.subscription_id),
            delivery.replay_id,
            &event.namespace,
            &event.actor,
            reason,
        );
    }
}

#[async_trait]
impl EventReplay for EventManager {
    async fn replay(&self, request: ReplayRequest) -> EventResult<EventReplayRecord> {
        EventManager::replay(self, request).await
    }
}

#[derive(PartialEq, Eq)]
struct PublishIdentity {
    id: Uuid,
    event_type: String,
    category: crate::domain::EventCategory,
    namespace: String,
    source_kind: crate::domain::EventSourceKind,
    source_id: Option<Uuid>,
    target: Option<String>,
    payload_type: String,
    schema_version: u32,
    actor: String,
}

fn publish_identity(event: &EventEnvelope) -> PublishIdentity {
    PublishIdentity {
        id: event.id,
        event_type: event.event_type.clone(),
        category: event.category,
        namespace: event.namespace.clone(),
        source_kind: event.source.kind,
        source_id: event.source.id,
        target: event.target.clone(),
        payload_type: event.payload_type.clone(),
        schema_version: event.schema_version,
        actor: event.actor.clone(),
    }
}

fn ensure_same_event(incoming: &EventEnvelope, stored: &EventEnvelope) -> EventResult<()> {
    if publish_content(incoming)? != publish_content(stored)? {
        return Err(EventError::Conflict(
            "event id already belongs to different immutable content".into(),
        ));
    }
    Ok(())
}

fn publish_content(event: &EventEnvelope) -> EventResult<Vec<u8>> {
    Ok(serde_json::to_vec(&(
        event.id,
        &event.event_type,
        event.category,
        &event.namespace,
        &event.source,
        &event.target,
        &event.payload,
        &event.payload_type,
        &event.metadata,
        event.priority,
        event.visibility,
        event.sensitive,
        event.schema_version,
        event.policy_id,
        event.occurred_at,
        event.created_at,
    ))?)
}

fn validate_routes(
    event: &EventEnvelope,
    routed: &[EventSubscription],
    originals: &HashMap<Uuid, EventSubscription>,
) -> EventResult<()> {
    let mut seen = HashSet::new();
    for subscription in routed {
        subscription.validate()?;
        if !seen.insert(subscription.id)
            || originals.get(&subscription.id) != Some(subscription)
            || !subscription.matches(event)
        {
            return Err(EventError::Validation(
                "event router or interceptor returned an invalid subscription".into(),
            ));
        }
    }
    Ok(())
}

fn upsert_event_delivery(event: &mut EventEnvelope, delivery: EventDelivery) {
    if let Some(value) = event
        .deliveries
        .iter_mut()
        .find(|value| value.subscription_id == delivery.subscription_id)
    {
        *value = delivery;
    } else {
        event.deliveries.push(delivery);
    }
}

fn upsert_replay_delivery(replay: &mut EventReplayRecord, delivery: EventDelivery) {
    if let Some(value) = replay
        .deliveries
        .iter_mut()
        .find(|value| value.subscription_id == delivery.subscription_id)
    {
        *value = delivery;
    } else {
        replay.deliveries.push(delivery);
    }
}

fn bounded_error(error: &EventError) -> String {
    let value = error.to_string();
    if value.len() <= 4096 {
        return value;
    }
    let mut boundary = 4096;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].into()
}
