use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use core_agent_event::{
    DeadLetterQueue, DeliveryState, EventCategory, EventCommit, EventDefinition, EventDelivery,
    EventDeliveryContext, EventEnvelope, EventError, EventHandler, EventInterceptor, EventManager,
    EventObservation, EventObserver, EventPolicyDefinition, EventReplayRecord, EventResult,
    EventRouter, EventSourceKind, EventState, EventStore, EventSubscription, EventVisibility,
    InMemoryEventStore, ReplayRequest, ReplayState, SqliteEventStore, TypedEventPayload,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BuildSucceeded {
    build_id: String,
}

impl TypedEventPayload for BuildSucceeded {
    const EVENT_TYPE: &'static str = "domain.build.succeeded";
    const CATEGORY: EventCategory = EventCategory::Domain;
}

fn typed_event(namespace: &str) -> EventEnvelope {
    EventEnvelope::from_typed(
        namespace,
        EventSourceKind::Execution,
        BuildSucceeded {
            build_id: "build-1".into(),
        },
        "publisher",
    )
    .unwrap()
}

fn manager() -> EventManager {
    let value = EventManager::builder().build();
    value
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    value
}

#[derive(Default)]
struct RecordingHandler {
    values: Mutex<Vec<EventDeliveryContext>>,
}

#[async_trait]
impl EventHandler for RecordingHandler {
    async fn handle(
        &self,
        event: &EventEnvelope,
        context: &EventDeliveryContext,
    ) -> EventResult<()> {
        assert_eq!(event.decode::<BuildSucceeded>()?.build_id, "build-1");
        self.values.lock().unwrap().push(context.clone());
        Ok(())
    }
}

struct FlakyHandler {
    failures: usize,
    calls: AtomicUsize,
}

impl FlakyHandler {
    fn new(failures: usize) -> Self {
        Self {
            failures,
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl EventHandler for FlakyHandler {
    async fn handle(
        &self,
        _event: &EventEnvelope,
        _context: &EventDeliveryContext,
    ) -> EventResult<()> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call <= self.failures {
            Err(EventError::Handler(format!("failure {call}")))
        } else {
            Ok(())
        }
    }
}

struct AlwaysFail;

#[async_trait]
impl EventHandler for AlwaysFail {
    async fn handle(
        &self,
        _event: &EventEnvelope,
        _context: &EventDeliveryContext,
    ) -> EventResult<()> {
        Err(EventError::Handler("permanent failure".into()))
    }
}

#[tokio::test]
async fn publish_routes_by_priority_and_is_idempotent() {
    let manager = manager();
    let order = Arc::new(Mutex::new(Vec::new()));

    struct OrderedHandler(&'static str, Arc<Mutex<Vec<&'static str>>>);
    #[async_trait]
    impl EventHandler for OrderedHandler {
        async fn handle(
            &self,
            _event: &EventEnvelope,
            _context: &EventDeliveryContext,
        ) -> EventResult<()> {
            self.1.lock().unwrap().push(self.0);
            Ok(())
        }
    }

    let mut low =
        EventSubscription::for_type("low-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    low.priority = 1;
    manager
        .subscribe(low, Arc::new(OrderedHandler("low", order.clone())))
        .await
        .unwrap();
    let mut high =
        EventSubscription::for_type("high-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    high.priority = 10;
    manager
        .subscribe(high, Arc::new(OrderedHandler("high", order.clone())))
        .await
        .unwrap();

    let event = typed_event("tenant-a");
    let duplicate = event.clone();
    let outcome = manager.publish(event).await.unwrap();
    assert_eq!(outcome.event.state, EventState::Archived);
    assert_eq!(outcome.handled, 2);
    assert_eq!(*order.lock().unwrap(), vec!["high", "low"]);

    let duplicate = manager.publish(duplicate).await.unwrap();
    assert!(duplicate.idempotent);
    assert_eq!(*order.lock().unwrap(), vec!["high", "low"]);

    let mut conflicting = typed_event("tenant-a");
    conflicting.id = duplicate.event.id;
    conflicting.created_at = duplicate.event.created_at;
    conflicting.occurred_at = duplicate.event.occurred_at;
    conflicting.updated_at = duplicate.event.created_at;
    conflicting.payload["build_id"] = serde_json::Value::String("different-build".into());
    assert!(matches!(
        manager.publish(conflicting).await,
        Err(EventError::Conflict(_))
    ));
}

#[tokio::test]
async fn delivered_attempt_resumes_with_stable_identity() {
    let store = Arc::new(InMemoryEventStore::default());
    let manager = EventManager::builder().store(store.clone()).build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    let handler = Arc::new(RecordingHandler::default());
    let subscription = manager
        .subscribe(
            EventSubscription::for_type(
                "recovering-handler",
                "tenant-a",
                BuildSucceeded::EVENT_TYPE,
            ),
            handler.clone(),
        )
        .await
        .unwrap();

    let mut event = typed_event("tenant-a");
    event.state = EventState::Published;
    event.version = 2;
    event.updated_at = Utc::now().max(event.updated_at);
    store
        .commit_event(&EventCommit::create(event.clone()), &[], "publisher")
        .await
        .unwrap();
    let mut delivery = EventDelivery::new(subscription.id, None);
    delivery.state = DeliveryState::Delivered;
    delivery.attempts = 1;
    delivery.delivered_at = Some(Utc::now());
    delivery.updated_at = Utc::now();
    let delivery_id = delivery.id;
    event.state = EventState::Delivered;
    event.deliveries.push(delivery);
    event.version = 3;
    event.updated_at = Utc::now().max(event.updated_at);
    store
        .commit_event(&EventCommit::update(event.clone(), 2), &[], "publisher")
        .await
        .unwrap();

    let outcome = manager.resume(event.id, "recovery-worker").await.unwrap();
    let calls = handler.values.lock().unwrap();
    assert_eq!(outcome.event.state, EventState::Archived);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].delivery_id, delivery_id);
    assert_eq!(calls[0].attempt, 1);
    assert_eq!(calls[0].actor, "recovery-worker");
}

#[tokio::test]
async fn delivered_replay_attempt_resumes_with_stable_identity() {
    let store = Arc::new(InMemoryEventStore::default());
    let manager = EventManager::builder().store(store.clone()).build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    let event = manager
        .publish(typed_event("tenant-a"))
        .await
        .unwrap()
        .event;
    let handler = Arc::new(RecordingHandler::default());
    let subscription = manager
        .subscribe(
            EventSubscription::for_type(
                "recovering-replay-handler",
                "tenant-a",
                BuildSucceeded::EVENT_TYPE,
            ),
            handler.clone(),
        )
        .await
        .unwrap();
    let mut request = ReplayRequest::new(event.id, "operator");
    request.subscription_ids.insert(subscription.id);
    let mut replay = EventReplayRecord::new(event.id, &request).unwrap();
    store
        .save_replay(&replay, None, &[], "operator")
        .await
        .unwrap();
    let mut delivery = EventDelivery::new(subscription.id, Some(replay.id));
    delivery.state = DeliveryState::Delivered;
    delivery.attempts = 1;
    delivery.delivered_at = Some(Utc::now());
    delivery.updated_at = Utc::now();
    let delivery_id = delivery.id;
    replay.state = ReplayState::Running;
    replay.deliveries.push(delivery);
    replay.version = 2;
    replay.updated_at = Utc::now().max(replay.updated_at);
    store
        .save_replay(&replay, Some(1), &[], "operator")
        .await
        .unwrap();

    let resumed = manager
        .resume_replay(replay.id, "recovery-worker")
        .await
        .unwrap();
    let calls = handler.values.lock().unwrap();
    assert_eq!(resumed.state, ReplayState::Completed);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].delivery_id, delivery_id);
    assert_eq!(calls[0].attempt, 1);
    assert_eq!(calls[0].actor, "recovery-worker");
}

struct RejectingRouter;

impl EventRouter for RejectingRouter {
    fn route(
        &self,
        _event: &EventEnvelope,
        _subscriptions: Vec<EventSubscription>,
    ) -> EventResult<Vec<EventSubscription>> {
        Err(EventError::Extension("router unavailable".into()))
    }
}

#[tokio::test]
async fn routing_failure_happens_before_event_persistence() {
    let manager = EventManager::builder()
        .router(Arc::new(RejectingRouter))
        .build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    assert!(matches!(
        manager.publish(typed_event("tenant-a")).await,
        Err(EventError::Extension(_))
    ));
    assert!(manager.list_events("tenant-a").await.unwrap().is_empty());
}

#[tokio::test]
async fn bounded_retry_reuses_delivery_identity_then_handles() {
    let manager = manager();
    let handler = Arc::new(FlakyHandler::new(2));
    let subscription =
        EventSubscription::for_type("flaky-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    manager
        .subscribe(subscription, handler.clone())
        .await
        .unwrap();
    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    assert_eq!(handler.calls.load(Ordering::SeqCst), 3);
    assert_eq!(outcome.handled, 1);
    assert!(outcome.dead_letters.is_empty());
    assert_eq!(outcome.event.deliveries[0].attempts, 3);
    assert_eq!(outcome.event.deliveries[0].state.as_str(), "HANDLED");
}

#[tokio::test]
async fn exhausted_failure_dead_letters_without_blocking_other_subscribers() {
    let manager = manager();
    let mut failing =
        EventSubscription::for_type("failing-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    failing.max_attempts = 2;
    manager
        .subscribe(failing, Arc::new(AlwaysFail))
        .await
        .unwrap();
    let success = Arc::new(RecordingHandler::default());
    let subscription =
        EventSubscription::for_type("success-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    manager
        .subscribe(subscription, success.clone())
        .await
        .unwrap();

    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    assert_eq!(outcome.handled, 1);
    assert_eq!(outcome.dead_letters.len(), 1);
    assert_eq!(success.values.lock().unwrap().len(), 1);
    assert_eq!(
        manager
            .list_dead_letters(outcome.event.id)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn replay_is_explicit_and_does_not_mutate_archived_event() {
    let manager = manager();
    let handler = Arc::new(FlakyHandler::new(1));
    let mut subscription =
        EventSubscription::for_type("replay-handler", "tenant-a", BuildSucceeded::EVENT_TYPE);
    subscription.max_attempts = 1;
    manager
        .subscribe(subscription.clone(), handler.clone())
        .await
        .unwrap();
    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    assert_eq!(outcome.dead_letters.len(), 1);
    let original_version = outcome.event.version;

    let mut request = ReplayRequest::new(outcome.event.id, "operator");
    request.subscription_ids.insert(subscription.id);
    request.reason = "verify recovered consumer".into();
    let replay = manager.replay(request).await.unwrap();
    assert_eq!(replay.state, ReplayState::Completed);
    assert_eq!(replay.deliveries[0].replay_id, Some(replay.id));
    assert_eq!(handler.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        manager
            .find_event(outcome.event.id)
            .await
            .unwrap()
            .unwrap()
            .version,
        original_version
    );
}

#[tokio::test]
async fn unsubscribe_disables_routing_but_preserves_declaration() {
    let manager = manager();
    let handler = Arc::new(RecordingHandler::default());
    let subscription = manager
        .subscribe(
            EventSubscription::for_type(
                "temporary-handler",
                "tenant-a",
                BuildSucceeded::EVENT_TYPE,
            ),
            handler.clone(),
        )
        .await
        .unwrap();
    let disabled = manager
        .unsubscribe(subscription.id, "operator")
        .await
        .unwrap();
    assert!(!disabled.enabled);
    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    assert_eq!(outcome.handled, 0);
    assert!(handler.values.lock().unwrap().is_empty());
    assert_eq!(
        manager.list_subscriptions("tenant-a").await.unwrap().len(),
        1
    );
}

#[tokio::test]
async fn policy_denies_sensitive_external_publish_and_replay() {
    let manager = manager();
    let mut sensitive = typed_event("tenant-a");
    sensitive.sensitive = true;
    sensitive.visibility = EventVisibility::External;
    assert!(matches!(
        manager.publish(sensitive).await,
        Err(EventError::PolicyDenied(_))
    ));

    let mut policy = EventPolicyDefinition::new("no-replay", "No Replay");
    policy.allow_replay = false;
    let policy = manager.register_policy(policy, "admin").await.unwrap();
    let mut event = typed_event("tenant-a");
    event.policy_id = Some(policy.id);
    let outcome = manager.publish(event).await.unwrap();
    assert!(matches!(
        manager
            .replay(ReplayRequest::new(outcome.event.id, "operator"))
            .await,
        Err(EventError::PolicyDenied(_))
    ));
}

struct RedirectingInterceptor;

impl EventInterceptor for RedirectingInterceptor {
    fn before_publish(&self, event: &mut EventEnvelope) -> EventResult<()> {
        event.namespace = "tenant-b".into();
        Ok(())
    }
}

struct PanickingObserver;

impl EventObserver for PanickingObserver {
    fn on_observation(&self, _observation: &EventObservation) {
        panic!("observer failure")
    }
}

#[tokio::test]
async fn extensions_cannot_redirect_scope_and_observer_panics_are_isolated() {
    let manager = EventManager::builder()
        .interceptor(Arc::new(RedirectingInterceptor))
        .build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    assert!(matches!(
        manager.publish(typed_event("tenant-a")).await,
        Err(EventError::Validation(_))
    ));

    let manager = EventManager::builder()
        .observer(Arc::new(PanickingObserver))
        .build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    assert_eq!(
        manager
            .publish(typed_event("tenant-a"))
            .await
            .unwrap()
            .event
            .state,
        EventState::Archived
    );
}

#[tokio::test]
async fn stale_store_commit_conflicts_without_losing_winner() {
    let store = Arc::new(InMemoryEventStore::default());
    let manager = EventManager::builder().store(store.clone()).build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    let expected = outcome.event.version;
    let mut winner = outcome.event.clone();
    winner.version += 1;
    winner.updated_at = Utc::now().max(winner.updated_at);
    let stale = winner.clone();
    store
        .commit_event(&EventCommit::update(winner, expected), &[], "publisher")
        .await
        .unwrap();
    assert!(matches!(
        store
            .commit_event(&EventCommit::update(stale, expected), &[], "publisher")
            .await,
        Err(EventError::Conflict(_))
    ));
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("event.db");
    let store = Arc::new(SqliteEventStore::new(&path).unwrap());
    let manager = EventManager::builder().store(store.clone()).build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    let handler = Arc::new(RecordingHandler::default());
    let subscription = manager
        .subscribe(
            EventSubscription::for_type("sqlite-handler", "tenant-a", BuildSucceeded::EVENT_TYPE),
            handler,
        )
        .await
        .unwrap();
    let event = manager
        .publish(typed_event("tenant-a"))
        .await
        .unwrap()
        .event;
    drop(manager);
    drop(store);

    let reopened = Arc::new(SqliteEventStore::new(&path).unwrap());
    assert_eq!(
        reopened.find_event(event.id).await.unwrap().unwrap().state,
        EventState::Archived
    );
    assert!(
        reopened
            .find_subscription(subscription.id)
            .await
            .unwrap()
            .unwrap()
            .enabled
    );

    let connection = Connection::open(&path).unwrap();
    for table in [
        "event",
        "event_subscription",
        "event_replay",
        "event_policy",
        "event_dead_letter",
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
            assert!(columns.contains(required), "{table} is missing {required}");
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
            "UPDATE event SET state = 'HANDLED' WHERE id = ?1",
            [event.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        reopened.find_event(event.id).await,
        Err(EventError::Validation(_))
    ));
}

#[tokio::test]
async fn sqlite_dead_letter_and_replay_survive_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("replay.db");
    let store = Arc::new(SqliteEventStore::new(&path).unwrap());
    let manager = EventManager::builder().store(store.clone()).build();
    manager
        .register_type(EventDefinition::for_payload::<BuildSucceeded>(
            "build completed",
        ))
        .unwrap();
    let handler = Arc::new(FlakyHandler::new(1));
    let mut subscription = EventSubscription::for_type(
        "durable-replay-handler",
        "tenant-a",
        BuildSucceeded::EVENT_TYPE,
    );
    subscription.max_attempts = 1;
    manager
        .subscribe(subscription.clone(), handler)
        .await
        .unwrap();
    let outcome = manager.publish(typed_event("tenant-a")).await.unwrap();
    let mut request = ReplayRequest::new(outcome.event.id, "operator");
    request.subscription_ids.insert(subscription.id);
    let replay = manager.replay(request).await.unwrap();
    drop(manager);
    drop(store);

    let reopened = SqliteEventStore::new(&path).unwrap();
    assert_eq!(
        reopened
            .find_replay(replay.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ReplayState::Completed
    );
    assert_eq!(
        reopened
            .list_dead_letters(outcome.event.id)
            .await
            .unwrap()
            .len(),
        1
    );
}
