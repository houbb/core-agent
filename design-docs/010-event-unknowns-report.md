# P9 Event Runtime — Unknowns Report

## Metadata

- **Task / Feature:** Local typed Event Runtime
- **Mode:** Standard
- **Date:** 2026-07-18
- **Prepared by:** Codex
- **Scope:** `core-agent-event`, root composition, Memory subscriber integration, SQLite MVP

## Intent

### User-visible problem

Existing Runtimes compose through direct calls. Adding Workflow, Plugin, Audit, Notification and Analytics on top of those calls would create bidirectional dependencies and make the Agent Kernel difficult to evolve.

### Desired behavior change

Runtime authors can register typed events, subscribe local handlers, publish through deterministic routing/policy/delivery, inspect delivery and dead-letter state, and explicitly replay an archived event without either producer or consumer depending on the other Runtime.

### Affected users and workflows

- Runtime authors publishing system or domain events.
- Local consumers subscribing by namespace/type/category/source/target.
- Operators inspecting event lifecycle, attempts, dead letters and replay outcomes.
- Composition hosts connecting existing Runtime contracts through Event handlers.

### Success criteria

- Event Runtime contains communication mechanics only and depends on no business Runtime.
- Typed payload creation, registry validation and version checks reject unknown or incompatible events before persistence.
- Publish is idempotent by event ID, namespace-isolated and records state before invoking handlers.
- Routing and handler order are deterministic; bounded retries reuse a stable delivery identity and exhausted failures enter a durable dead-letter queue.
- Replay is explicit, policy-checked and auditable; it never pretends handler side effects are transactional.
- SQLite uses the five designed tables with mandatory audit columns, comments, indexes, strict cold-read checks and no foreign keys.
- A root composition handler proves Event can feed Memory without a dependency cycle.

### Non-goals

- Kafka, RabbitMQ, Redis Streams, distributed/cluster routing, CloudEvents, CQRS, Event Sourcing or a general message queue.
- Guaranteed exactly-once side effects, delayed scheduling, cross-process live handler discovery or UI screens.
- Automatic conversion of every existing direct Runtime call during P9.

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | Runtime boundaries must stay independent and pluggable | High |
| Documentation | `design-docs/010-event.md` | Event responsibilities, lifecycle, extension points, five tables and MVP exclusions | High |
| Code | root `integrations` module | Cross-Runtime adapters belong in composition rather than lower crates | High |
| Code | `core-agent-memory` | Memory Event is serializable and can be consumed through an Event handler | High |
| Code/Schema | existing Runtime crates | Builder injection, CAS, strict SQLite reads, audit columns and no foreign keys are established conventions | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| P9 is an in-process/local Event Bus backed by SQLite | Current design explicitly excludes brokers and distributed buses | Dispatch must remain local and deterministic |
| Event Registry requires named types and schema/version information | Current design rejects string-only/map-only events | Public creation API must be generic over typed payloads |
| Replay and dead-letter concepts are required extension boundaries | Architecture and five-table schema | Durable records are needed even if scheduling/distribution are deferred |
| Memory must not depend on Tool/Execution producers | P8 boundary and P9 motivation | Event-to-Memory wiring must live in root composition |
| Every table requires `id`, four audit columns, comments, indexes and no foreign keys | `AGENTS.md` | Mandatory schema invariant |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| What delivery guarantee can a local handler safely claim? | Known unknown | Handler side effects cannot share the Event SQLite transaction | 5 | 5 | 4 | 5 | 500 | Decision | At-least-once delivery only; stable event/subscription/delivery IDs are exposed and handlers must be idempotent |
| What is persisted when handler code is process-local? | Known unknown | Function pointers cannot survive restart | 5 | 5 | 3 | 5 | 375 | Decision | Persist subscription declarations; bind handlers in a live EventBus. Missing live handlers fail visibly and dead-letter |
| How does publish survive a process crash around side effects? | Unknown unknown candidate | Persist-after-handle can lose evidence; persist-before-handle can repeat work | 5 | 4 | 4 | 5 | 400 | Decision | Persist Published/Delivered attempt before handler invocation, then record Handled/Failed afterward |
| How should one event represent multiple subscriber outcomes? | Known unknown | One lifecycle state cannot encode per-subscriber attempts | 4 | 5 | 3 | 4 | 240 | Decision | Keep aggregate lifecycle plus bounded per-subscription delivery records in the serialized Event |
| Does replay mutate the original event? | Known unknown | Rewriting an archived historical aggregate harms audit meaning | 4 | 4 | 3 | 4 | 192 | Decision | Original Event remains archived; `event_replay` stores replay state and new dead letters reference the replay ID |
| How are tenant boundaries represented before Permission Runtime exists? | Unknown unknown candidate | Event object does not specify tenancy in the design sketch | 5 | 4 | 4 | 5 | 400 | Decision | Require normalized namespace on Event and Subscription; Router never crosses namespace |
| How much of Sync/Async/Delayed/Priority belongs in MVP? | Unknown known | Design says Dispatcher supports these “later” while P9.0 is Local Bus | 3 | 4 | 2 | 2 | 48 | Decision | Async Rust API with deterministic priority-ordered sequential delivery; delayed/background scheduling is deferred |
| What makes a payload “typed” after persistence? | Known unknown | SQLite stores bytes/JSON while public API must not be a raw map | 4 | 4 | 3 | 4 | 192 | Decision | `TypedEventPayload` supplies stable type/category/version; envelope snapshots payload type and JSON, Registry validates it before publish/replay |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Handler failures must never hide other subscribers | Event Bus fan-out example lists independent consumers | E2E with one success and one exhausted failure |
| Ordering must be explainable | Priority and Timeline are explicit UX concepts | Stable priority/key/ID sort and assertion tests |
| Replay must be visibly dangerous/auditable | Replay can repeat real side effects | Explicit actor/reason, replay ID and policy gate |
| Sensitive external delivery should fail closed | Event Policy examples distinguish Sensitive and External | Default policy and nested secret-key tests |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| Handler panic could abort fan-out | Extensions are untrusted | Panic isolation E2E |
| Router/interceptor could redirect scope | Extension output crosses security boundary | Identity and route-membership validation tests |
| Duplicate publish could repeat side effects | Publishers are commonly at-least-once | Idempotent publish E2E |
| Concurrent publish/replay could lose state | Both update durable records | CAS/uniqueness conflict tests |
| Corrupt delivery or subscription JSON could change routing evidence | SQLite stores structured columns and content | Cold-read tamper tests |
| Replay after subscription removal could call stale code | Declaration and handler lifetimes differ | Replay selects only current enabled declarations/live handlers |

