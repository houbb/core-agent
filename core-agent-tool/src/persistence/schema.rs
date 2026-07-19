//! P3 schema. Logical relations are indexed and deliberately have no foreign keys.

pub const SCHEMA_SQL: &str = r#"
-- Tool Provider catalog. Runtime instances and credentials are never stored here.
CREATE TABLE IF NOT EXISTS tool_provider (
    id                  TEXT PRIMARY KEY NOT NULL, -- UUID
    provider_key        TEXT NOT NULL UNIQUE,      -- Stable provider identity
    name                TEXT NOT NULL,             -- Display name
    provider_kind       TEXT NOT NULL,             -- BUILTIN/MCP/PLUGIN/...
    enabled             INTEGER NOT NULL DEFAULT 1,-- Boolean 0/1
    metadata            TEXT NOT NULL DEFAULT '{}',-- Non-secret JSON metadata
    created_at          TEXT NOT NULL,             -- Domain creation time
    updated_at          TEXT NOT NULL,             -- Domain update time
    create_time         TEXT NOT NULL DEFAULT '',  -- Audit creation time
    update_time         TEXT NOT NULL DEFAULT '',  -- Audit update time
    create_user         TEXT NOT NULL DEFAULT 'system', -- Audit creator
    update_user         TEXT NOT NULL DEFAULT 'system'  -- Audit updater
);

CREATE INDEX IF NOT EXISTS idx_tool_provider_enabled
    ON tool_provider(enabled, provider_key);

-- Durable Tool metadata; executable instances remain in ToolRegistry.
CREATE TABLE IF NOT EXISTS tool (
    id                  TEXT PRIMARY KEY NOT NULL, -- UUID
    tool_key            TEXT NOT NULL UNIQUE,      -- provider/name@version
    provider_key        TEXT NOT NULL,             -- Logical provider reference
    name                TEXT NOT NULL,             -- Stable tool name
    description         TEXT NOT NULL DEFAULT '',  -- Human-readable description
    input_schema        TEXT NOT NULL,             -- JSON Schema
    version             TEXT NOT NULL,             -- Tool version
    category            TEXT NOT NULL,             -- Catalog category
    icon                TEXT,                      -- Optional icon reference
    tags                TEXT NOT NULL DEFAULT '[]',-- JSON string set
    capabilities        TEXT NOT NULL DEFAULT '[]',-- JSON capability set
    default_permission  TEXT NOT NULL DEFAULT 'ASK',-- ALLOW/ASK/DENY
    timeout_ms          INTEGER NOT NULL,           -- Maximum execution time
    enabled             INTEGER NOT NULL DEFAULT 1, -- Boolean 0/1
    metadata            TEXT NOT NULL DEFAULT '{}', -- Non-secret JSON metadata
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_tool_provider
    ON tool(provider_key, enabled, tool_key);
CREATE INDEX IF NOT EXISTS idx_tool_category
    ON tool(enabled, category, tool_key);
CREATE INDEX IF NOT EXISTS idx_tool_name_version
    ON tool(name, version, enabled);

-- Content-free execution audit. Parameters, output and attachment bodies are excluded.
CREATE TABLE IF NOT EXISTS tool_execution (
    id                  TEXT PRIMARY KEY NOT NULL, -- UUID
    request_id          TEXT NOT NULL UNIQUE,      -- Invocation correlation ID
    tool_key            TEXT NOT NULL,             -- Logical Tool reference
    provider_key        TEXT NOT NULL,             -- Actual Provider attribution
    session_id          TEXT,                      -- Opaque correlation UUID
    subject             TEXT,                      -- Opaque permission subject
    status              TEXT NOT NULL,             -- Lifecycle terminal/intermediate state
    latency_ms          INTEGER NOT NULL DEFAULT 0,-- Duration without payload
    error_kind          TEXT,                      -- Stable content-free error category
    metadata            TEXT NOT NULL DEFAULT '{}',-- Allowlisted correlation metadata
    started_at          TEXT,
    completed_at        TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_tool_execution_tool
    ON tool_execution(tool_key, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_execution_status
    ON tool_execution(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_execution_session
    ON tool_execution(session_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_tool_execution_created
    ON tool_execution(created_at DESC);

-- P3 local Tool permission rules; no RBAC or approval workflow is embedded.
CREATE TABLE IF NOT EXISTS tool_permission (
    id                  TEXT PRIMARY KEY NOT NULL, -- UUID
    tool_key            TEXT,                      -- Optional exact Tool scope
    capability          TEXT,                      -- Optional capability subtree
    subject             TEXT,                      -- Optional exact subject
    decision            TEXT NOT NULL,             -- ALLOW/ASK/DENY
    priority            INTEGER NOT NULL DEFAULT 0,-- Deterministic rule precedence
    enabled             INTEGER NOT NULL DEFAULT 1,-- Boolean 0/1
    metadata            TEXT NOT NULL DEFAULT '{}',-- Non-secret JSON metadata
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_tool_permission_tool
    ON tool_permission(tool_key, subject, enabled, priority DESC);
CREATE INDEX IF NOT EXISTS idx_tool_permission_capability
    ON tool_permission(capability, subject, enabled, priority DESC);
CREATE INDEX IF NOT EXISTS idx_tool_permission_decision
    ON tool_permission(decision, enabled, priority DESC);
"#;
