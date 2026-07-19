use std::sync::Arc;

use core_agent::integrations::MemoryContextProvider;
use core_agent::{
    ContextPipeline, ContextSource, DefaultComposer, MemoryContent, MemoryEvent, MemoryEventKind,
    MemoryManager, MemorySourceKind, ProviderContext, SqliteSessionStore,
};
use uuid::Uuid;

#[tokio::test]
async fn structured_memory_recall_populates_the_context_memory_slot() {
    let memories = Arc::new(MemoryManager::builder().build());
    let mut event = MemoryEvent::new(
        "agent/general",
        MemorySourceKind::Execution,
        MemoryContent::new(
            "Windows terminal timeout",
            "Use a bounded command timeout and preserve the failure kind",
        ),
    );
    event.kind = MemoryEventKind::Outcome;
    event.tags.insert("windows".into());
    event.actor = "agent-runtime".into();
    let memory = memories.remember(event).await.unwrap().memory.unwrap();

    let session_id = Uuid::new_v4();
    let session_store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
    let provider_context = ProviderContext::new(session_id, session_store);
    let pipeline = ContextPipeline::builder()
        .add_provider(
            MemoryContextProvider::new(Arc::clone(&memories), "agent/general")
                .with_query("timeout")
                .with_limit(5),
        )
        .add_composer(DefaultComposer::new())
        .build();
    let context = pipeline
        .execute(session_id, None, &provider_context)
        .await
        .unwrap();

    assert!(context.memory.enabled);
    assert!(context
        .segments
        .iter()
        .any(|segment| segment.source == ContextSource::Memory));
    assert!(context
        .memory
        .content
        .to_string()
        .contains("terminal timeout"));
    assert!(context.token_distribution.memory > 0);
    assert_eq!(
        memories
            .find(memory.id)
            .await
            .unwrap()
            .unwrap()
            .recall_count,
        1
    );
}
