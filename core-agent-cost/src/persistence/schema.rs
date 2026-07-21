pub const SCHEMA_SQL: &str = r#"
-- Append-only cost record store
CREATE TABLE IF NOT EXISTS cost_record (
    id                      TEXT PRIMARY KEY NOT NULL,
    tenant_id               TEXT NOT NULL,
    organization_id         TEXT,
    project_id              TEXT,
    agent_id                TEXT,
    session_id              TEXT,
    model_key               TEXT,
    input_tokens            INTEGER NOT NULL,
    output_tokens           INTEGER NOT NULL,
    price_per_token_micros  INTEGER NOT NULL,
    amount_micros           INTEGER NOT NULL,
    currency                TEXT NOT NULL,
    event_key               TEXT NOT NULL,
    actor                   TEXT NOT NULL,
    occurred_at             TEXT NOT NULL,
    version                 INTEGER NOT NULL,
    content                 TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    create_time             TEXT NOT NULL DEFAULT '',
    update_time             TEXT NOT NULL DEFAULT '',
    create_user             TEXT NOT NULL DEFAULT 'system',
    update_user             TEXT NOT NULL DEFAULT 'system'
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_cost_event_key ON cost_record(event_key);
CREATE INDEX IF NOT EXISTS idx_cost_tenant ON cost_record(tenant_id, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_cost_agent ON cost_record(agent_id, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_cost_model ON cost_record(model_key, occurred_at DESC, id);

-- Budget configuration
CREATE TABLE IF NOT EXISTS budget (
    id                      TEXT PRIMARY KEY NOT NULL,
    tenant_id               TEXT NOT NULL,
    scope                   TEXT NOT NULL,
    scope_id                TEXT NOT NULL,
    monthly_limit_micros    INTEGER NOT NULL,
    monthly_used_micros     INTEGER NOT NULL,
    currency                TEXT NOT NULL,
    alert_threshold         INTEGER NOT NULL,
    state                   TEXT NOT NULL,
    version                 INTEGER NOT NULL,
    content                 TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    create_time             TEXT NOT NULL DEFAULT '',
    update_time             TEXT NOT NULL DEFAULT '',
    create_user             TEXT NOT NULL DEFAULT 'system',
    update_user             TEXT NOT NULL DEFAULT 'system',
    UNIQUE(scope, scope_id)
);
CREATE INDEX IF NOT EXISTS idx_budget_tenant ON budget(tenant_id, scope, state, id);
"#;