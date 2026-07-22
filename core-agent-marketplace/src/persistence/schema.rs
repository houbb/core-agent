pub const SCHEMA_SQL: &str = r#"
-- Marketplace package store.
CREATE TABLE IF NOT EXISTS marketplace_package (
    id              TEXT PRIMARY KEY NOT NULL,
    asset_type      TEXT NOT NULL,
    name            TEXT NOT NULL,
    key             TEXT NOT NULL,
    version         TEXT NOT NULL,
    author          TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    state           TEXT NOT NULL DEFAULT 'DRAFT',
    rating          REAL NOT NULL DEFAULT 0.0,
    downloads       INTEGER NOT NULL DEFAULT 0,
    tags            TEXT NOT NULL DEFAULT '[]',
    content         TEXT NOT NULL DEFAULT '{}',
    metadata        TEXT NOT NULL DEFAULT '{}',
    version_count   INTEGER NOT NULL DEFAULT 1,
    actor           TEXT NOT NULL,
    content_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_marketplace_key_version ON marketplace_package(key, version);
CREATE INDEX IF NOT EXISTS idx_marketplace_type ON marketplace_package(asset_type, rating DESC, id);
CREATE INDEX IF NOT EXISTS idx_marketplace_author ON marketplace_package(author, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_marketplace_state ON marketplace_package(state, created_at DESC, id);
"#;