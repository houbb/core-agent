# P1 Context Runtime Post-Implementation Review

## Metadata

- **Task / Feature:** Review and enhance `002-context.md`
- **Date completed:** 2026-07-17
- **Reviewer:** Codex
- **Related Unknowns Report:** `design-docs/002-context-unknowns-report.md`
- **Related implementation notes:** `design-docs/002-context-implementation-notes.md`
- **Related PR / commit:** None

## Behavior Changes

### Before

- `max_tokens` was ignored and `max_messages` selected the oldest rows.
- Workspace, Memory, Tool, and Plugin content disappeared during composition.
- Hashes represented totals and build time instead of actual Context content.
- Build duration was always zero and the public response was not sufficient for Model Runtime consumption.
- Snapshot lists silently dropped invalid rows and the table lacked required audit fields.
- Serializer, Cache, and Observer extension contracts were absent.

### After

- Request and Slot budgets are enforced; required content returns an explicit error instead of silently exceeding the limit.
- The newest N messages are returned chronologically, while a required zero-token metadata segment preserves the original conversation count after trimming.
- All eight Slots and their ordered source segments survive composition and snapshot replay.
- Semantic hashes cover canonical structured content and source metadata while excluding build identity/time; stale hashes are rejected on save.
- Pipeline duration is measured, Environment collection runs outside async worker threads, and observers are panic-isolated.
- The public API can return a complete Context and load complete snapshots.
- SQLite migrates audit fields additively, validates metadata strictly, and restores file-backed snapshots after reopening.

## Files and Systems Affected

| Area | Change | Why it changed |
|---|---|---|
| Context domain | Added ToolContext, ordered segments, Conversation source, semantic hashing | Preserve all P1 data and provenance |
| Pipeline/Reducer/Composer | Request config, Slot enable/budget/priority, deterministic trimming, observers, duration | Make public configuration and lifecycle real |
| Providers | Latest-message paging, statistics segment, strict user input, non-blocking environment collection | Correct data selection and failure behavior |
| Public API/DTO | Complete Context build and replay APIs | Support Model Runtime and Inspector consumers |
| Snapshot Store | Audit migration, strict conversions/hash checks, file recovery | Reliable replay and audit |
| Session Store integration | Stable `created_at, rowid` message ordering | Resolve equal-timestamp ordering |

## Assumptions Review

| Assumption | Status | Evidence | Action |
|---|---|---|---|
| Archived Sessions remain readable; deleted Sessions are rejected | Confirmed | Lifecycle E2E test | Keep |
| TokenCounter is an estimate | Confirmed | Character-based implementation and tests | Monitor until Model Runtime |
| Future Slot content can remain structured placeholders | Confirmed | Custom Provider and all-Slot tests | Keep |
| P1 reduction is deterministic and non-AI by default | Confirmed | Reducer config and budget tests | Keep |

## Unknowns Review

### Resolved

| Unknown | Resolution | Evidence |
|---|---|---|
| Required content exceeds budget | Explicit `TokenBudgetExceeded` | Unit + E2E tests |
| Context hash identity | Canonical semantic content, sources, distribution; no build ID/time | Stable/canonical hash tests |
| Legacy audit migration | Add columns and backfill from `created_at` | Migration assertion test |
| Later Slot representation | Typed placeholders plus ordered ContextSegment provenance | All-Slot/custom Provider tests |

### Remaining

| Unknown | Risk | Follow-up |
|---|---|---|
| Exact model tokenization | Estimated counts may differ from provider billing | Replace TokenCounter during P2 integration |
| Snapshot list/prune under multi-writer databases | Ordering is deterministic but no distributed transaction policy exists | Resolve with future Postgres/multi-writer Store |
| Custom Pipeline plus both Pipeline/Service snapshot stores | Same ID may be upserted twice | Document ownership; unify configuration in a later API cleanup |
| Environment command timeout | Local Git/OS commands run on blocking workers but have no explicit deadline | Add timeout policy with Observation Runtime metrics |

### Newly discovered

| Unknown | Impact | Recommended action |
|---|---:|---|
| Public exhaustive matches on ContextSource | New Conversation variant can require consumer changes | Freeze source taxonomy before 1.0 |
| ContextResponse contains complete Context plus summaries | Larger response payload | Consumers that need only framework data should use `build`; revisit DTO shape before 1.0 |

## Deviations

