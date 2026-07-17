//! SQLite Schema 定义
//!
//! 五张表：session / conversation / message / attachment / manifest
//! 每个表必须有 id / create_time / update_time + 合适索引。禁止外键。

/// 建表 DDL
pub const SCHEMA_SQL: &str = r#"
-- Session 表
CREATE TABLE IF NOT EXISTS session (
    id              TEXT PRIMARY KEY NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT,
    state           TEXT NOT NULL DEFAULT 'CREATED',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    last_active_at  TEXT NOT NULL,
    owner           TEXT,
    workspace_id    TEXT,
    metadata        TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_session_state ON session(state);
CREATE INDEX IF NOT EXISTS idx_session_owner ON session(owner);
CREATE INDEX IF NOT EXISTS idx_session_last_active ON session(last_active_at DESC);

-- Conversation 表
CREATE TABLE IF NOT EXISTS conversation (
    id                  TEXT PRIMARY KEY NOT NULL,
    session_id          TEXT NOT NULL,
    conversation_type   TEXT NOT NULL DEFAULT 'MAIN',
    name                TEXT,
    created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversation_session ON conversation(session_id);

-- Message 表
CREATE TABLE IF NOT EXISTS message (
    id                  TEXT PRIMARY KEY NOT NULL,
    conversation_id     TEXT NOT NULL,
    role                TEXT NOT NULL,
    content             TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'PENDING',
    created_at          TEXT NOT NULL,
    metadata            TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_message_conversation ON message(conversation_id);
CREATE INDEX IF NOT EXISTS idx_message_created ON message(conversation_id, created_at);

-- Attachment 表
CREATE TABLE IF NOT EXISTS attachment (
    id                  TEXT PRIMARY KEY NOT NULL,
    message_id          TEXT,
    session_id          TEXT,
    attachment_type     TEXT NOT NULL,
    name                TEXT NOT NULL,
    mime_type           TEXT,
    size_bytes          INTEGER,
    storage_path        TEXT,
    content             BLOB,
    created_at          TEXT NOT NULL,
    metadata            TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_attachment_message ON attachment(message_id);
CREATE INDEX IF NOT EXISTS idx_attachment_session ON attachment(session_id);

-- Manifest 表
CREATE TABLE IF NOT EXISTS manifest (
    id                      TEXT PRIMARY KEY NOT NULL,
    session_id              TEXT NOT NULL UNIQUE,
    name                    TEXT NOT NULL,
    model                   TEXT,
    workspace_path          TEXT,
    tags                    TEXT NOT NULL DEFAULT '[]',
    state                   TEXT NOT NULL,
    last_active_at          TEXT NOT NULL,
    conversation_count      INTEGER NOT NULL DEFAULT 0,
    message_count           INTEGER NOT NULL DEFAULT 0,
    token_count             INTEGER,
    last_conversation_id    TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_manifest_session ON manifest(session_id);
CREATE INDEX IF NOT EXISTS idx_manifest_state ON manifest(state);
CREATE INDEX IF NOT EXISTS idx_manifest_last_active ON manifest(last_active_at DESC);
"#;
