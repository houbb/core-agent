pub const SCHEMA_SQL: &str = r#"
-- Append-only audit event store. No UPDATE or DELETE operations.
-- Relationships are logical; foreign keys are forbidden.
CREATE TABLE IF NOT EXISTS audit_event (
    id              TEXT PRIMARY KEY NOT NULL,
    tenant_id       TEXT NOT NULL,
    actor           TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    action          TEXT NOT NULL,
    resource        TEXT NOT NULL,
    payload         TEXT NOT NULL DEFAULT 'null',
    severity        TEXT NOT NULL DEFAULT 'INFO',
    result          TEXT NOT NULL DEFAULT 'success',
    request_id      TEXT,
    session_id      TEXT,
    trace_id        TEXT,
    client_ip       TEXT,
    user_agent      TEXT,
    occurred_at     TEXT NOT NULL,
    version         INTEGER NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    create_time     TEXT NOT NULL DEFAULT '',
    update_time     TEXT NOT NULL DEFAULT '',
    create_user     TEXT NOT NULL DEFAULT 'system',
    update_user     TEXT NOT NULL DEFAULT 'system'
);

CREATE INDEX IF NOT EXISTS idx_audit_tenant ON audit_event(tenant_id, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_event(actor, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_audit_type ON audit_event(event_type, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_audit_severity ON audit_event(severity, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_event(action, occurred_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_audit_request ON audit_event(request_id, id);
CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_event(session_id, occurred_at DESC, id);
"#;