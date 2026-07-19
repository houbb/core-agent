# P6 Execution Runtime — Unknowns Report

## Scope

- Evidence read: `000-roadMap.md`, `007-execution.md`, and only the directly related P3/P4/P5 public contracts and persistence patterns.
- Goal: execute an approved P5 Plan through a command-oriented, resumable, auditable sequential runtime.
- Explicit exclusions: parallel/DAG scheduler, distributed execution, queue, workflow engine, approval flow, multi-agent execution.

## Known knowns

- Planning owns Plan generation/review; Execution must not generate or rewrite a Plan.
- P5 exposes an approved `PlanStatus::Ready`, task/step dependencies and per-step `max_attempts`.
- P3 ToolManager owns live Tool lookup, validation, permission, timeout, audit and cooperative current-process cancellation.
- P3 deliberately does not retain request/result bodies or provide durable exactly-once replay.
- P4 snapshots restore workspace files as a non-destructive overlay; they are not a universal compensation primitive.
- P6 requires five SQLite tables: `execution`, `checkpoint`, `execution_state`, `retry`, `rollback`.

## Prioritized unknowns and decisions

| Priority | Unknown | Impact | Decision for P6 |
|---|---|---|---|
| Blocker | What happens after a crash during a side-effecting command? | Automatic replay can duplicate writes. | Persist dispatch intent before invocation. A recovered `RUNNING` action is treated as outcome-unknown and is never replayed automatically; only safe `PAUSED`/boundary state can resume. Exactly-once is not claimed. |
| Blocker | Is Rollback equivalent to restoring a Workspace snapshot? | A Tool may affect remote systems or non-file state. | No. Rollback calls only an executor-declared compensation operation, in reverse completion order, and records unsupported/skipped outcomes explicitly. |
| High | Does execution mutate the P5 Plan? | Mutation couples runtimes and prevents stable replay. | No. Each execution stores the immutable Plan snapshot/hash and owns separate task/step progress. Multiple executions of one Plan are allowed. |
| High | How do P5 Action and the recommended Command model coexist? | Direct Action→Tool coupling blocks future Agent/Workflow commands. | Normalize every Action to a stable `ExecutionCommand`. MVP supports Tool commands plus side-effect-free built-in control markers for Analyze/Produce/Verify. Tool is one command implementation. |
| High | Who retries? | Layered retries can multiply side effects and latency. | Execution is the outer command-attempt owner and honors `Step.max_attempts`; ToolManager remains the inner single Tool invocation boundary. Only retryable terminal failures are retried. |
| High | Can pause/cancel interrupt an in-flight operation? | Generic executors may not cooperate. | Cancellation is cooperative and passed to the ActionExecutor. The Tool adapter bridges it to `ToolManager::cancel`. Pause is a safe-boundary operation and never interrupts a command midway. |
| High | How is scheduling defined without implementing a DAG scheduler? | Dependency semantics still need deterministic progress. | A sequential dispatcher selects one ready step at a time, respecting task/step dependencies and deterministic priority/key order. It does not batch or parallelize. |
| Medium | What command output is persisted? | Outputs may be large or sensitive. | Persist bounded status/usage/error summaries only; never persist Tool request parameters, content or attachments in execution audit rows beyond the immutable approved Plan snapshot. |
| Medium | What does checkpoint mean? | Checkpoints can be mistaken for external-side-effect transactions. | A checkpoint is an integrity-hashed execution-progress boundary, not a database or external-system transaction. It is written after successful steps and on lifecycle boundaries. |
| Medium | How are extension failures handled? | Observer/interceptor failures can corrupt lifecycle or identity. | Policy/interceptor failures fail closed; command/execution/action identity is immutable across interceptors; observer panics are isolated. |

## Conservative assumptions

- `Analyze`, `Produce` and `Verify` without Tool binding are control markers in P6; the default executor acknowledges them without external side effects. Model/Agent-backed implementations remain extension points.
- SQLite operations remain synchronous behind the async Store contract, matching the existing project pattern; async offloading is deferred.
- Paused executions are returned from `execute`/`resume`. Resuming a terminal execution or an execution with an outcome-unknown running action is rejected.

## Verification targets

- Unit assertions: state machine, deterministic dispatcher, command identity, retry bounds, checkpoint hash, strict validation.
- Runtime E2E: sequential dependency execution, retry, pause/resume, cancel, compensation records, SQLite cold recovery/tamper rejection.
- Cross-runtime E2E: P5 approved Plan → P6 Tool command → P3 ToolManager, including Tool permission and cancellation bridge.

## Deferred risks

- Exactly-once external effects require provider idempotency keys or a durable result ledger.
- Generic compensation requires command-specific protocols and cannot be inferred from Tool parameters.
- Parallel scheduling needs resource locking, deterministic join semantics and conflict handling.
- SQLite calls should eventually be isolated from async workers for high-throughput workloads.
