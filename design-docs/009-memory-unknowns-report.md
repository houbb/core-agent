# P8 Memory Runtime — Unknowns Report

## Metadata

- **Task / Feature:** Structured Memory Runtime
- **Mode:** Standard
- **Date:** 2026-07-18
- **Prepared by:** Codex
- **Scope:** `core-agent-memory`, root composition, Context integration, SQLite MVP

## Intent

### User-visible problem

The platform can execute long-running Agents but has no durable, inspectable way to retain useful experience or knowledge across Sessions.

### Desired behavior change

Runtime callers can submit typed Memory Events, deterministically decide whether to remember them, store and index accepted Memories, recall them through structured filters and ranking, update/archive/forget them, and snapshot/restore their content.

### Affected users and workflows

- Runtime authors emitting Memory Events after useful outcomes.
- Context builders recalling bounded Memories for a namespace.
- Operators inspecting Memory state, reason, source, tags, confidence and importance.

### Success criteria

- Memory remains independent of Tool, Planning, Execution and Agent implementations.
- Remember is idempotent by event identity and policy/classifier decisions fail closed.
- Recall is namespace-isolated, deterministic, bounded and does not use embeddings.
- Forget purges searchable content, tags and snapshots rather than only hiding a row.
- SQLite uses the five designed tables, required audit columns, comments, indexes and no foreign keys.
- Memory can feed the existing Context Memory slot through a composition adapter.

### Non-goals

- Embeddings, vector databases, semantic search, graph memory, AI summarization/reflection, compression, memory sharing and UI screens.
- Automatic edits to every producer Runtime; P8 exposes the event contract and keeps producer wiring at the composition layer.

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | Memory is an independent Runtime and precedes Permission | High |
| Documentation | `design-docs/009-memory.md` | Structured lifecycle, five SQLite tables, extension points and MVP exclusions | High |
| Code | `core-agent-context` | A Memory source/slot and provider extension already exist | High |
| Code | `core-agent-agent/src/domain.rs` | Agent Profile already has a declarative `memory_key`; Agent must not own Memory implementation | High |
| Code/Schema | existing Runtime crates | Builder injection, CAS aggregates, strict SQLite reads, audit columns and no foreign keys are project conventions | High |
| Tests | existing Runtime E2E suites | In-memory + SQLite + cross-Runtime tests are the established verification pattern | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Context already understands Memory but has no real Memory provider | `ContextSource::Memory`, `ContextSlot::Memory` | Integration can be additive |
| P8 explicitly excludes embeddings and semantic search | `009-memory.md` MVP section | Retrieval must stay structured/deterministic |
| Exactly five first-version tables are proposed | `memory`, `memory_index`, `memory_snapshot`, `memory_policy`, `memory_tag` | Schema boundary is stable |
| Every project table requires five audit fields and no foreign keys | `AGENTS.md` | Mandatory persistence invariant |
| Runtime dependencies are kept acyclic through root composition adapters | root `integrations` module | Memory-to-Context wiring belongs in root crate |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| How to prevent cross-user/workspace recall | Unknown unknown candidate | No tenant Runtime exists yet | 5 | 4 | 4 | 5 | 400 | Decision | Require a normalized namespace on every Event and Query; never search across namespaces implicitly |
| Whether repeated delivery creates duplicate Memories | Known unknown | Event-driven systems are normally at-least-once | 5 | 4 | 3 | 5 | 300 | Decision | Persist a unique event ID and return the existing Memory idempotently |
| What “forget” means for retained snapshots | Unknown known | Policy/GDPR direction conflicts with soft-delete-only behavior | 5 | 3 | 4 | 5 | 300 | Decision | Tombstone aggregate content and atomically purge index, tags and snapshots |
| Whether Recall mutates lifecycle/audit data | Known unknown | Lifecycle includes Recalled | 3 | 4 | 2 | 3 | 72 | Decision | Atomically update returned Memories to Recalled with count/time; observer records operation |
| How Episodic and Semantic relate to MemoryType | Known unknown | Design asks for both while also listing eight types | 3 | 4 | 2 | 3 | 72 | Decision | Freeze two orthogonal enums: cognitive kind and domain type |
| How Context selects Memory | Known unknown | ProviderContext has extensions but no Memory dependency | 4 | 4 | 2 | 3 | 96 | Decision | Root adapter reads namespace/query limits from configuration, keeping lower crates independent |
| Retention cleanup scheduling | Unknown unknown candidate | No scheduler/Plugin Runtime exists yet | 3 | 3 | 2 | 2 | 36 | Monitor | Compute expiry and exclude expired rows; background purge is deferred |
| Ranking expectations without semantic search | Unknown known | “Rank” is required but embeddings are forbidden | 3 | 4 | 2 | 3 | 72 | Decision | Stable score from exact text/tag matches, importance, confidence and recency, with UUID tie-break |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Recall must be explainable | Inspector and Recall UX show why items matched/used | Return score and matched fields |
| Conversation and temporary logs should not be retained by default | Explicit examples in P8 | Default classifier regression tests |
| Sensitive content must fail closed | Enterprise policy direction and existing secret validation | Policy + nested secret-key tests |
| Historical side effects must never be replayed by restore | Existing Runtime recovery convention | Restore only Memory data as a new version |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| Concurrent update/recall can lose counters | Both mutate one aggregate | CAS and batch-commit conflict tests |
| Corrupt denormalized index can change Recall results | SQLite stores aggregate and structured columns | Strict cold-read cross-check tests |
| Unicode byte limits can strand lifecycle commits | Existing Runtime discovered this failure mode | Multibyte content/error tests |
| Observer/interceptor panics can change durable behavior | Extension points are untrusted | Panic isolation tests |
| Snapshot restore could resurrect forgotten content | Privacy requirement | Explicit Forgotten restore rejection and purge test |

