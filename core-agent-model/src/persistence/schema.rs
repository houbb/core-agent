//! P2 schema. Relations are indexed but deliberately have no foreign keys.

pub const SCHEMA_SQL: &str = r#"
-- Provider metadata. API keys and other credentials are never stored here.
CREATE TABLE IF NOT EXISTS model_provider (
    id                      TEXT PRIMARY KEY NOT NULL,
    provider_key            TEXT NOT NULL UNIQUE,
    name                    TEXT NOT NULL,
    endpoint                TEXT,
    enabled                 INTEGER NOT NULL DEFAULT 1,
    timeout_ms              INTEGER NOT NULL,
    max_retries             INTEGER NOT NULL,
    rate_limit_per_minute   INTEGER,
    metadata                TEXT NOT NULL DEFAULT '{}',
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    create_time             TEXT NOT NULL DEFAULT '',
    update_time             TEXT NOT NULL DEFAULT '',
    create_user             TEXT NOT NULL DEFAULT 'system',
    update_user             TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_model_provider_enabled
    ON model_provider(enabled, provider_key);

-- Model Profile catalog. provider_key is an indexed logical reference, not an FK.
CREATE TABLE IF NOT EXISTS model (
    id                      TEXT PRIMARY KEY NOT NULL,
    profile_key             TEXT NOT NULL UNIQUE,
    provider_key            TEXT NOT NULL,
    model_name              TEXT NOT NULL,
    capabilities            TEXT NOT NULL DEFAULT '[]',
    limits                  TEXT NOT NULL DEFAULT '{}',
    pricing                 TEXT NOT NULL DEFAULT '{}',
    performance             TEXT NOT NULL DEFAULT '{}',
    policies                TEXT NOT NULL DEFAULT '{}',
    metadata                TEXT NOT NULL DEFAULT '{}',
    priority                INTEGER NOT NULL DEFAULT 0,
    enabled                 INTEGER NOT NULL DEFAULT 1,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    create_time             TEXT NOT NULL DEFAULT '',
    update_time             TEXT NOT NULL DEFAULT '',
    create_user             TEXT NOT NULL DEFAULT 'system',
    update_user             TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_model_provider ON model(provider_key, enabled);
CREATE INDEX IF NOT EXISTS idx_model_priority ON model(enabled, priority DESC, profile_key);

-- Content-free inference usage/audit records.
CREATE TABLE IF NOT EXISTS model_usage (
    id                      TEXT PRIMARY KEY NOT NULL,
    request_id              TEXT NOT NULL,
    operation               TEXT NOT NULL,
    provider_key            TEXT NOT NULL,
    model_name              TEXT NOT NULL,
    profile_key             TEXT NOT NULL,
    prompt_tokens           INTEGER NOT NULL DEFAULT 0,
    completion_tokens       INTEGER NOT NULL DEFAULT 0,
    cache_tokens            INTEGER NOT NULL DEFAULT 0,
    total_tokens            INTEGER NOT NULL DEFAULT 0,
    latency_ms              INTEGER NOT NULL DEFAULT 0,
    cost                    REAL,
    success                 INTEGER NOT NULL,
    error_kind              TEXT,
    metadata                TEXT NOT NULL DEFAULT '{}',
    created_at              TEXT NOT NULL,
    create_time             TEXT NOT NULL DEFAULT '',
    update_time             TEXT NOT NULL DEFAULT '',
    create_user             TEXT NOT NULL DEFAULT 'system',
    update_user             TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_model_usage_request ON model_usage(request_id);
CREATE INDEX IF NOT EXISTS idx_model_usage_created ON model_usage(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_model_usage_profile ON model_usage(profile_key, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_model_usage_success ON model_usage(success, created_at DESC);
"#;
