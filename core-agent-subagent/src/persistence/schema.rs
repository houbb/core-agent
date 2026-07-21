//! P2 schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- AgentInstance registry with lifecycle tracking.
CREATE TABLE IF NOT EXISTS agent_instance (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    instance_type TEXT NOT NULL,
    role TEXT NOT NULL,
    parent_agent_id TEXT,
    supervisor_agent_id TEXT,
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
CREATE INDEX IF NOT EXISTS idx_agent_instance_parent ON agent_instance(parent_agent_id, status, id);
CREATE INDEX IF NOT EXISTS idx_agent_instance_supervisor ON agent_instance(supervisor_agent_id, status, id);
CREATE INDEX IF NOT EXISTS idx_agent_instance_status ON agent_instance(status, updated_at DESC, id);
"#;