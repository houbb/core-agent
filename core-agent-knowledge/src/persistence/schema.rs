//! P6 Knowledge schema. Foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Knowledge items
CREATE TABLE IF NOT EXISTS knowledge_item (
    id              TEXT PRIMARY KEY NOT NULL,
    kind            TEXT NOT NULL,
    title           TEXT NOT NULL,
    content         TEXT NOT NULL,
    source          TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.8,
    owner           TEXT NOT NULL,
    tags            TEXT NOT NULL DEFAULT '[]',
    version         INTEGER NOT NULL DEFAULT 1,
    status          TEXT NOT NULL DEFAULT 'CREATED',
    document_id     TEXT,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    actor           TEXT NOT NULL DEFAULT 'system',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_knowledge_status ON knowledge_item(status);
CREATE INDEX IF NOT EXISTS idx_knowledge_kind ON knowledge_item(kind);
CREATE INDEX IF NOT EXISTS idx_knowledge_owner ON knowledge_item(owner);

-- Knowledge categories (hierarchical tree)
CREATE TABLE IF NOT EXISTS knowledge_category (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    parent_id       TEXT,
    description     TEXT NOT NULL DEFAULT '',
    actor           TEXT NOT NULL DEFAULT 'system',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_knowledge_category_parent ON knowledge_category(parent_id);
"#;