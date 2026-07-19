# P1 Context Runtime Unknowns Report

## Metadata

- **Task / Feature:** Review and enhance `002-context.md`
- **Mode:** Standard
- **Date:** 2026-07-17
- **Prepared by:** Codex
- **Scope:** `core-agent-context` and its read-only Session Runtime integration

## Intent

### User-visible problem

The existing Context Runtime has the expected module names, but some public request options and Context Slots do not affect the generated Context. A caller cannot reliably treat the result as the exact, replayable input for a later Model Runtime.

### Desired behavior change

Build a deterministic, budget-aware, fully structured Context from validated Session data; preserve every Slot, save strict replayable snapshots, and expose the extension points required by the P1 design.

### Affected users and workflows

- Framework consumers calling `ContextRuntime::build_context`.
- Provider, Reducer, Composer, Snapshot Store, Serializer, Cache, and Observer implementers.
- Future Model Runtime, Context Inspector, replay, audit, and debugging workflows.

### Success criteria

- Request `max_messages` and `max_tokens` change the result as documented.
- The newest N conversation messages are retained in chronological order.
- Every enabled Slot survives composition and contributes to token distribution and hash.
- Required content never silently violates a token budget; an explicit error is returned.
- Snapshot round-trip, migration, pruning, and corrupt-row behavior are assertion-tested.
- Session ownership/state validation and a real Context Runtime end-to-end flow pass.

### Non-goals

- RAG, embedding, vector databases, long memory, AI summaries, automatic compression, Context Inspector UI, or implementations of later Workspace/Memory/Tool/Plugin runtimes.

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `000-roadMap.md`, `002-context.md` | P1 responsibilities, lifecycle, slots, schema, extension points, and non-goals | High |
| Code | `core-agent-context/src/application` | Request budgets are replaced by defaults; fallback and composer duplicate logic | High |
| Code | `core-agent-context/src/persistence/providers` | Conversation collection requests offset zero, therefore keeps the oldest rows | High |
| Code | `core-agent-context/src/domain` | Eight Slots exist, but Context has no Tool context and no canonical segment view | High |
| Schema | `core-agent-context/src/persistence/schema.rs` | Snapshot table lacks project-required audit fields | High |
| Tests | Inline P1 tests | Existing tests cover the skeleton but not request budgets, strict replay, migration, or full E2E behavior | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| `BuildContextRequest.max_tokens` is ignored | Pipeline constructs `ReducerConfig::default()` | Public API contract is false |
| `max_messages` selects the oldest N messages | Session Store orders ascending and Provider uses offset zero | Recent conversational context is lost |
| Workspace, Memory, Tool, and Plugin contents are discarded | Composer updates only token counters for these Slots | Context cannot be replayed or inspected |
| Hash excludes actual Context content and includes build time | Composer hash payload contains totals and `built_at` | Integrity and deduplication semantics are invalid |
| Build duration is always zero | Composer hard-codes `build_duration_ms` | Observability data is invalid |
| Snapshot row corruption is hidden in list queries | `filter_map`, default UUID/time conversion | Audit and replay can silently lie |
| Required P1 extension traits are incomplete | No Serializer, Cache, or Observer trait | Future implementations would require core modification |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| What happens when required Slots exceed the budget? | Known unknown | Required segments cannot be safely dropped | 5 | 4 | 2 | 5 | 200 | Decision | Return `TokenBudgetExceeded` with required/limit values |
| What constitutes identical Context for hashing? | Known unknown | IDs and timestamps are per-build, content is semantic | 5 | 5 | 2 | 4 | 200 | Decision | Hash canonical semantic content and token distribution; exclude ID/time/duration |
| How should old snapshots acquire audit fields? | Unknown unknown candidate | Existing P1 databases may use the current schema | 4 | 4 | 2 | 4 | 128 | Decision | Additive startup migration and backfill from `created_at` |
| Can later Slot data be represented before those runtimes exist? | Unknown known | The design requires placeholders and custom Providers | 4 | 5 | 2 | 4 | 160 | Decision | Preserve structured segment content without implementing data collection runtimes |
| Does a custom Pipeline own snapshot persistence? | Known unknown | Service may save a second time if Pipeline also has a Store | 3 | 3 | 2 | 3 | 54 | Accept | Keep service as the single default owner; document custom pipeline behavior and avoid duplicate configuration internally |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Context is the exact future model input, not a lossy summary DTO | Replay, audit, and Context Inspector are explicit goals | Round-trip and all-Slot tests |
| Same semantic inputs produce the same hash | Hash is described as integrity and deduplication data | Determinism test across two builds |
| Later runtimes remain plug-in dependencies | Roadmap requires independent runtimes | Custom Provider/Composer/Observer E2E test |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| Conversation ID belongs to another Session | Cross-session data could leak into Context | Ownership assertion test |
| Archived/deleted Session builds | Replay may be useful for archived data, but deleted data must not be exposed | Allow archived reads, reject deleted; E2E test |
| Concurrent snapshot writers | IDs are unique, but pruning/list consistency may vary | Defer until multi-writer persistence; document |
| Token estimation differs from future model tokenizers | MVP explicitly permits estimation | Keep TokenCounter replaceable and document approximation |

