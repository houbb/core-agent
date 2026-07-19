//! P7 schema. Runtime relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Durable Agent aggregate with immutable Profile and Policy snapshots in content.
CREATE TABLE IF NOT EXISTS agent (
    id                   TEXT PRIMARY KEY NOT NULL, -- UUID identity
    name                 TEXT NOT NULL,
    profile_id           TEXT NOT NULL,             -- Logical Profile catalog reference
    profile_version      INTEGER NOT NULL,
    state                TEXT NOT NULL,
    session_id           TEXT,
    workspace_id         TEXT,
    current_goal_id      TEXT,
    current_plan_id      TEXT,
    current_execution_id TEXT,
    version              INTEGER NOT NULL,          -- Compare-and-swap aggregate version
    content              TEXT NOT NULL,             -- Strict serialized Agent aggregate
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    create_time          TEXT NOT NULL DEFAULT '',  -- Audit creation time
    update_time          TEXT NOT NULL DEFAULT '',  -- Audit update time
    create_user          TEXT NOT NULL DEFAULT 'system',
    update_user          TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_agent_state_updated ON agent(state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_profile_state ON agent(profile_id, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_session_workspace ON agent(session_id, workspace_id);

-- Reusable versioned declarative Agent Profiles; secrets are rejected by validation.
CREATE TABLE IF NOT EXISTS agent_profile (
    id              TEXT PRIMARY KEY NOT NULL,
    profile_key     TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    policy_id       TEXT,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,                  -- Strict serialized AgentProfile
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_agent_profile_name ON agent_profile(name, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_profile_policy ON agent_profile(policy_id, updated_at DESC);

-- Integrity-hashed safe-boundary Agent snapshots.
CREATE TABLE IF NOT EXISTS agent_snapshot (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL,
    agent_version   INTEGER NOT NULL,
    state           TEXT NOT NULL,
    label           TEXT NOT NULL,
    hash            TEXT NOT NULL,
    content         TEXT NOT NULL,                  -- Strict serialized AgentSnapshot
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_snapshot_version ON agent_snapshot(agent_id, agent_version);
CREATE INDEX IF NOT EXISTS idx_agent_snapshot_created ON agent_snapshot(agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_snapshot_hash ON agent_snapshot(hash);

-- Append-only lifecycle and coordination timeline.
CREATE TABLE IF NOT EXISTS agent_state (
    id                   TEXT PRIMARY KEY NOT NULL,
    agent_id             TEXT NOT NULL,
    sequence             INTEGER NOT NULL,
    from_state           TEXT,
    to_state             TEXT NOT NULL,
    goal_id              TEXT,
    plan_id              TEXT,
    execution_id         TEXT,
    reason               TEXT NOT NULL,
    actor                TEXT NOT NULL,             -- Actor responsible for this transition
    content              TEXT NOT NULL,             -- Strict serialized AgentStateRecord
    created_at           TEXT NOT NULL,
    create_time          TEXT NOT NULL DEFAULT '',
    update_time          TEXT NOT NULL DEFAULT '',
    create_user          TEXT NOT NULL DEFAULT 'system',
    update_user          TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_state_sequence ON agent_state(agent_id, sequence);
CREATE INDEX IF NOT EXISTS idx_agent_state_status ON agent_state(agent_id, to_state, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_agent_state_execution ON agent_state(execution_id, created_at DESC);

-- Versioned Agent-lifecycle policy declarations; Tool permissions remain separate.
CREATE TABLE IF NOT EXISTS agent_policy (
    id              TEXT PRIMARY KEY NOT NULL,
    policy_key      TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,                 -- Strict serialized AgentPolicyDefinition
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_agent_policy_name ON agent_policy(name, updated_at DESC);
"#;
