//! P6 schema. Relations are logical and indexed; SQLite foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Durable Execution aggregate including the immutable approved Plan snapshot.
CREATE TABLE IF NOT EXISTS execution (
    id              TEXT PRIMARY KEY NOT NULL, -- UUID identity
    plan_id         TEXT NOT NULL,             -- Logical Planning Runtime reference
    plan_version    INTEGER NOT NULL,
    plan_hash       TEXT NOT NULL,             -- SHA-256 of immutable Plan snapshot
    status          TEXT NOT NULL,
    version         INTEGER NOT NULL,           -- Compare-and-swap aggregate version
    current_task_id TEXT,
    current_step_id TEXT,
    content         TEXT NOT NULL,              -- Strict serialized Execution aggregate
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',   -- Audit creation time
    update_time     TEXT NOT NULL DEFAULT '',   -- Audit update time
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_execution_plan_status ON execution(plan_id, status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_execution_status_version ON execution(status, version, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_execution_plan_hash ON execution(plan_id, plan_hash);

-- Integrity-hashed execution progress boundaries; not external-system transactions.
CREATE TABLE IF NOT EXISTS checkpoint (
    id              TEXT PRIMARY KEY NOT NULL,
    execution_id    TEXT NOT NULL,
    sequence        INTEGER NOT NULL,
    label           TEXT NOT NULL,
    hash            TEXT NOT NULL,
    content         TEXT NOT NULL,              -- Strict serialized ExecutionCheckpoint
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_checkpoint_execution_sequence ON checkpoint(execution_id, sequence);
CREATE INDEX IF NOT EXISTS idx_checkpoint_execution_created ON checkpoint(execution_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_checkpoint_hash ON checkpoint(hash);

-- Append-only lifecycle/progress timeline.
CREATE TABLE IF NOT EXISTS execution_state (
    id              TEXT PRIMARY KEY NOT NULL,
    execution_id    TEXT NOT NULL,
    sequence        INTEGER NOT NULL,
    from_status     TEXT,
    to_status       TEXT NOT NULL,
    reason          TEXT NOT NULL,
    content         TEXT NOT NULL,              -- Strict serialized ExecutionStateRecord
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_execution_state_sequence ON execution_state(execution_id, sequence);
CREATE INDEX IF NOT EXISTS idx_execution_state_status ON execution_state(execution_id, to_status, created_at DESC);

-- Command retry schedule/resume audit; request and result bodies are excluded.
CREATE TABLE IF NOT EXISTS retry (
    id              TEXT PRIMARY KEY NOT NULL,
    execution_id    TEXT NOT NULL,
    step_id         TEXT NOT NULL,
    action_id       TEXT NOT NULL,
    attempt         INTEGER NOT NULL,
    delay_ms        INTEGER NOT NULL,
    error_kind      TEXT NOT NULL,
    status          TEXT NOT NULL,
    content         TEXT NOT NULL,              -- Bounded serialized RetryRecord
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_retry_execution_step ON retry(execution_id, step_id, attempt);
CREATE INDEX IF NOT EXISTS idx_retry_status_created ON retry(status, created_at DESC);

-- Explicit command compensation outcomes in reverse completion order.
CREATE TABLE IF NOT EXISTS rollback (
    id              TEXT PRIMARY KEY NOT NULL,
    execution_id    TEXT NOT NULL,
    step_id         TEXT NOT NULL,
    action_id       TEXT NOT NULL,
    command_id      TEXT NOT NULL,
    status          TEXT NOT NULL,
    error_kind      TEXT,
    content         TEXT NOT NULL,              -- Bounded serialized RollbackRecord
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_rollback_execution_created ON rollback(execution_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_rollback_command_status ON rollback(command_id, status);
"#;
