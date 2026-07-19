pub const SCHEMA_SQL: &str = r#"
-- Tenant isolation root.
CREATE TABLE IF NOT EXISTS tenant(id TEXT PRIMARY KEY NOT NULL,tenant_key TEXT NOT NULL UNIQUE,name TEXT NOT NULL,state TEXT NOT NULL,version INTEGER NOT NULL CHECK(version>0),content TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,create_time TEXT NOT NULL,update_time TEXT NOT NULL,create_user TEXT NOT NULL,update_user TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_tenant_state ON tenant(state,updated_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_tenant_key ON tenant(tenant_key,id);
-- Tenant-owned organization directory.
CREATE TABLE IF NOT EXISTS organization(id TEXT PRIMARY KEY NOT NULL,tenant_id TEXT NOT NULL,parent_id TEXT,organization_key TEXT NOT NULL,name TEXT NOT NULL,version INTEGER NOT NULL CHECK(version>0),content TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,create_time TEXT NOT NULL,update_time TEXT NOT NULL,create_user TEXT NOT NULL,update_user TEXT NOT NULL,UNIQUE(tenant_id,organization_key));
CREATE INDEX IF NOT EXISTS idx_platform_org_tenant ON organization(tenant_id,parent_id,organization_key,id);
-- Deterministic tenant Policy rules.
CREATE TABLE IF NOT EXISTS policy(id TEXT PRIMARY KEY NOT NULL,tenant_id TEXT NOT NULL,organization_id TEXT,policy_key TEXT NOT NULL,enabled INTEGER NOT NULL CHECK(enabled IN(0,1)),version INTEGER NOT NULL CHECK(version>0),content TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,create_time TEXT NOT NULL,update_time TEXT NOT NULL,create_user TEXT NOT NULL,update_user TEXT NOT NULL,UNIQUE(tenant_id,policy_key));
CREATE INDEX IF NOT EXISTS idx_policy_scope ON policy(tenant_id,organization_id,enabled,policy_key,id);
-- Immutable governance Audit records.
CREATE TABLE IF NOT EXISTS audit(id TEXT PRIMARY KEY NOT NULL,request_id TEXT NOT NULL,tenant_id TEXT NOT NULL,organization_id TEXT,subject TEXT NOT NULL,action TEXT NOT NULL,resource TEXT NOT NULL,decision TEXT NOT NULL,content TEXT NOT NULL,created_at TEXT NOT NULL,create_time TEXT NOT NULL,update_time TEXT NOT NULL,create_user TEXT NOT NULL,update_user TEXT NOT NULL,UNIQUE(tenant_id,request_id));
CREATE INDEX IF NOT EXISTS idx_audit_tenant_time ON audit(tenant_id,created_at DESC,id);
CREATE INDEX IF NOT EXISTS idx_audit_decision ON audit(tenant_id,decision,action,created_at DESC);
-- Atomic bounded Quota windows and idempotency ledger.
CREATE TABLE IF NOT EXISTS quota(id TEXT PRIMARY KEY NOT NULL,tenant_id TEXT NOT NULL,organization_id TEXT,quota_key TEXT NOT NULL,quota_limit INTEGER NOT NULL,consumed INTEGER NOT NULL,window_ends_at TEXT NOT NULL,version INTEGER NOT NULL CHECK(version>0),content TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,create_time TEXT NOT NULL,update_time TEXT NOT NULL,create_user TEXT NOT NULL,update_user TEXT NOT NULL,UNIQUE(tenant_id,organization_id,quota_key));
CREATE INDEX IF NOT EXISTS idx_quota_scope ON quota(tenant_id,organization_id,quota_key,id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_quota_scope_key ON quota(tenant_id,COALESCE(organization_id,''),quota_key);
CREATE INDEX IF NOT EXISTS idx_quota_window ON quota(window_ends_at,tenant_id,id);
"#;
