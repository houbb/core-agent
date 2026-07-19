# P6 Execution Runtime — Implementation Notes

## Implemented

- Added independent `core-agent-execution`; it consumes an immutable approved P5 Plan and never generates or mutates Planning state.
- Added command normalization with stable per-attempt UUID v5 identity. MVP supports Tool commands and side-effect-free built-in markers while preserving future command extension space.
- Added deterministic sequential dependency dispatcher, Execution/Action state machines, lifecycle, policy, interceptor, observer and cooperative control contracts.
- Added safe-boundary pause/resume, cooperative cancellation, cold boundary recovery and explicit refusal to replay an outcome-unknown in-flight command.
- Added centralized bounded retry using `Step.max_attempts`, integrity-hashed checkpoint capture/restore, and reverse-order executor-declared compensation.
- Added atomic CAS persistence and strict cold-read cross-checking for `execution`, `checkpoint`, `execution_state`, `retry` and `rollback`.
- Added root `ToolActionExecutor`: revalidates the live Tool capability, preserves approved capability/target metadata, bridges cancellation to ToolManager, and persists only bounded result summaries.

## Material decisions

- Plan is the immutable execution definition; Execution owns progress and supports multiple executions per Plan.
- Checkpoint restore accepts only the latest non-terminal safe boundary, preventing rewind/replay of already completed external effects.
- A process interruption with a persisted `RUNNING` action is outcome-unknown and cannot auto-resume.
- `after_command` extension failure is observable but cannot retroactively turn a successfully executed side effect into a failed command.
- Rollback never infers compensation from parameters or assumes Workspace restore; unsupported compensation is recorded as `SKIPPED`.
- ExecutionPolicy is evaluated on start, dispatch, retry, rollback, restore, pause, resume and cancel, including live control paths.

## Usage

```rust
let execution = ExecutionManager::builder()
    .executor(Arc::new(ToolActionExecutor::new(tool_manager)))
    .store(Arc::new(SqliteExecutionStore::new("execution.db")?))
    .build()
    .execute(approved_plan, ExecuteRequest::new("agent"))
    .await?;
```

- `pause(id)` requests a safe command-boundary pause.
- `resume(id, actor)` resumes PAUSED or a recoverable cold safe boundary.
- `cancel(id, actor)` cooperatively cancels an active executor and records the actor.
- `restore_checkpoint(checkpoint_id, actor)` restores only the latest safe checkpoint.

## Verification added

- Unit assertions: command identity, strict status parsing, lifecycle transitions and retry bounds.
- Runtime E2E: dependency execution, bounded retry, explicit rollback, pause/checkpoint restore/resume, cooperative cancel, retry-delay cancel consistency, policy denial, post-command hook failure, outcome-unknown crash recovery, SQLite cold recovery and child-column tamper rejection.
- Cross-runtime E2E: approved P5 Tool Action through P6 into P3 ToolManager, capability/target propagation and pre-registration cancellation.

## Deferred

- Exactly-once external effects require provider idempotency/result ledgers.
- Parallel/DAG/distributed scheduling, workflow/approval commands and generic compensation protocols remain outside P6 MVP.
- SQLite calls remain synchronous behind the async store contract and should be offloaded for high-throughput workloads.
