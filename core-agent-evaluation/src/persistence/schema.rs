pub const SCHEMA_SQL: &str = r#"
-- Evaluation storage. Each evaluation is immutable once created.
CREATE TABLE IF NOT EXISTS evaluation (
    id              TEXT PRIMARY KEY NOT NULL,
    agent_id        TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    execution_id    TEXT NOT NULL,
    criteria        TEXT NOT NULL DEFAULT '[]',
    feedback        TEXT NOT NULL DEFAULT '[]',
    total_score     INTEGER NOT NULL DEFAULT 0,
    passed          INTEGER NOT NULL DEFAULT 0,
    metadata        TEXT NOT NULL DEFAULT '{}',
    evaluator       TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_eval_agent ON evaluation(agent_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_eval_task ON evaluation(task_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_eval_passed ON evaluation(passed, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_eval_evaluator ON evaluation(evaluator, created_at DESC, id);
"#;