| Deviation | Reason | User-visible effect | Risk | Approved |
|---|---|---|---|---|
| Added `ContextSource::Conversation` | Existing code mislabeled conversation data as SYSTEM | Correct source trace | Pre-1.0 exhaustive matches change | Yes |
| Added ToolContext and ordered segments to Context | Eight Slots and Inspector/replay require complete data | Full Tool/source visibility | Context struct initializer changes | Yes |
| Default SummaryReducer disables summary | P1 explicitly excludes automatic compression | Deterministic Last-N behavior | Explicit opt-in compatibility remains | Yes |

## Verification Evidence

### Automated checks

- [x] Unit tests — P1 52/52
- [x] Integration/end-to-end tests — P1 4/4
- [x] Migration tests
- [x] Extension contract tests
- [x] Static analysis
- [x] Workspace build
- [x] Strict P0/P1 Clippy
- [x] Format check

P0 regression evidence: 36 unit assertions and 4 end-to-end tests passed in the same workspace run. The root crate still reports its pre-existing ambiguous glob re-export warnings; the changed crates pass strict `-D warnings` Clippy.

### Manual checks

- [x] Happy path represented by E2E
- [x] Empty/trimmed state represented by assertions
- [x] Failure paths represented by ownership, budget, corruption, and stale-hash tests
- [x] Recovery represented by file database reopen
- [x] Lifecycle boundary represented by archived/deleted E2E
- [ ] UI/responsive/accessibility — no P1 UI exists
- [ ] Production performance — requires Observation Runtime metrics

### Production or runtime evidence

- Logs: Not introduced; Observation Runtime is a later phase.
- Metrics: ContextObserver receives stage, segment, token, and duration data.
- Traces: Observer extension point only.
- User validation: Awaiting release/version selection.

## Three Review Passes

### Pass 1 — Architecture and API

- Confirmed dependency direction and trait-based extensibility.
- Fixed inactive Slot priority configuration and preserved ordered source segments.
- No ADR, `CLAUDE.md`, or configured engine specialists were present, so those checks were skipped.

### Pass 2 — Invariants and failure paths

- Fixed strict user/Conversation data validation, async blocking, snapshot hash consistency, audit backfill, and stored/returned duration consistency.
- Verified cross-session isolation, deleted-state rejection, corrupt rows, and rollback-safe additive migration.

### Pass 3 — Regression and maintainability

- Fixed conversation totals after full message trimming, canonicalized metadata hashing, documented public extension contracts, and added file recovery coverage.
- Production code introduces no `unsafe`, `panic`, `expect`, TODO, or FIXME; panic/unwrap occurrences are limited to tests and ignored examples.

### Code Review Verdict

- **Architecture:** CLEAN
- **SOLID:** COMPLIANT
- **Testability:** TESTABLE
- **ADR compliance:** NO ADRS FOUND
- **Verdict:** APPROVED WITH SUGGESTIONS

Suggestions are the four remaining unknowns above; none block single-process SQLite P1.

## Rollback and Recovery

- **Rollback trigger:** Context ordering, budget behavior, or snapshot replay regression.
- **Code rollback steps:** Revert P1 implementation, tests, and documentation changes.
- **Data rollback steps:** Keep additive audit columns; old readers ignore them and no rows need deletion.
- **Configuration rollback steps:** Use the default Pipeline without custom Slot configs/observers.
- **Recovery verification:** Reopen the snapshot database and load a pre-rollback Context by ID.

## Maintainer Notes

- `ContextRuntime::build` is the complete framework API; `build_context` additionally returns summaries for transport/UI consumers.
- A required segment is never trimmed. If required content cannot fit, the build fails explicitly.
- Conversation ordering uses `message_index`; Slot priority must not be overloaded as a message sequence.
- Semantic hash changes require `Context::refresh_hash` before saving a mutated Context.
- ContextCache is a contract only; P1 intentionally has no caching or invalidation policy implementation.

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill

## Understanding Check

1. **What changed?** P1 now produces a complete, budgeted, replayable Context instead of a lossy summary.
2. **Which old paths enter it?** Both `build` and legacy `build_context` use the same validated service and Pipeline.
3. **Most likely failures?** Required content exceeds budget, persisted snapshot corruption, or future Store/cache concurrency policy mismatch.
4. **Evidence versus assumptions?** Current SQLite behavior is test-backed; model token accuracy and multi-writer behavior remain assumptions.
5. **Safe rollback?** Revert code and retain additive audit columns/data.
6. **First place to inspect later?** `application/pipeline.rs`, then Reducer/Composer, then `persistence/store.rs`.
7. **Future difficult contracts?** Context hash semantics, ContextSource taxonomy, snapshot JSON compatibility, and model-specific tokenization.
