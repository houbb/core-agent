pub const SCHEMA_SQL: &str = r#"
-- Installed Extension identity and current immutable Manifest revision.
CREATE TABLE IF NOT EXISTS extension (
    id TEXT PRIMARY KEY NOT NULL,
    extension_key TEXT NOT NULL UNIQUE,
    current_manifest_id TEXT NOT NULL,
    current_version TEXT NOT NULL,
    state TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_extension_key ON extension(extension_key, id);
CREATE INDEX IF NOT EXISTS idx_extension_state ON extension(state, updated_at DESC, id);

-- Immutable declarative Manifest revisions and verified local artifact identity.
CREATE TABLE IF NOT EXISTS extension_manifest (
    id TEXT PRIMARY KEY NOT NULL,
    extension_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    version_name TEXT NOT NULL,
    source_uri TEXT NOT NULL,
    checksum TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(extension_id, revision),
    UNIQUE(extension_id, version_name)
);
CREATE INDEX IF NOT EXISTS idx_extension_manifest_owner ON extension_manifest(extension_id, revision DESC, id);
CREATE INDEX IF NOT EXISTS idx_extension_manifest_checksum ON extension_manifest(checksum, id);

-- Durable Extension lifecycle timeline.
CREATE TABLE IF NOT EXISTS extension_state (
    id TEXT PRIMARY KEY NOT NULL,
    extension_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence > 0),
    from_state TEXT,
    to_state TEXT NOT NULL,
    reason TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(extension_id, sequence)
);
CREATE INDEX IF NOT EXISTS idx_extension_state_timeline ON extension_state(extension_id, sequence, id);
CREATE INDEX IF NOT EXISTS idx_extension_state_target ON extension_state(to_state, created_at DESC, id);

-- Capability declarations exported by one Manifest revision.
CREATE TABLE IF NOT EXISTS capability (
    id TEXT PRIMARY KEY NOT NULL,
    extension_id TEXT NOT NULL,
    manifest_id TEXT NOT NULL,
    capability_key TEXT NOT NULL,
    version_name TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(manifest_id, capability_key)
);
CREATE INDEX IF NOT EXISTS idx_capability_lookup ON capability(capability_key, enabled, extension_id, id);
CREATE INDEX IF NOT EXISTS idx_capability_manifest ON capability(manifest_id, capability_key, id);

-- Provider declarations able to serve one or more Capabilities.
CREATE TABLE IF NOT EXISTS provider (
    id TEXT PRIMARY KEY NOT NULL,
    extension_id TEXT NOT NULL,
    manifest_id TEXT NOT NULL,
    provider_key TEXT NOT NULL,
    provider_kind TEXT NOT NULL,
    priority INTEGER NOT NULL,
    enabled INTEGER NOT NULL CHECK(enabled IN (0,1)),
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(manifest_id, provider_key)
);
CREATE INDEX IF NOT EXISTS idx_provider_extension ON provider(extension_id, enabled, priority, id);
CREATE INDEX IF NOT EXISTS idx_provider_manifest ON provider(manifest_id, provider_key, id);
"#;
