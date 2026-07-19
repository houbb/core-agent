use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    EventDeadLetter, EventDefinition, EventEnvelope, EventPolicyDefinition, EventReplayRecord,
    EventState, EventSubscription, ReplayRequest,
};
use crate::error::{EventError, EventResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventOperation {
    Register,
    Subscribe,
    Unsubscribe,
    Publish,
    Deliver,
    Replay,
    DeadLetter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStage {
    Registry,
    Policy,
    Routing,
    Persistence,
    Delivery,
    Retry,
    Replay,
    DeadLetter,
}

#[derive(Debug, Clone)]
pub struct EventObservation {
    pub operation: EventOperation,
    pub stage: EventStage,
    pub success: bool,
    pub event_id: Option<Uuid>,
    pub subscription_id: Option<Uuid>,
    pub replay_id: Option<Uuid>,
    pub namespace: String,
    pub actor: String,
    pub reason: String,
    pub occurred_at: DateTime<Utc>,
}

pub trait EventObserver: Send + Sync {
    fn on_observation(&self, observation: &EventObservation);
}

pub trait EventInterceptor: Send + Sync {
    fn before_publish(&self, _event: &mut EventEnvelope) -> EventResult<()> {
        Ok(())
    }

    fn after_route(
        &self,
        _event: &EventEnvelope,
        _subscriptions: &mut Vec<EventSubscription>,
    ) -> EventResult<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EventDeliveryContext {
    pub event_id: Uuid,
    pub subscription_id: Uuid,
    pub delivery_id: Uuid,
    pub replay_id: Option<Uuid>,
    pub attempt: u32,
    pub actor: String,
}

#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(
        &self,
        event: &EventEnvelope,
        context: &EventDeliveryContext,
    ) -> EventResult<()>;
}

pub trait EventRegistry: Send + Sync {
    fn register(&self, definition: EventDefinition) -> EventResult<()>;
    fn find(&self, key: &str) -> EventResult<Option<EventDefinition>>;
    fn list(&self) -> EventResult<Vec<EventDefinition>>;
}

pub trait EventBus: Send + Sync {
    fn bind(&self, subscription_id: Uuid, handler: Arc<dyn EventHandler>) -> EventResult<()>;
    fn unbind(&self, subscription_id: Uuid) -> EventResult<()>;
    fn handler(&self, subscription_id: Uuid) -> EventResult<Option<Arc<dyn EventHandler>>>;
}

pub trait EventRouter: Send + Sync {
    fn route(
        &self,
        event: &EventEnvelope,
        subscriptions: Vec<EventSubscription>,
    ) -> EventResult<Vec<EventSubscription>>;
}

#[async_trait]
pub trait EventDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        handler: Arc<dyn EventHandler>,
        event: &EventEnvelope,
        context: &EventDeliveryContext,
    ) -> EventResult<()>;
}

pub trait EventPolicy: Send + Sync {
    fn check(
        &self,
        operation: EventOperation,
        event: Option<&EventEnvelope>,
        subscription: Option<&EventSubscription>,
        definition: Option<&EventPolicyDefinition>,
        actor: &str,
    ) -> EventResult<()>;
}

pub trait EventLifecycle: Send + Sync {
    fn transition(
        &self,
        event: &mut EventEnvelope,
        next: EventState,
        actor: &str,
    ) -> EventResult<()>;
}

#[derive(Debug, Clone)]
pub struct EventCommit {
    pub event: EventEnvelope,
    pub expected_version: Option<u64>,
}

impl EventCommit {
    pub fn create(event: EventEnvelope) -> Self {
        Self {
            event,
            expected_version: None,
        }
    }

    pub fn update(event: EventEnvelope, expected_version: u64) -> Self {
        Self {
            event,
            expected_version: Some(expected_version),
        }
    }

    pub fn validate(&self) -> EventResult<()> {
        self.event.validate()?;
        if let Some(expected) = self.expected_version {
            if self.event.version != expected.saturating_add(1) {
                return Err(EventError::Validation(
                    "event update version must advance exactly once".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait DeadLetterQueue: Send + Sync {
    async fn save_dead_letter(&self, value: &EventDeadLetter, actor: &str) -> EventResult<()>;
    async fn find_dead_letter(&self, id: Uuid) -> EventResult<Option<EventDeadLetter>>;
    async fn list_dead_letters(&self, event_id: Uuid) -> EventResult<Vec<EventDeadLetter>>;
}

#[async_trait]
pub trait EventStore: DeadLetterQueue + Send + Sync {
    async fn commit_event(
        &self,
        commit: &EventCommit,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()>;
    async fn find_event(&self, id: Uuid) -> EventResult<Option<EventEnvelope>>;
    async fn list_events(&self, namespace: &str) -> EventResult<Vec<EventEnvelope>>;

    async fn save_subscription(
        &self,
        value: &EventSubscription,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()>;
    async fn find_subscription(&self, id: Uuid) -> EventResult<Option<EventSubscription>>;
    async fn list_subscriptions(&self, namespace: &str) -> EventResult<Vec<EventSubscription>>;

    async fn save_replay(
        &self,
        value: &EventReplayRecord,
        expected_version: Option<u64>,
        dead_letters: &[EventDeadLetter],
        actor: &str,
    ) -> EventResult<()>;
    async fn find_replay(&self, id: Uuid) -> EventResult<Option<EventReplayRecord>>;
    async fn list_replays(&self, event_id: Uuid) -> EventResult<Vec<EventReplayRecord>>;

    async fn save_policy(
        &self,
        value: &EventPolicyDefinition,
        expected_version: Option<u64>,
        actor: &str,
    ) -> EventResult<()>;
    async fn find_policy(&self, id: Uuid) -> EventResult<Option<EventPolicyDefinition>>;
    async fn list_policies(&self) -> EventResult<Vec<EventPolicyDefinition>>;
}

#[async_trait]
pub trait EventReplay: Send + Sync {
    async fn replay(&self, request: ReplayRequest) -> EventResult<EventReplayRecord>;
}

pub type SharedEventStore = Arc<dyn EventStore>;
pub type SharedEventHandler = Arc<dyn EventHandler>;
