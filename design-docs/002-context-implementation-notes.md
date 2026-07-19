# P1 Context Runtime Implementation Notes

## Metadata

- **Task / Feature:** Review and enhance `002-context.md`
- **Date started:** 2026-07-17
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/002-context-unknowns-report.md`
- **Related plan / issue / PR:** P1 iteration; no PR

## Confirmed Discoveries

### Discovery D-001 — Public limits were not runtime limits

- **What was discovered:** `max_tokens` never reached the Reducer and `max_messages` selected the oldest rows.
- **Evidence:** `ContextPipeline::execute` created a default config; ConversationProvider queried offset zero against ascending rows.
- **Why it matters:** Request behavior differed from its API contract.
- **Affected scope:** Service, Pipeline, Reducer, ConversationProvider.
- **Action taken:** Request config now reaches the Pipeline; ConversationProvider reads the newest rows and preserves chronological order.

### Discovery D-002 — Composed Context was lossy

- **What was discovered:** Several Slot branches counted tokens but discarded content; hash excluded semantic content.
- **Evidence:** DefaultComposer Workspace/Memory/Tool/Plugin match arms and hash payload.
- **Why it matters:** Replay, Model Runtime consumption, Context Inspector, and audit were unreliable.
- **Affected scope:** Domain Context, Composer, DTO, snapshot replay.
- **Action taken:** Composer preserves all eight Slots, API exposes the complete Context, and hashes semantic content deterministically.

### Discovery D-003 — Snapshot persistence hid invalid data

- **What was discovered:** List conversion silently dropped rows and defaulted invalid UUID/time values; audit columns were absent.
- **Evidence:** Snapshot Store `filter_map`/`unwrap_or_default` and schema.
- **Why it matters:** Replay and audit could report incomplete or false data.
- **Affected scope:** SQLite schema, migration, Store conversions.
- **Action taken:** Snapshot schema migrates audit columns additively and conversions return explicit errors.

## Decisions

### Decision DEC-001 — P1 reduction remains deterministic and non-AI

- **Decision:** Default to Last-N plus token-budget trimming; retain explicitly enabled legacy extractive summary behavior only for compatibility.
- **Alternatives considered:** AI summary, automatic compression, removal of existing SummaryReducer API.
- **Reason:** AI summary and automatic compression are explicit P1 non-goals; removing the API would create avoidable breakage.
- **Evidence:** `002-context.md` MVP non-goals and current public exports.
- **Owner / approver:** User scope + Codex implementation.
- **Reversibility:** High.
- **Follow-up:** Add dedicated compression strategies in P1.5.

### Decision DEC-002 — Semantic hash excludes build identity and timing

- **Decision:** Hash Session/Conversation identity plus complete structured Context content and token distribution; exclude Context ID, build timestamp, duration, and hash itself.
- **Alternatives considered:** Hash serialized Context wholesale; retain totals-only hash.
- **Reason:** Supports deterministic integrity/deduplication without making every build unique.
- **Evidence:** Snapshot/replay requirements.
- **Owner / approver:** Codex.
- **Reversibility:** Medium; hash semantics are documented and tested.
- **Follow-up:** Add explicit Context versioning in P1.7.

### Decision DEC-003 — Migrate snapshots additively

- **Decision:** Add and backfill required audit columns without deleting old columns or rows.
- **Alternatives considered:** Recreate the table; require a clean database.
- **Reason:** Matches project DB rules and preserves existing P1 data.
- **Evidence:** Existing current schema and P0 migration convention.
- **Owner / approver:** Project convention.
- **Reversibility:** High; old readers ignore new columns.
- **Follow-up:** None.

## Assumptions

### Assumption A-001

- **Assumption:** Archived Sessions may build/read Context for replay, while deleted Sessions may not.
- **Why it is currently acceptable:** Aligns with Session lifecycle and audit goals.
- **Risk:** A future policy may require stricter archived access.
- **How it will be validated:** End-to-end state tests.
- **Reversal plan:** Tighten service validation without schema changes.

### Assumption A-002

- **Assumption:** TokenCounter remains an estimate until Model Runtime provides model-specific tokenization.
- **Why it is currently acceptable:** The P1 design explicitly calls the MVP count an estimate.
- **Risk:** Actual model token counts may differ.
- **How it will be validated:** Budget invariants use the same counter consistently.
- **Reversal plan:** Introduce a TokenCounter trait implementation later.

## Deviations

### Deviation DEV-001 — Add Conversation as an explicit ContextSource

- **Original plan:** The example source list omitted Conversation while defining a Conversation Slot and Provider.
- **Actual implementation:** Added `ContextSource::Conversation`.
- **Reason for deviation:** Labeling persisted conversation history as SYSTEM was observably incorrect for audit and Inspector consumers.
- **User-visible effect:** Conversation segments now report their real source.
- **Data / API effect:** Additive enum variant in the pre-1.0 P1 API.
- **Risk introduced:** External exhaustive matches must handle the new variant.
- **Approval required:** No; required to satisfy the source-traceability invariant.
- **Follow-up:** Freeze source taxonomy before 1.0.

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| Snapshot list/prune concurrency | 2 | Unique IDs and transactional single statements | Future persistence owner | Multi-writer Store implementation |
| Estimated versus model token count | 3 | Explicit estimator contract | Model Runtime owner | P2 model integration |

## Tests Added or Updated

| Test | Purpose | Result |
|---|---|---|
| P1 unit suite (52) | Budget, ordering, Slot config/content, hash, serializer, observer, migration, corruption | Pass |
| P1 end-to-end suite (4) | Full build/replay, ownership/budget failures, lifecycle policy, file recovery | Pass |
| P0 regression suite (40) | Session integration compatibility | Pass |

## Verification Results

- `cargo test --workspace`: passed; P1 52 unit assertions + 4 end-to-end tests, P0 36 unit assertions + 4 end-to-end tests.
- `cargo build --workspace`: passed; only pre-existing root glob re-export warnings remain.
- `cargo clippy -p core-agent-context -p core-agent-session --all-targets -- -D warnings`: passed.
- `cargo fmt -p core-agent-context -p core-agent-session -- --check`: passed.
- `git diff --check`: passed; Git emitted line-ending conversion notices only.

## Rollback Notes

- Code rollback: revert P1 files and related documentation.
- Data rollback: keep additive audit columns; no row rollback is required.
- Configuration rollback: use existing default Pipeline constructors.
- External-system rollback: none.
- Recovery validation: reopen a legacy snapshot database and load its rows.

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
