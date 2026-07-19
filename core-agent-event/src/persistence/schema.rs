//! P9 schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Durable typed Event aggregate and per-subscription delivery evidence.
CREATE TABLE IF NOT EXISTS event (
    id              TEXT PRIMARY KEY NOT NULL,
    event_type      TEXT NOT NULL,
    category        TEXT NOT NULL,
    namespace       TEXT NOT NULL,
    source_kind     TEXT NOT NULL,
    target          TEXT,
    state           TEXT NOT NULL,
    priority        TEXT NOT NULL,
    visibility      TEXT NOT NULL,
    sensitive       INTEGER NOT NULL,
    schema_version  INTEGER NOT NULL,
    policy_id       TEXT,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    occurred_at     TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_event_namespace_time ON event(namespace, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_event_type_state ON event(namespace, event_type, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_source_target ON event(namespace, source_kind, target, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_policy ON event(policy_id, updated_at DESC);

-- Durable routing declaration; live handler code is bound process-locally.
CREATE TABLE IF NOT EXISTS event_subscription (
    id                TEXT PRIMARY KEY NOT NULL,
    subscription_key  TEXT NOT NULL UNIQUE,
    namespace         TEXT NOT NULL,
    priority          INTEGER NOT NULL,
    enabled           INTEGER NOT NULL,
    version           INTEGER NOT NULL,
    content           TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    create_time       TEXT NOT NULL DEFAULT '',
    update_time       TEXT NOT NULL DEFAULT '',
    create_user       TEXT NOT NULL DEFAULT 'system',
    update_user       TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_event_subscription_route ON event_subscription(namespace, enabled, priority DESC, subscription_key);

-- Explicit replay audit. Original archived Event rows are not rewritten.
CREATE TABLE IF NOT EXISTS event_replay (
    id              TEXT PRIMARY KEY NOT NULL,
    event_id        TEXT NOT NULL,
    state           TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_event_replay_event ON event_replay(event_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_event_replay_state ON event_replay(state, updated_at DESC);

-- Reusable publish/delivery/replay policy declarations.
CREATE TABLE IF NOT EXISTS event_policy (
    id              TEXT PRIMARY KEY NOT NULL,
    policy_key      TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_event_policy_name ON event_policy(name, updated_at DESC);

-- Exhausted local deliveries, including replay failures.
CREATE TABLE IF NOT EXISTS event_dead_letter (
    id                TEXT PRIMARY KEY NOT NULL,
    event_id          TEXT NOT NULL,
    subscription_id   TEXT NOT NULL,
    replay_id         TEXT,
    resolved          INTEGER NOT NULL,
    attempts          INTEGER NOT NULL,
    error             TEXT NOT NULL,
    version           INTEGER NOT NULL,
    content           TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    create_time       TEXT NOT NULL DEFAULT '',
    update_time       TEXT NOT NULL DEFAULT '',
    create_user       TEXT NOT NULL DEFAULT 'system',
    update_user       TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_event_dead_letter_event ON event_dead_letter(event_id, resolved, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_event_dead_letter_subscription ON event_dead_letter(subscription_id, replay_id, created_at DESC);
"#;
