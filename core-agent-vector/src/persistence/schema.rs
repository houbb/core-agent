//! P6 Vector schema. Embedding stored as BLOB; FTS5 for keyword search.
//! Foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Vector records with embeddings
CREATE TABLE IF NOT EXISTS vector_record (
    id              TEXT PRIMARY KEY NOT NULL,
    content         TEXT NOT NULL,
    embedding       BLOB NOT NULL,              -- f32 binary array
    dimension       INTEGER NOT NULL,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    source          TEXT NOT NULL,
    document_id     TEXT,
    chunk_id        TEXT,
    actor           TEXT NOT NULL DEFAULT 'system',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_vector_document ON vector_record(document_id);
CREATE INDEX IF NOT EXISTS idx_vector_source ON vector_record(source);
CREATE INDEX IF NOT EXISTS idx_vector_created ON vector_record(created_at DESC);

-- FTS5 virtual table for full-text keyword search on vector content
CREATE VIRTUAL TABLE IF NOT EXISTS vector_record_fts USING fts5(
    content, metadata_json, content=vector_record, content_rowid=rowid
);

-- Triggers to keep FTS index in sync
CREATE TRIGGER IF NOT EXISTS vector_fts_insert AFTER INSERT ON vector_record BEGIN
    INSERT INTO vector_record_fts(rowid, content, metadata_json)
    VALUES (new.rowid, new.content, new.metadata_json);
END;

CREATE TRIGGER IF NOT EXISTS vector_fts_delete AFTER DELETE ON vector_record BEGIN
    INSERT INTO vector_record_fts(vector_record_fts, rowid, content, metadata_json)
    VALUES ('delete', old.rowid, old.content, old.metadata_json);
END;

CREATE TRIGGER IF NOT EXISTS vector_fts_update AFTER UPDATE ON vector_record BEGIN
    INSERT INTO vector_record_fts(vector_record_fts, rowid, content, metadata_json)
    VALUES ('delete', old.rowid, old.content, old.metadata_json);
    INSERT INTO vector_record_fts(rowid, content, metadata_json)
    VALUES (new.rowid, new.content, new.metadata_json);
END;
"#;