//! P6 Document schema. Relations are logical and indexed; foreign keys are forbidden.

pub const SCHEMA_SQL: &str = r#"
-- Core document aggregate
CREATE TABLE IF NOT EXISTS document (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    doc_type        TEXT NOT NULL,              -- MARKDOWN, TXT, CODE, PDF, DOCX, HTML
    source          TEXT NOT NULL,              -- MANUAL, FILE_UPLOAD, URL, API, GIT, AGENT
    status          TEXT NOT NULL,              -- UPLOADED, PARSING, ..., EMBEDDED, FAILED
    content         TEXT NOT NULL,
    chunk_count     INTEGER NOT NULL DEFAULT 0,
    embedding_status TEXT NOT NULL DEFAULT 'PENDING',
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    actor           TEXT NOT NULL DEFAULT 'system',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_document_status ON document(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_document_type ON document(doc_type, updated_at DESC);

-- Document chunks produced by the splitter
CREATE TABLE IF NOT EXISTS document_chunk (
    id              TEXT PRIMARY KEY NOT NULL,
    document_id     TEXT NOT NULL,
    chunk_index     INTEGER NOT NULL,
    content         TEXT NOT NULL,
    metadata_json   TEXT NOT NULL DEFAULT '{}',
    token_count     INTEGER NOT NULL DEFAULT 0,
    hash            TEXT NOT NULL,
    actor           TEXT NOT NULL DEFAULT 'system',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_document_chunk_doc ON document_chunk(document_id, chunk_index);
CREATE INDEX IF NOT EXISTS idx_document_chunk_hash ON document_chunk(hash);
"#;