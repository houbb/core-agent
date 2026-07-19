//! P5 schema. All relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Goal aggregate. Intent is embedded to preserve the five-table MVP boundary.
CREATE TABLE IF NOT EXISTS goal (
    id              TEXT PRIMARY KEY NOT NULL, -- UUID identity
    intent_id       TEXT,                      -- Optional reusable Intent identity
    intent          TEXT,                      -- Strict embedded Intent JSON
    title           TEXT NOT NULL,
    status          TEXT NOT NULL,             -- PROPOSED/ACTIVE/SATISFIED/CANCELLED
    priority        INTEGER NOT NULL DEFAULT 0,
    session_id      TEXT,                      -- Logical Session reference
    workspace_id    TEXT,                      -- Logical Workspace reference
    version         INTEGER NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',-- Non-secret JSON metadata
    content         TEXT NOT NULL,             -- Strict serialized Goal
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',  -- Audit creation time
    update_time     TEXT NOT NULL DEFAULT '',  -- Audit update time
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_goal_status_priority ON goal(status, priority DESC, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_goal_intent ON goal(intent_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_goal_session_workspace ON goal(session_id, workspace_id, updated_at DESC);

-- Plan aggregate and canonical Planning Graph.
CREATE TABLE IF NOT EXISTS plan (
    id              TEXT PRIMARY KEY NOT NULL,
    goal_id         TEXT NOT NULL,             -- Logical Goal reference
    strategy_key    TEXT NOT NULL,
    status          TEXT NOT NULL,
    version         INTEGER NOT NULL,
    review          TEXT,                      -- Strict optional PlanReview JSON
    metadata        TEXT NOT NULL DEFAULT '{}',
    graph           TEXT NOT NULL,             -- Strict PlanningGraph JSON
    content         TEXT NOT NULL,             -- Strict serialized Plan aggregate
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_plan_goal_status ON plan(goal_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_plan_status_version ON plan(status, version, updated_at DESC);

-- Tasks belonging logically to a Plan; no SQLite foreign key is used.
CREATE TABLE IF NOT EXISTS task (
    id              TEXT PRIMARY KEY NOT NULL,
    plan_id         TEXT NOT NULL,
    task_key        TEXT NOT NULL,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 0,
    dependencies    TEXT NOT NULL DEFAULT '[]',
    content         TEXT NOT NULL,             -- Strict serialized Task
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_task_plan_key ON task(plan_id, task_key);
CREATE INDEX IF NOT EXISTS idx_task_plan_status ON task(plan_id, status, priority DESC);

-- Atomic executable intent only; P5 persists it but never invokes it.
CREATE TABLE IF NOT EXISTS step (
    id              TEXT PRIMARY KEY NOT NULL,
    plan_id         TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    step_key        TEXT NOT NULL,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL,
    dependencies    TEXT NOT NULL DEFAULT '[]',
    action_kind     TEXT NOT NULL,
    tool_key        TEXT,
    content         TEXT NOT NULL,             -- Strict serialized Step + Action
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_step_plan_key ON step(plan_id, step_key);
CREATE INDEX IF NOT EXISTS idx_step_task_status ON step(task_id, status, step_key);
CREATE INDEX IF NOT EXISTS idx_step_tool ON step(tool_key, action_kind);

-- Immutable full Plan snapshots with semantic integrity hash.
CREATE TABLE IF NOT EXISTS plan_snapshot (
    id              TEXT PRIMARY KEY NOT NULL,
    plan_id         TEXT NOT NULL,
    plan_version    INTEGER NOT NULL,
    label           TEXT NOT NULL,
    content         TEXT NOT NULL,             -- Strict serialized PlanSnapshot
    hash            TEXT NOT NULL,             -- SHA-256 of embedded Plan
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_plan_snapshot_plan ON plan_snapshot(plan_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_plan_snapshot_version ON plan_snapshot(plan_id, plan_version DESC);
CREATE INDEX IF NOT EXISTS idx_plan_snapshot_hash ON plan_snapshot(hash);
"#;
