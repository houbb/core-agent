pub const SCHEMA_SQL: &str = r#"
-- Approval request store
CREATE TABLE IF NOT EXISTS approval_request (
    id                  TEXT PRIMARY KEY NOT NULL,
    tenant_id           TEXT NOT NULL,
    organization_id     TEXT,
    request_type        TEXT NOT NULL,
    requester           TEXT NOT NULL,
    action              TEXT NOT NULL,
    resource            TEXT NOT NULL,
    risk_level          TEXT NOT NULL,
    state               TEXT NOT NULL,
    required_approvals  INTEGER NOT NULL,
    expires_at          TEXT,
    version             INTEGER NOT NULL,
    content             TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_approval_tenant ON approval_request(tenant_id, state, updated_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_approval_requester ON approval_request(requester, state, updated_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_approval_pending ON approval_request(state, expires_at, id);

-- Risk rules for automatic risk level evaluation
CREATE TABLE IF NOT EXISTS risk_rule (
    id                  TEXT PRIMARY KEY NOT NULL,
    tenant_id           TEXT NOT NULL,
    action_pattern      TEXT NOT NULL,
    resource_pattern    TEXT NOT NULL,
    risk_level          TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    version             INTEGER NOT NULL,
    content             TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    create_time         TEXT NOT NULL DEFAULT '',
    update_time         TEXT NOT NULL DEFAULT '',
    create_user         TEXT NOT NULL DEFAULT 'system',
    update_user         TEXT NOT NULL DEFAULT 'system'
);
CREATE INDEX IF NOT EXISTS idx_risk_rule_tenant ON risk_rule(tenant_id, enabled, id);
"#;