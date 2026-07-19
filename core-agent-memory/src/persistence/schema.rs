//! P8 schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Current structured Memory aggregate; event_id makes at-least-once delivery idempotent.
CREATE TABLE IF NOT EXISTS memory (
    id              TEXT PRIMARY KEY NOT NULL,
    event_id        TEXT NOT NULL UNIQUE,
    namespace       TEXT NOT NULL,              -- Required isolation boundary
    memory_kind     TEXT NOT NULL,              -- EPISODIC or SEMANTIC
    memory_type     TEXT NOT NULL,
    source_kind     TEXT NOT NULL,
    importance      TEXT NOT NULL,
    state           TEXT NOT NULL,
    workspace_id    TEXT,
    agent_id        TEXT,
    goal_id         TEXT,
    execution_id    TEXT,
    policy_id       TEXT,
    version         INTEGER NOT NULL,           -- Compare-and-swap aggregate version
    expires_at      TEXT,
    content         TEXT NOT NULL,              -- Strict serialized Memory aggregate
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_memory_namespace_state ON memory(namespace, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_namespace_type ON memory(namespace, memory_type, importance, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_workspace_goal ON memory(namespace, workspace_id, goal_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_agent_execution ON memory(namespace, agent_id, execution_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_expiry ON memory(namespace, expires_at);

-- Denormalized deterministic index. P8 does not contain embeddings or vectors.
CREATE TABLE IF NOT EXISTS memory_index (
    id              TEXT PRIMARY KEY NOT NULL,
    memory_id       TEXT NOT NULL UNIQUE,
    namespace       TEXT NOT NULL,
    normalized_text TEXT NOT NULL,
    memory_kind     TEXT NOT NULL,
    memory_type     TEXT NOT NULL,
    source_kind     TEXT NOT NULL,
    importance      TEXT NOT NULL,
    state           TEXT NOT NULL,
    workspace_id    TEXT,
    agent_id        TEXT,
    goal_id         TEXT,
    memory_version  INTEGER NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    content         TEXT NOT NULL,              -- Strict serialized MemoryIndexEntry
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_memory_index_filter ON memory_index(namespace, memory_type, importance, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_index_scope ON memory_index(namespace, workspace_id, agent_id, goal_id);

-- Integrity-hashed, explicitly requested Memory snapshots.
CREATE TABLE IF NOT EXISTS memory_snapshot (
    id              TEXT PRIMARY KEY NOT NULL,
    memory_id       TEXT NOT NULL,
    memory_version  INTEGER NOT NULL,
    label           TEXT NOT NULL,
    hash            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_snapshot_version ON memory_snapshot(memory_id, memory_version);
CREATE INDEX IF NOT EXISTS idx_memory_snapshot_created ON memory_snapshot(memory_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_snapshot_hash ON memory_snapshot(hash);

-- Reusable, versioned retention and sensitivity policy declarations.
CREATE TABLE IF NOT EXISTS memory_policy (
    id              TEXT PRIMARY KEY NOT NULL,
    policy_key      TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_memory_policy_name ON memory_policy(name, updated_at DESC);

-- Normalized tags are replace-on-version and are purged by Forget.
CREATE TABLE IF NOT EXISTS memory_tag (
    id              TEXT PRIMARY KEY NOT NULL,
    memory_id       TEXT NOT NULL,
    namespace       TEXT NOT NULL,
    tag             TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_tag_unique ON memory_tag(memory_id, tag);
CREATE INDEX IF NOT EXISTS idx_memory_tag_search ON memory_tag(namespace, tag, updated_at DESC);
"#;
