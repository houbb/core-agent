//! SQLite Schema 定义 — Context Snapshot
//!
//! context_snapshot 表：存储每次 build() 的完整 Context JSON 快照。

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
    build_duration_ms   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_session
    ON context_snapshot(session_id);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_created
    ON context_snapshot(session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_context_snapshot_hash
    ON context_snapshot(hash);
"#;