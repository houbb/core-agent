//! P6 Semantic schema. Foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Semantic entities
CREATE TABLE IF NOT EXISTS semantic_entity (
    id                  TEXT PRIMARY KEY NOT NULL,
    name                TEXT NOT NULL,
    entity_type         TEXT NOT NULL,
    attributes_json     TEXT NOT NULL DEFAULT '{}',
    source_document_id  TEXT,
    actor               TEXT NOT NULL DEFAULT 'system',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_semantic_entity_name ON semantic_entity(name);
CREATE INDEX IF NOT EXISTS idx_semantic_entity_type ON semantic_entity(entity_type);

-- Semantic relations
CREATE TABLE IF NOT EXISTS semantic_relation (
    id                  TEXT PRIMARY KEY NOT NULL,
    source_entity_id    TEXT NOT NULL,
    target_entity_id    TEXT NOT NULL,
    relation_type       TEXT NOT NULL,
    confidence          REAL NOT NULL DEFAULT 0.8,
    attributes_json     TEXT NOT NULL DEFAULT '{}',
    source_document_id  TEXT,
    actor               TEXT NOT NULL DEFAULT 'system',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_semantic_relation_source ON semantic_relation(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_semantic_relation_target ON semantic_relation(target_entity_id);
CREATE INDEX IF NOT EXISTS idx_semantic_relation_type ON semantic_relation(relation_type);
"#;