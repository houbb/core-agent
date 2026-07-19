//! P4 schema. Relations are logical, indexed and deliberately have no foreign keys.

pub const SCHEMA_SQL: &str = r#"
-- Workspace aggregate root. `content` stores base metadata and the Workspace Graph.
CREATE TABLE IF NOT EXISTS workspace (
    id              TEXT PRIMARY KEY NOT NULL, -- UUID identity
    name            TEXT NOT NULL,             -- Display name
    provider_key    TEXT NOT NULL,             -- Logical provider identity
    uri             TEXT NOT NULL UNIQUE,      -- Canonical credential-free URI
    state           TEXT NOT NULL,             -- CREATED/LOADED/READY/MODIFIED/SNAPSHOT/CLOSED
    metadata        TEXT NOT NULL DEFAULT '{}',-- Non-secret JSON metadata
    content         TEXT NOT NULL,             -- Strict serialized aggregate base
    created_at      TEXT NOT NULL,             -- Domain creation time
    updated_at      TEXT NOT NULL,             -- Domain update time
    create_time     TEXT NOT NULL DEFAULT '',  -- Audit creation time
    update_time     TEXT NOT NULL DEFAULT '',  -- Audit update time
    create_user     TEXT NOT NULL DEFAULT 'system', -- Audit creator
    update_user     TEXT NOT NULL DEFAULT 'system'  -- Audit updater
);

CREATE INDEX IF NOT EXISTS idx_workspace_state ON workspace(state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_workspace_provider ON workspace(provider_key, name);
CREATE INDEX IF NOT EXISTS idx_workspace_updated ON workspace(updated_at DESC);

-- Projects discovered inside a Workspace. No SQLite foreign key is used.
CREATE TABLE IF NOT EXISTS project (
    id              TEXT PRIMARY KEY NOT NULL, -- Deterministic UUID
    workspace_id    TEXT NOT NULL,             -- Logical Workspace reference
    name            TEXT NOT NULL,
    project_kind    TEXT NOT NULL,             -- RUST/MAVEN/GRADLE/NODE/PYTHON/GENERIC
    root_uri        TEXT NOT NULL,             -- Canonical project root URI
    module_count    INTEGER NOT NULL DEFAULT 1,
    markers         TEXT NOT NULL DEFAULT '[]',-- Detection markers
    metadata        TEXT NOT NULL DEFAULT '{}',
    content         TEXT NOT NULL,             -- Strict serialized Project
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_project_workspace_uri
    ON project(workspace_id, root_uri);
CREATE INDEX IF NOT EXISTS idx_project_workspace_kind
    ON project(workspace_id, project_kind, name);

-- Unified files, directories and future non-file Resources.
CREATE TABLE IF NOT EXISTS resource (
    id              TEXT PRIMARY KEY NOT NULL, -- Deterministic UUID
    workspace_id    TEXT NOT NULL,             -- Logical Workspace reference
    project_id      TEXT,                      -- Optional logical Project reference
    resource_type   TEXT NOT NULL,             -- FILE/DIRECTORY/IMAGE/PDF/...
    uri             TEXT NOT NULL,             -- Canonical Resource URI
    name            TEXT NOT NULL,
    size_bytes      INTEGER,
    capabilities    TEXT NOT NULL DEFAULT '[]',-- READ/WRITE/SEARCH/...
    provider_key    TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    content         TEXT NOT NULL,             -- Strict serialized Resource metadata
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_resource_workspace_uri
    ON resource(workspace_id, uri);
CREATE INDEX IF NOT EXISTS idx_resource_workspace_type
    ON resource(workspace_id, resource_type, name);
CREATE INDEX IF NOT EXISTS idx_resource_project
    ON resource(project_id, resource_type, name);

-- One current detected Environment per Workspace. Variable values are never stored.
CREATE TABLE IF NOT EXISTS environment (
    id                  TEXT PRIMARY KEY NOT NULL,
    workspace_id        TEXT NOT NULL UNIQUE,
    os                  TEXT NOT NULL,
    shell               TEXT,
    git                 TEXT,
    languages           TEXT NOT NULL DEFAULT '[]',
    runtimes            TEXT NOT NULL DEFAULT '[]',
    package_managers    TEXT NOT NULL DEFAULT '[]',
    variable_names      TEXT NOT NULL DEFAULT '[]', -- Names only, never values
    metadata            TEXT NOT NULL DEFAULT '{}',
    content             TEXT NOT NULL,             -- Strict serialized Environment
    detected_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_environment_os ON environment(os, detected_at DESC);
CREATE INDEX IF NOT EXISTS idx_environment_detected ON environment(detected_at DESC);

-- Snapshot metadata only. File bodies live in the replaceable Snapshot provider.
CREATE TABLE IF NOT EXISTS workspace_snapshot (
    id              TEXT PRIMARY KEY NOT NULL,
    workspace_id    TEXT NOT NULL,
    label           TEXT NOT NULL,
    storage_uri     TEXT NOT NULL,             -- Credential-free provider URI
    resource_count  INTEGER NOT NULL DEFAULT 0,
    total_bytes     INTEGER NOT NULL DEFAULT 0,
    metadata        TEXT NOT NULL DEFAULT '{}',
    content         TEXT NOT NULL,             -- Strict serialized Snapshot
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_workspace_snapshot_workspace
    ON workspace_snapshot(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_workspace_snapshot_created
    ON workspace_snapshot(created_at DESC);
"#;