## Decisions Required

No external decision blocks implementation. All selected decisions are additive or locally reversible.

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| Can strict budget enforcement preserve required content? | Unit and E2E tests | Fits budget or returns explicit error; never silently exceeds | Low | Codex |
| Can existing snapshot databases migrate without data loss? | Legacy-schema migration test | Old row loads and all audit columns exist | Low | Codex |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| `max_tokens = 0` means unlimited | Existing Reducer contract already defines this | Validate zero as invalid in a later API revision if desired |
| Archived Sessions remain readable for replay; deleted Sessions are rejected | Matches Session lifecycle and audit use cases | Tighten state policy in application validation |
| Non-conversation custom Slot content is stored as ordered JSON segments | Preserves data without interpreting later-runtime schemas | Replace placeholder representation when each Runtime contract exists |
| Deterministic non-AI reduction is the P1 MVP | Explicit P1 non-goals exclude AI summary and automatic compression | Add a later compressor/reducer implementation |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| Exact per-model tokenization | Model Runtime does not exist | Implement model-specific TokenCounter later |
| Cache eviction and consistency | P1 only requires the extension point | Add with P1.6 Context Cache |
| Privacy filtering and redaction | Context Policy is P1.8 | Add policy before enterprise data use |

## Recommended Implementation Boundary

### Implement now

- Request-driven reducer config, latest-message collection, deterministic ordering, strict budgets, complete Slot composition, semantic hashing, duration measurement, Session ownership validation, strict snapshot persistence/migration, and missing extension traits.
- Unit assertions and Context Runtime end-to-end tests.

### Do not implement now

- AI-generated summaries/compression, real Workspace/Memory/Tool/Plugin data sources, cache backend, UI, RAG, or policy engine.

### Interfaces or data contracts to freeze

- `ContextProvider`, `ContextReducer`, `ContextComposer`, `ContextSnapshotStore`, plus additive `ContextSerializer`, `ContextCache`, and `ContextObserver` extension contracts.
- Snapshot content remains the complete serialized `Context`.

### Areas that must remain reversible

- Placeholder representation for future Slot contents and additive snapshot migration.

## Verification Plan

### Automated

- Unit tests: token budgets, Last-N ordering, every Slot, stable hash, observer isolation, serializer round-trip.
- Integration tests: Session → messages/providers → reducer → composer → snapshot/load/list/prune.
- Migration tests: legacy snapshot table to required audit columns.
- Contract tests: custom Provider/Observer and invalid ownership/state behavior.
- Static analysis: P1 formatting and strict Clippy.

### Manual

- Happy path, empty Context, explicit budget failure, snapshot recovery, and archived/deleted Session behavior are represented as automated E2E scenarios.
- UI, responsive behavior, and accessibility are out of scope because P1 exposes no UI.

### Observability

- Accurate build duration and Observer hooks provide the P1 foundation; production metrics/alerts belong to Observation Runtime.

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file
