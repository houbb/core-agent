# P7 Agent Runtime — Unknowns Report

## Scope

- Design sources: `000-roadMap.md` and `008-agent.md` only.
- Code evidence: current Planning Runtime and Execution Runtime public contracts, persistence conventions, and workspace composition.
- Objective: implement a single-Agent runtime that owns one Agent's lifecycle across multiple Goals while delegating planning and execution to P5/P6.
- Explicitly out of scope: multi-Agent coordination, workflow, long-term memory, human approval UI, marketplace/package deployment, cloud/distributed runtime, and real Tool/Context implementations.

## Known knowns

1. Agent Runtime is a top-level runtime, not a Session alias or a single Execution wrapper.
2. One Agent can accept multiple Goals sequentially and must expose create/start/stop/finish/destroy lifecycle operations.
3. Planning Runtime already creates durable Goal and Plan records; Execution Runtime already owns durable Execution, checkpoints, retries, cancellation, and recovery.
4. The design requires Agent Profile, Capability, Policy, Observer, Coordinator, Snapshot, Registry, and Lifecycle extension points.
5. The five required SQLite tables are `agent`, `agent_profile`, `agent_snapshot`, `agent_state`, and `agent_policy`; project rules require audit columns, comments, indexes, and no foreign keys.
6. No frontend crate currently exists in the workspace, so P7 can provide monitor/timeline data contracts but not fabricate a UI application.

## Material unknowns and decisions

| Priority | Unknown | Evidence / risk | Conservative decision |
| --- | --- | --- | --- |
| P0 | How can `stop` identify an Execution while it is running? | P6 `execute` returns its ID only after the run finishes or pauses. Agent cannot reliably control an opaque live run. | Add compatible P6 `prepare(plan, request)` and `start(id)` APIs; keep `execute` as `prepare + start`. The Agent persists Goal/Plan/Execution IDs before starting side effects. |
| P0 | What happens when planning or execution fails? | Leaving Agent in `Running` would make recovery ambiguous. The suggested state list has no explicit failure state. | Add durable `Failed`; record bounded error kind/message and allow an explicit restart to `Ready`. Lower-runtime artifacts remain auditable; no unsafe cross-runtime rollback is attempted. |
| P0 | Can two callers run the same Agent? | Concurrent Goals would violate the single-Agent lifecycle and race durable state. | Process-local live-operation guard plus optimistic store versioning. One Agent accepts one live Goal at a time; cross-process writers fail closed on version conflict. |
| P0 | What authority does Agent Policy have? | Replacing Tool Runtime permissions would cross a runtime ownership boundary. `Ask` also requires human collaboration, which is excluded. | Agent Policy gates Agent lifecycle operations only. `Ask` denies in the MVP. Tool permissions remain owned by Tool Runtime. |
| P0 | What can a snapshot restore? | Rewinding an Agent could imply replaying already-completed external side effects. | Snapshot only non-running states; restore only when the current Agent version still equals the snapshot version. Restore changes Agent metadata/state only and never replays or rolls back Planning/Execution side effects. |
| P1 | Is a Profile a mutable live configuration? | Editing a catalog profile while an Agent runs would silently change behavior. | Profile is a reusable versioned template. Each Agent embeds immutable Profile and Policy snapshots at creation. Later catalog changes affect only new Agents. |
| P1 | How are Session, Workspace, Model, Tool, and Memory bound? | Direct dependencies would collapse runtime boundaries; Model/Memory are not consumed by P5/P6 today. | Store normalized IDs/keys as declarations. Validate bound Session/Workspace IDs and fail closed on undeclared planning tools. Model and Memory keys remain visible declarations until their owning runtimes integrate them. |
| P1 | How should stop behave during Planning? | No Execution ID exists until a Plan is prepared. Task cancellation alone could leave Agent state stale. | A per-Agent stop flag is checked at safe boundaries. Once P6 is prepared, stop pauses that known Execution before side effects. The run owner performs the final Agent state commit. |
| P1 | What if Goal creation succeeds but Plan creation fails? | P5 operations and Agent persistence cannot share one transaction. Deleting the Goal would erase useful audit evidence. | Preserve the lower-runtime record, mark the Agent `Failed`, report the failed stage, and allow retry as a new Goal. |
| P1 | What does `destroy` mean? | Hard deletion would conflict with audit/history and snapshot references. | `destroy` is a terminal soft lifecycle transition; records remain queryable. |
| P2 | How are timeline and monitor screens supported without a UI? | `008-agent.md` describes UX, but the repository contains no UI surface. | Persist ordered Agent state records and expose list/find APIs. Frontend rendering is deferred to the future UI host. |
| P2 | Is synchronous SQLite access acceptable in async APIs? | Existing runtimes use the same pattern; replacing it would expand scope. | Follow the established store convention for P7 and record blocking isolation as future infrastructure work. |

## Lifecycle contract

```text
Created -> Ready
Ready | Waiting -> Running
Running -> Waiting | Paused | Failed
Paused | Failed | Completed -> Ready
Ready | Waiting -> Completed
Created | Ready | Waiting | Paused | Failed | Completed -> Destroyed
Destroyed -> terminal
```

- `Waiting` means the previous Goal completed and the Agent can accept another Goal.
- `Paused` may retain a current Execution ID and resume it on `start`.
- `Completed` means the caller explicitly finished this Agent lifecycle; it is restartable by design.

## Architecture boundary

- New crate: `core-agent-agent`.
- Dependency direction: Agent Runtime depends on Planning Runtime and Execution Runtime; neither lower runtime depends on Agent Runtime.
- `AgentCoordinator.next` creates Goal + Plan and prepares Execution.
- `AgentCoordinator.run` starts the prepared Execution; `resume` and `pause` delegate control to P6.
- Manager owns lifecycle transitions, policy evaluation, observer/interceptor isolation, snapshots, and durable Agent aggregation.
- Registry/store owns catalog and Agent persistence using compare-and-swap versions and strict cold-read validation.

## Security and compatibility constraints

- Reject blank/oversized IDs, names, configuration, metadata, errors, and reasons.
- Reject sensitive-looking Profile configuration keys; secrets belong in a future secret provider.
- Treat unknown policy decisions and `Ask` as deny.
- Profile tool declarations are an upper bound: a Planning context cannot introduce undeclared tools.
- Interceptors cannot change Agent identity/version; observer panics are isolated.
- Existing P6 `execute` behavior remains source-compatible.

## Verification targets

1. Domain and lifecycle assertion tests.
2. Real Agent -> Planning -> Execution end-to-end run across multiple Goals.
3. Stop/pause/resume behavior using a controllable executor.
4. Profile capability/tool boundary and policy-denial tests.
5. Snapshot version/replay protection tests.
6. SQLite round-trip, cold-read integrity, schema/audit-column, index, and no-foreign-key tests.
7. Workspace-level Agent -> Planning -> Execution -> Tool integration test.

## Deferred unknowns

- Cross-process distributed locking and ownership leases.
- Human approval semantics for `Ask`.
- Profile package signing/import/export and marketplace identity.
- Model/Memory runtime enforcement and deployable Agent packaging.
- UI rendering and streaming live monitor transport.
