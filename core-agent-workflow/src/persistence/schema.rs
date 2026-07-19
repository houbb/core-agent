//! P10 schema. Relationships are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Stable Workflow registry identity and current immutable Definition version.
CREATE TABLE IF NOT EXISTS workflow (
    id                          TEXT PRIMARY KEY NOT NULL,
    workflow_key                TEXT NOT NULL UNIQUE,
    name                        TEXT NOT NULL,
    current_definition_id       TEXT NOT NULL,
    current_definition_version  INTEGER NOT NULL,
    enabled                     INTEGER NOT NULL,
    version                     INTEGER NOT NULL,
    content                     TEXT NOT NULL,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    create_time                 TEXT NOT NULL DEFAULT '',
    update_time                 TEXT NOT NULL DEFAULT '',
    create_user                 TEXT NOT NULL DEFAULT 'system',
    update_user                 TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_workflow_enabled_key ON workflow(enabled, workflow_key, id);
CREATE INDEX IF NOT EXISTS idx_workflow_current_definition ON workflow(current_definition_id, current_definition_version);

-- Immutable, strongly typed Workflow -> Stage -> Activity -> Action Definition version.
CREATE TABLE IF NOT EXISTS workflow_definition (
    id                  TEXT PRIMARY KEY NOT NULL,
    workflow_id         TEXT NOT NULL,
    definition_version  INTEGER NOT NULL,
    definition_key      TEXT NOT NULL,
    name                TEXT NOT NULL,
    content             TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system',
    UNIQUE(workflow_id, definition_version)
);
CREATE INDEX IF NOT EXISTS idx_workflow_definition_lookup ON workflow_definition(workflow_id, definition_version DESC, id);
CREATE INDEX IF NOT EXISTS idx_workflow_definition_key ON workflow_definition(definition_key, definition_version DESC);

-- Durable Workflow Instance aggregate, including Definition snapshot and hierarchical progress.
CREATE TABLE IF NOT EXISTS workflow_instance (
    id                  TEXT PRIMARY KEY NOT NULL,
    workflow_id         TEXT NOT NULL,
    definition_id       TEXT NOT NULL,
    definition_version  INTEGER NOT NULL,
    state               TEXT NOT NULL,
    current_stage_id    TEXT,
    current_activity_id TEXT,
    current_action_id   TEXT,
    version             INTEGER NOT NULL,
    content             TEXT NOT NULL,
    started_at          TEXT,
    completed_at        TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_workflow_instance_workflow ON workflow_instance(workflow_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_workflow_instance_state ON workflow_instance(state, updated_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_workflow_instance_current ON workflow_instance(current_stage_id, current_activity_id, current_action_id);

-- Point-in-time Workflow Instance aggregate for current-version restore.
CREATE TABLE IF NOT EXISTS workflow_snapshot (
    id              TEXT PRIMARY KEY NOT NULL,
    instance_id     TEXT NOT NULL,
    sequence        INTEGER NOT NULL,
    label           TEXT NOT NULL,
    hash            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_workflow_snapshot_instance ON workflow_snapshot(instance_id, sequence DESC, id);

-- Append-only Workflow lifecycle timeline.
CREATE TABLE IF NOT EXISTS workflow_state (
    id              TEXT PRIMARY KEY NOT NULL,
    instance_id     TEXT NOT NULL,
    sequence        INTEGER NOT NULL,
    from_state      TEXT,
    to_state        TEXT NOT NULL,
    reason          TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system',
    UNIQUE(instance_id, sequence)
);
CREATE INDEX IF NOT EXISTS idx_workflow_state_timeline ON workflow_state(instance_id, sequence, id);
CREATE INDEX IF NOT EXISTS idx_workflow_state_target ON workflow_state(to_state, created_at DESC);
"#;