## Decisions Required

No user-blocking decision remains. All selected decisions are local, reversible contracts consistent with the design and current Runtime conventions.

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| Can structured Recall feed Context without a dependency cycle? | Cross-Runtime E2E | Memory segment appears in Context with bounded tokens | Low | Runtime implementation |
| Is completion deterministic after reopen? | SQLite cold-recovery test | Same ordered hits and integrity checks after reopen | Low | Runtime implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| Namespace is supplied by the host rather than inferred from Agent | Avoids hidden sharing and dependency cycles | Add an Agent-aware root adapter later |
| Default retention is 90 days; Temporary is 7 days; Critical has no automatic expiry | Declarative policy can override it | Update policy catalog without schema migration |
| Backend contracts precede Memory UI | Current workspace has no frontend host | UI consumes stable list/recall/observation APIs later |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| Tenant/RBAC mapping | Permission Runtime is the next phase | Bind namespace access to Permission Runtime |
| Background expiry purge | No scheduler Runtime exists | Add explicit maintenance operation or Plugin job |
| Embedding and hybrid rank calibration | Explicitly outside MVP | Implement behind `MemoryIndexer`/`MemoryRetriever` |

## Recommended Implementation Boundary

### Implement now

- Structured Memory/Event/Query/Hit/Snapshot/Policy domain.
- Manager, Store, Indexer, Retriever, Classifier, Lifecycle, Policy, Observer and Interceptor extensions.
- In-memory and strict SQLite implementations with exactly five tables.
- Root Context provider adapter and cross-Runtime test.

### Do not implement now

- Producer-specific automatic event listeners, vectors, AI inference, sharing, UI or scheduled cleanup.

### Interfaces or data contracts to freeze

- Namespace and event ID idempotency.
- Cognitive kind versus domain type.
- Tombstone-and-purge Forget behavior.
- Structured Query/Hit match explanations.
- Five-table schema and audit fields.

### Areas that must remain reversible

- Classifier rules, ranking weights, retention defaults and Context serialization shape.

## Verification Plan

### Automated

- Unit tests: validation, classification, lifecycle, ranking and policy.
- Integration tests: remember/recall/update/archive/forget/snapshot, CAS, observer/interceptor, SQLite reopen/tamper/schema.
- Cross-Runtime: Memory recall feeds Context Memory slot.
- Static analysis: strict Clippy and formatting.

### Manual

- Happy/empty/failure/recovery/namespace boundaries are represented by assertion tests.
- UI, responsive and accessibility checks are not applicable to this backend-only phase.

### Observability

- Observer emits Remember/Recall/Update/Archive/Forget/Snapshot/Restore outcomes with IDs, actor and reason.
- Durable audit columns preserve current-row ownership; snapshots preserve opted-in history until Forget.

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file planned
