use std::sync::Arc;

use core_agent::integrations::{MemoryRememberEventHandler, MemoryRememberPayload};
use core_agent::{
    EventDefinition, EventEnvelope, EventManager, EventSourceKind, EventState, EventSubscription,
    MemoryContent, MemoryEvent, MemoryEventKind, MemoryManager, MemoryQuery, MemorySourceKind,
    TypedEventPayload,
};

#[tokio::test]
async fn typed_event_delivers_to_memory_without_runtime_dependency_cycle() {
    let memories = Arc::new(MemoryManager::builder().build());
    let events = EventManager::builder().build();
    events
        .register_type(EventDefinition::for_payload::<MemoryRememberPayload>(
            "remember a useful typed Memory Event",
        ))
        .unwrap();
    events
        .subscribe(
            EventSubscription::for_type(
                "memory-runtime",
                "agent/general",
                MemoryRememberPayload::EVENT_TYPE,
            ),
            Arc::new(MemoryRememberEventHandler::new(memories.clone())),
        )
        .await
        .unwrap();

    let mut memory_event = MemoryEvent::new(
        "agent/general",
        MemorySourceKind::Execution,
        MemoryContent::new(
            "Event delivery recovery",
            "Use stable delivery identities and idempotent handlers",
        ),
    );
    memory_event.kind = MemoryEventKind::Outcome;
    memory_event.actor = "execution-runtime".into();
    let envelope = EventEnvelope::from_typed(
        "agent/general",
        EventSourceKind::Execution,
        MemoryRememberPayload {
            event: memory_event,
        },
        "event-runtime",
    )
    .unwrap();
    let duplicate = envelope.clone();

    let outcome = events.publish(envelope).await.unwrap();
    assert_eq!(outcome.event.state, EventState::Archived);
    assert_eq!(outcome.handled, 1);
    assert!(events.publish(duplicate).await.unwrap().idempotent);

    let mut query = MemoryQuery::new("agent/general");
    query.text = Some("delivery".into());
    let hits = memories.recall(query).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.actor, "system");
    assert!(hits[0].memory.content.title.contains("Event delivery"));
}
