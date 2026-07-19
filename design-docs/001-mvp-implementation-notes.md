# P0 Session Runtime Implementation Notes

## Metadata

- **Task:** Review and enhance `001-mvp.md`
- **Date started:** 2026-07-17
- **Implementation owner:** Codex
- **Scope:** `core-agent-session` only; preserve `core-agent-context` compatibility

## Confirmed Discoveries

### D-001 — Public lifecycle was unreachable

- The domain required `READY → RUNNING → PAUSED`, but the public API exposed neither start nor pause.
- Archive and resume therefore failed for normally created sessions.
- Added explicit start and pause operations while preserving existing archive/resume APIs.

### D-002 — Manifest data drifted from Session data

- The default MAIN Conversation was not counted.
- Message append/delete did not update message totals.
- State updates recreated Manifest and reset its statistics.
- Manifest synchronization now preserves statistics and refreshes counts after related writes.

### D-003 — SQLite writes could leave partial aggregates

- Session, Manifest, and MAIN Conversation were inserted independently.
- Added a backward-compatible `SessionStore::create_session_bundle` extension with an atomic SQLite override.

### D-004 — Persistence hid corrupted rows

- Invalid UUID, timestamp, enum, or JSON values were converted to defaults or silently dropped.
- Row conversion now fails explicitly so corruption is observable.

## Decisions

### DEC-001 — Preserve the SessionStore contract

- New trait operations have default implementations.
- Existing Context Runtime and third-party Store implementations remain source-compatible.

### DEC-002 — Migrate audit fields additively

- Existing `created_at` / `updated_at` columns are retained.
- Required `create_time`, `update_time`, `create_user`, and `update_user` columns are added and backfilled.
- No existing column or row is deleted.

### DEC-003 — Keep P0 independent from later Runtime concerns

- No Model, Prompt, Tool, Planner, Memory, or Workflow behavior is introduced.

## Assumptions

- Existing external SQLite databases may use the 0.1.0 schema; startup migration must therefore be non-destructive.
- Archived and deleted sessions are immutable, while READY/RUNNING/PAUSED sessions may accept related data.
- A Session has exactly one MAIN Conversation in P0.

## Verification Plan

- Domain assertion tests for Session and Message state machines.
- Schema and legacy migration tests for all required audit columns.
- Transaction rollback and corrupted-row tests.
- End-to-end lifecycle, persistence restore, Manifest consistency, event, attachment, and lifecycle-hook tests.
- Workspace tests, build, formatting, and Clippy after implementation is complete.

## Verification Results

- `cargo test --workspace`: passed; P0 36 unit assertions + 4 end-to-end tests, P1 33 existing tests.
- `cargo build --workspace`: passed; only pre-existing P1/root warnings remain.
- `cargo fmt -p core-agent-session -- --check`: passed.
- `cargo clippy -p core-agent-session --all-targets -- -D warnings`: passed.
- `git diff --check`: passed; Git reported line-ending conversion notices only.

## Rollback Notes

- Code can be reverted without deleting migrated data.
- Added audit columns are backward-compatible with 0.1.0 readers.
- No destructive migration or external-system change is performed.
