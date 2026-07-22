pub const SCHEMA_SQL: &str = r#"
-- Agent network registry.
CREATE TABLE IF NOT EXISTS agent_registration (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    capabilities    TEXT NOT NULL DEFAULT '[]',
    status          TEXT NOT NULL DEFAULT 'OFFLINE',
    trust_level     TEXT NOT NULL DEFAULT 'MEDIUM',
    endpoint        TEXT,
    reputation      REAL NOT NULL DEFAULT 0.0,
    metadata        TEXT NOT NULL DEFAULT '{}',
    version         INTEGER NOT NULL,
    actor           TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_network_status ON agent_registration(status, reputation DESC, id);
CREATE INDEX IF NOT EXISTS idx_network_trust ON agent_registration(trust_level, id);
"#;