# P7 Agent Runtime — Implementation Notes

## Delivered

- Added the independent `core-agent-agent` crate and root composition exports.
- Implemented durable Agent identity, Profile/Policy snapshots, Capability declarations, lifecycle, Registry/Store, Coordinator, Observer, Interceptor, Factory, Snapshot and recovery contracts.
- Implemented the lifecycle `Created -> Ready -> Running -> Waiting/Paused/Failed -> Completed/Destroyed`, including restart and explicit cold reconciliation.
- Implemented real single-Agent coordination over P5 Planning and P6 Execution. One Agent can accept multiple Goals sequentially; lower-runtime Goal/Plan/Execution IDs remain linked and auditable.
- Added a shared Agent/P6 cooperative control. A stop requested during Planning, before Execution registration, during command execution, or during resume cannot be lost.
- Added actor-aware Agent Policy evaluation and state/observation audit. `Ask` fails closed because human approval is outside P7 MVP.
- Added immutable, hash-verified safe-boundary snapshots with current-version-only restore and no side-effect replay.
- Added five strict SQLite tables: `agent`, `agent_profile`, `agent_snapshot`, `agent_state`, and `agent_policy`; all have audit columns, comments, indexes, no foreign keys, CAS writes and structured-column/JSON cold-read checks.
- Enhanced P6 with compatible `prepare`/`start`, caller-owned `start_with_control`/`resume_with_control`, and separate `Prepare` versus side-effect-time `Start` policy checks. Existing `execute` remains the convenience path.

## Minimal usage

```rust
let planning = Arc::new(PlanningManager::builder().build());
let execution = Arc::new(ExecutionManager::builder().build());
let agents = AgentManager::builder()
    .coordinator(Arc::new(RuntimeAgentCoordinator::new(planning, execution)))
    .build();

let profile = agents
    .register_profile(AgentProfile::new("general", "General Agent"), "admin")
    .await?;
let agent = agents
    .create(CreateAgentRequest::new("worker", profile.id))
    .await?;
agents.start(agent.id, "operator").await?;
let outcome = agents
    .run_goal(
        agent.id,
        AgentGoalRequest::new(
            CreateGoalRequest::new("ship", "ship one change"),
            PlanningContext::default(),
        ),
    )
    .await?;
assert_eq!(outcome.agent.state, AgentState::Waiting);
```

## Material discoveries and resolutions

- P6 originally exposed an Execution ID only after running. P7 required a durable ID before side effects, so P6 now separates preparation from start.
- A process-local duplicate live registration originally risked replacing the active stop signal. Entry-based exclusive registration and shared lower-runtime control close that race.
- Live ownership distinguishes cooperative Goal/Execution work from non-stoppable lifecycle and snapshot mutations; a stop can no longer report success against an operation that cannot observe it.
- Accepting a new Goal clears the previous Goal/Plan/Execution references in the same durable `Running` commit, preventing stop or cold recovery from acting on an earlier completed Goal.
- Cold interruption after lower completion could leave Agent `Running`. `reconcile` now maps completed/paused/failed/cancelled outcomes, pauses recoverable safe boundaries, and marks in-flight outcome-unknown commands as `Failed` without replay.
- Cross-runtime operations cannot share one SQLite transaction. Structured partial-coordination errors and Agent state records retain all known Goal/Plan/Execution IDs instead of deleting lower audit evidence.
- Profile and Policy catalog updates preserve stable keys/creation times and monotonic timestamps; live Agents retain their embedded snapshots.

## Deliberately deferred

- Frontend Agent list/detail/profile/monitor rendering; the runtime exposes ordered state and observation data for a future UI host.
- Multi-Agent, Workflow, long-term Memory, human approval, package marketplace, cloud/distributed ownership and cross-process leases.
- Async isolation for synchronous SQLite access, following the existing runtime persistence convention for this phase.
