pub const SCHEMA_SQL: &str = r#"
-- Learning record store. Each record is a discovered experience/improvement.
CREATE TABLE IF NOT EXISTS learning_record (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL,
    source          TEXT NOT NULL,
    learning_type   TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'CANDIDATE',
    title           TEXT NOT NULL,
    description     TEXT NOT NULL,
    experience      TEXT NOT NULL DEFAULT '{}',
    improvement     TEXT NOT NULL DEFAULT '{}',
    confidence      REAL NOT NULL DEFAULT 0.0,
    source_id       TEXT,
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

CREATE INDEX IF NOT EXISTS idx_learn_agent ON learning_record(agent_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_learn_type ON learning_record(learning_type, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_learn_status ON learning_record(status, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_learn_source ON learning_record(source, created_at DESC, id);
"#;