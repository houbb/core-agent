# P7 Agent Runtime — Post-Implementation Review

## Review 1 — Architecture and ownership

- Result: approved.
- Agent Runtime depends downward on Planning and Execution; neither lower runtime depends on Agent Runtime.
- Agent owns lifecycle, declarations, coordination references and snapshots. Planning owns Goal/Plan; Execution owns commands, retries, checkpoints and side-effect state; Tool/Context/Model implementations were not duplicated.
- Profile/Policy catalogs are reusable declarations while every Agent embeds stable versioned snapshots.
- No ADR references, `CLAUDE.md`, engine-specialist configuration or UI engine were present; those code-review checks were not applicable.

## Review 2 — Safety, concurrency and persistence

- Result: approved after fixes.
- Fixed duplicate live-control replacement, stoppable/non-stoppable ownership confusion, P6 terminal/pause linearization, stop/resume TOCTOU, stop actor loss, side-effect-time policy revocation, multi-Goal stale references, partial lower-artifact linkage, UTF-8 error bounds and cold orphan recovery.
- Outcome-unknown in-flight commands are never called snapshot-safe or replayed.
- Lifecycle extensions cannot mutate Agent identity/Profile/Policy/bindings.
- In-memory and SQLite stores now share snapshot uniqueness/owner and catalog version invariants; SQLite cold reads cross-check identity and structured columns against serialized content.
- Restore is current-version-only; destroyed Agents and running Agents cannot be snapshotted.

## Review 3 — QA and testability

- Result: approved.
- Dependency injection exists for Store, Coordinator, Lifecycle, Policy, Factory, Interceptor, Observer and P6 ActionExecutor.
- Added assertion/E2E coverage for multi-Goal lifecycle, Profile security/tool bounds, actor-aware policies, live stop, planning-stage stop, conflicting runs, resume handoff, late stop attribution, partial coordination, injected Agent commit failure, cold reconciliation, outcome unknown, snapshots, SQLite integrity and Profile/Policy updates.
- Root integration verifies Agent -> Planning -> Execution -> Tool with a real Tool Runtime adapter.
- P6 tests verify Prepare/Start policy timing, never-started Resume policy enforcement and policy revocation before side effects.

## Remaining risks

- Process-local live ownership is combined with durable CAS but is not a distributed lease; multi-process orchestration remains deferred.
- SQLite calls are synchronous inside async methods, consistent with existing phases but not ideal for high concurrency.
- Human `Ask` policy decisions deny in this MVP until a Human Collaboration runtime exists.
- UI monitor transport and rendering remain outside the current backend-only workspace.

## Verification evidence

- `cargo test -p core-agent-agent`
- `cargo test -p core-agent-execution`
- `cargo test -p core-agent --test agent_runtime_integration`
- `cargo test --workspace`
- `cargo clippy -p core-agent-agent -p core-agent-execution --all-targets -- -D warnings`
- `cargo fmt --all -- --check`
