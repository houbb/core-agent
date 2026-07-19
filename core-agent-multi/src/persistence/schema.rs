pub const SCHEMA_SQL: &str = r#"
-- Organization identity and governance root.
CREATE TABLE IF NOT EXISTS organization (
    id TEXT PRIMARY KEY NOT NULL,
    organization_key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_organization_key ON organization(organization_key, id);
CREATE INDEX IF NOT EXISTS idx_organization_updated ON organization(updated_at DESC, id);

-- Versioned Role declarations owned by one Organization.
CREATE TABLE IF NOT EXISTS role (
    id TEXT PRIMARY KEY NOT NULL,
    organization_id TEXT NOT NULL,
    role_key TEXT NOT NULL,
    name TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(organization_id, role_key)
);
CREATE INDEX IF NOT EXISTS idx_role_organization ON role(organization_id, role_key, id);
CREATE INDEX IF NOT EXISTS idx_role_updated ON role(updated_at DESC, id);

-- Durable Team aggregate and lifecycle.
CREATE TABLE IF NOT EXISTS team (
    id TEXT PRIMARY KEY NOT NULL,
    organization_id TEXT NOT NULL,
    team_key TEXT NOT NULL,
    state TEXT NOT NULL,
    workspace_id TEXT,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(organization_id, team_key)
);
CREATE INDEX IF NOT EXISTS idx_team_organization ON team(organization_id, team_key, id);
CREATE INDEX IF NOT EXISTS idx_team_state ON team(state, updated_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_team_workspace ON team(workspace_id, state, id);

-- Agent membership, Role binding and current collaboration ownership.
CREATE TABLE IF NOT EXISTS agent_member (
    id TEXT PRIMARY KEY NOT NULL,
    team_id TEXT NOT NULL,
    role_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    state TEXT NOT NULL,
    current_collaboration_id TEXT,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL,
    UNIQUE(team_id, agent_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_member_team ON agent_member(team_id, role_id, state, id);
CREATE INDEX IF NOT EXISTS idx_agent_member_agent ON agent_member(agent_id, state, id);
CREATE INDEX IF NOT EXISTS idx_agent_member_work ON agent_member(current_collaboration_id, id);

-- Collaboration assignment, protocol transcript and external Agent binding.
CREATE TABLE IF NOT EXISTS collaboration (
    id TEXT PRIMARY KEY NOT NULL,
    team_id TEXT NOT NULL,
    role_id TEXT,
    source_member_id TEXT,
    target_member_id TEXT NOT NULL,
    dispatch_id TEXT,
    state TEXT NOT NULL,
    priority TEXT NOT NULL,
    version INTEGER NOT NULL CHECK(version > 0),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    create_time TEXT NOT NULL,
    update_time TEXT NOT NULL,
    create_user TEXT NOT NULL,
    update_user TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_collaboration_dispatch ON collaboration(dispatch_id) WHERE dispatch_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_collaboration_team ON collaboration(team_id, created_at DESC, id);
CREATE INDEX IF NOT EXISTS idx_collaboration_target ON collaboration(target_member_id, state, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_collaboration_correlation ON collaboration(team_id, state, priority, id);
"#;