## Decisions Required

No user-blocking decision remains. The chosen semantics are conservative local contracts and do not prevent a future broker-backed implementation.

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| Can Event feed Memory without either lower crate depending on the other? | Cross-Runtime E2E | Typed event is handled into a recalled Memory | Low | Runtime implementation |
| Can custom Router/Dispatcher remain testable? | Injected implementation tests | Manager accepts valid output and rejects scope redirection | Low | Runtime implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| A process-local handler is uniquely bound to one durable subscription ID | Matches Local Bus scope | Replace EventBus with broker consumer bindings later |
| Default retry limit is 3 and hard-capped at 10 | Bounded and subscription/policy configurable | Replace retry policy behind Dispatcher/Policy contracts |
| Event Registry is rebuilt at startup | No sixth event-type table is designed | Add a catalog table or Plugin declarations in a later schema version |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| Cross-process consumer ownership/leases | Distributed Bus is explicitly excluded | Add lease/consumer-group semantics with broker implementation |
| Delayed and background delivery | No scheduler Runtime exists | Add scheduler-backed Dispatcher |
| Permission/RBAC mapping | Permission Runtime is not implemented yet | Bind namespace, source and replay operations in the next security phase |
| UI Timeline/Replay controls | Current workspace is backend-only | Consume stable list/replay/dead-letter contracts later |

## Recommended Implementation Boundary

### Implement now

- Typed Event envelope/definition/source/subscription/delivery/replay/policy/dead-letter domain.
- Manager, Bus, Registry, Router, Dispatcher, Policy, Replay/Lifecycle, Observer and Interceptor contracts.
- In-memory and strict SQLite implementations with exactly five tables.
- Idempotent publish, deterministic fan-out, bounded retry, durable dead letter and explicit replay.
- Root typed Event-to-Memory handler and cross-Runtime test.

### Do not implement now

- Brokers, event sourcing, CQRS, delayed scheduler, distributed ownership, streaming, UI or automatic producer rewrites.

### Interfaces or data contracts to freeze

- Namespace isolation, typed key/category/schema version and event ID idempotency.
- Stable delivery identity and at-least-once handler contract.
- Archived original Event plus separate replay/dead-letter audit.
- Five-table schema.

### Areas that must remain reversible

- Router matching rules, priority weights, retry counts and Context/business adapters.

## Verification Plan

### Automated

- Unit tests: validation, type registry, route matching, lifecycle and policy.
- Integration tests: publish/fan-out/idempotency/retry/dead-letter/replay/unsubscribe/panic/CAS.
- Persistence tests: five-table audit/index/no-FK, reopen and tamper detection.
- Cross-Runtime: typed Event handler stores Memory and Recall finds it.
- Static analysis: strict Clippy, formatting and diff checks.

### Manual

- Happy/empty/failure/recovery/namespace/policy boundaries are represented by assertion tests.
- UI, responsive and accessibility checks are not applicable to this backend-only phase.

### Observability

- Observer receives register/publish/route/deliver/retry/dead-letter/replay lifecycle observations.
- Event, replay and dead-letter records retain actor, reason, attempts and timestamps.

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file planned
