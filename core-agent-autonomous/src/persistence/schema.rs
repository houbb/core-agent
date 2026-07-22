pub const SCHEMA_SQL: &str = r#"
-- Autonomous goals
CREATE TABLE IF NOT EXISTS autonomous_goal (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL,
    description     TEXT NOT NULL,
    priority        INTEGER NOT NULL DEFAULT 5,
    constraints     TEXT NOT NULL DEFAULT '{}',
    deadline        TEXT,
    autonomy_level  TEXT NOT NULL,
    active          INTEGER NOT NULL DEFAULT 1,
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

-- Autonomous loop state (one per agent)
CREATE TABLE IF NOT EXISTS autonomous_loop (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL DEFAULT 'IDLE',
    current_cycle   INTEGER NOT NULL DEFAULT 0,
    last_trigger    TEXT,
    last_trigger_at TEXT,
    autonomy_level  TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_auto_goal_agent ON autonomous_goal(agent_id, priority DESC, id);
CREATE INDEX IF NOT EXISTS idx_auto_goal_level ON autonomous_goal(autonomy_level, id);
CREATE INDEX IF NOT EXISTS idx_auto_loop_status ON autonomous_loop(status, id);
"#;