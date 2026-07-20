//! SQLite Schema 定义 — Context Snapshot + Context Reference
//!
//! context_snapshot 表：存储每次 build() 的完整 Context JSON 快照。
//! context_reference 表：存储用户创建的上下文引用。

/// Context Snapshot 建表 DDL
pub const CONTEXT_SNAPSHOT_SCHEMA_SQL: &str = r#"
-- Context Snapshot 表
CREATE TABLE IF NOT EXISTS context_snapshot (
    id                  TEXT PRIMARY KEY NOT NULL,
    session_id          TEXT NOT NULL,
    conversation_id     TEXT,
    created_at          TEXT NOT NULL,
    content             TEXT NOT NULL,
    token_count         INTEGER NOT NULL DEFAULT 0,
    hash                TEXT NOT NULL,
    build_duration_ms   INTEGER NOT NULL DEFAULT 0,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_session
    ON context_snapshot(session_id);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_created
    ON context_snapshot(session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_hash
    ON context_snapshot(hash);
"#;

/// Context Reference 建表 DDL
pub const CONTEXT_REFERENCE_SCHEMA_SQL: &str = r#"
-- Context Reference 表
CREATE TABLE IF NOT EXISTS context_reference (
    id                  TEXT PRIMARY KEY NOT NULL,
    session_id          TEXT NOT NULL,
    reference_type      TEXT NOT NULL,
    locator             TEXT NOT NULL,
    snapshot            TEXT NOT NULL DEFAULT '',
    metadata            TEXT NOT NULL DEFAULT '{}',
    created_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_context_reference_session
    ON context_reference(session_id);

CREATE INDEX IF NOT EXISTS idx_context_reference_type
    ON context_reference(reference_type);
"#;
