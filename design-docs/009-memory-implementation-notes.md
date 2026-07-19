# P8 Memory Runtime — Implementation Notes

## Delivered

- Added the independent `core-agent-memory` crate and explicit root composition exports.
- Implemented typed Memory Events, cognitive kind and domain type classification, importance/tags, namespace isolation, event-id idempotency and explainable structured recall.
- Implemented `Created -> Verified -> Indexed -> Recalled -> Updated -> Archived -> Forgotten`, CAS updates, bounded retention, policy checks and content-free Forget tombstones.
- Added injectable Store, Classifier, Indexer, Retriever, Lifecycle, Policy, Interceptor and Observer contracts. Interceptors and Lifecycle implementations cannot redirect identity, ownership or content outside their declared responsibility; observer panics are isolated.
- Added integrity-hashed, current-version snapshots. Restore creates a new version and never replays side effects; Forget atomically purges indexes, tags and snapshots.
- Added in-memory storage and five strict SQLite tables: `memory`, `memory_index`, `memory_snapshot`, `memory_policy` and `memory_tag`. Every table has the required audit columns, comments, indexes and no foreign keys.
- SQLite aggregate, index, snapshot and policy cold reads cross-check structured columns against serialized content. Snapshot checks/writes and Forget cleanup are transactional; custom Indexer output remains strictly verifiable.
- Added the root `MemoryContextProvider`, which recalls a bounded namespace into the existing Context Memory slot without creating a dependency cycle.

## Minimal usage

```rust
let memories = MemoryManager::builder().build();
let mut event = MemoryEvent::new(
    "agent/general",
    MemorySourceKind::Execution,
    MemoryContent::new("Timeout recovery", "Use a bounded command timeout"),
);
event.kind = MemoryEventKind::Outcome;
event.actor = "agent-runtime".into();

let memory = memories.remember(event).await?.memory.unwrap();
let mut query = MemoryQuery::new("agent/general");
query.text = Some("timeout".into());
let hits = memories.recall(query).await?;
assert_eq!(hits[0].memory.id, memory.id);
```

## Material discoveries and resolutions

- Context already had a Memory slot but no concrete provider. The adapter lives in the root composition crate so Memory and Context remain independently reusable.
- At-least-once event delivery requires durable idempotency. `event_id` is unique and concurrent duplicate delivery reuses the committed Memory.
- A soft-deleted row would leave private content in index/tag/snapshot tables. Forget now writes a tombstone and purges all searchable/history content in one transaction.
- An injectable Lifecycle could otherwise mutate Memory ownership during creation before Store CAS had an earlier row to compare. Manager-owned transition validation now restricts Lifecycle mutations to state/version/audit/recall fields.
- Recomputing every cold-read index with the default Indexer silently broke custom Indexer injection. SQLite now stores the complete `MemoryIndexEntry` and cross-checks it against all structured columns and the Memory aggregate.
- Delayed temporary events can already be expired when accepted. They remain durable audit evidence but are excluded from Recall.

## Deliberately deferred

- Embeddings, vector databases, semantic/hybrid retrieval, graph memory, reflection, compression and AI-generated summaries.
- Automatic producer listeners, cross-namespace sharing, Permission/RBAC bindings and Memory UI.
- Scheduled physical deletion of expired tombstones/rows; Recall already excludes expired Memories.
- Async isolation for synchronous SQLite access, consistent with the current Runtime persistence convention.

