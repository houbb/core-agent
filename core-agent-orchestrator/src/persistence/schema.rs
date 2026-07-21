//! P2 orchestration schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS orchestration (
    id TEXT PRIMARY KEY NOT NULL,
    goal TEXT NOT NULL,
    supervisor_agent_id TEXT NOT NULL,
    strategy TEXT NOT NULL,
    status TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL DEFAULT '',
    update_time TEXT NOT NULL DEFAULT '',
    create_user TEXT NOT NULL DEFAULT 'system',
    update_user TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_orchestration_supervisor ON orchestration(supervisor_agent_id, status, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_orchestration_status ON orchestration(status, updated_at DESC, id);
"#;