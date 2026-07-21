//! P2 message schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_message (
    id TEXT PRIMARY KEY NOT NULL,
    from_agent_id TEXT NOT NULL,
    to_agent_id TEXT NOT NULL,
    correlation_id TEXT,
    message_type TEXT NOT NULL,
    intent TEXT NOT NULL,
    priority TEXT NOT NULL,
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
CREATE INDEX IF NOT EXISTS idx_agent_message_to ON agent_message(to_agent_id, status, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_agent_message_from ON agent_message(from_agent_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_agent_message_correlation ON agent_message(correlation_id, id);
CREATE INDEX IF NOT EXISTS idx_agent_message_status ON agent_message(status, priority, created_at DESC, id);
"#;