use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Environment, Project, Resource, ResourceCapability, Snapshot, Workspace,
    WorkspaceMetadata,
};
use crate::error::{WorkspaceError, WorkspaceResult};
use crate::infrastructure::WorkspaceCatalog;

use super::schema::SCHEMA_SQL;

pub type SqlitePool = Pool<SqliteConnectionManager>;

pub struct SqliteWorkspaceStore {
    pool: SqlitePool,
}

impl SqliteWorkspaceStore {
    pub fn new(path: &str) -> WorkspaceResult<Self> {
        let manager = if path == ":memory:" {
            SqliteConnectionManager::memory()
        } else {
            SqliteConnectionManager::file(path)
        };
        let max_size = if path == ":memory:" { 1 } else { 8 };
        let pool = Pool::builder().max_size(max_size).build(manager)?;
        let store = Self { pool };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> WorkspaceResult<()> {
        let connection = self.pool.get()?;
        connection.execute_batch(
            "PRAGMA foreign_keys = OFF;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }

    fn load_workspace(connection: &Connection, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        let row = connection
            .query_row(
                "SELECT content, state, uri, name, provider_key, metadata, created_at, updated_at
                 FROM workspace WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            content,
            stored_state,
            stored_uri,
            stored_name,
            stored_provider,
            stored_metadata,
            stored_created_at,
            stored_updated_at,
        )) = row
        else {
            return Ok(None);
        };
        let mut workspace: Workspace = serde_json::from_str(&content)?;
        if workspace.id != id
            || workspace.state.as_str() != stored_state
            || workspace.uri != stored_uri
            || workspace.name != stored_name
            || workspace.provider_key != stored_provider
            || workspace.metadata != serde_json::from_str::<WorkspaceMetadata>(&stored_metadata)?
            || workspace.created_at.to_rfc3339() != stored_created_at
            || workspace.updated_at.to_rfc3339() != stored_updated_at
        {
            return Err(WorkspaceError::Validation(
                "workspace columns do not match serialized aggregate".into(),
            ));
        }
        workspace.projects = load_projects(connection, id)?;
        workspace.resources = load_resources(connection, id)?;
        workspace.environment = load_environment(connection, id)?;
        workspace.validate()?;
        Ok(Some(workspace))
    }

    fn insert_projects(
        transaction: &Transaction<'_>,
        workspace: &Workspace,
        actor: &str,
        audit_time: &str,
    ) -> WorkspaceResult<()> {
        for project in &workspace.projects {
            transaction.execute(
                "INSERT INTO project (
                    id, workspace_id, name, project_kind, root_uri, module_count, markers,
                    metadata, content, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?13)",
                params![
                    project.id.to_string(),
                    workspace.id.to_string(),
                    project.name,
                    project.kind.as_str(),
                    project.root_uri,
                    i64::from(project.module_count),
                    serde_json::to_string(&project.markers)?,
                    serde_json::to_string(&project.metadata)?,
                    serde_json::to_string(project)?,
                    project.created_at.to_rfc3339(),
                    project.updated_at.to_rfc3339(),
                    audit_time,
                    actor,
                ],
            )?;
        }
        Ok(())
    }

    fn insert_resources(
        transaction: &Transaction<'_>,
        workspace: &Workspace,
        actor: &str,
        audit_time: &str,
    ) -> WorkspaceResult<()> {
        for resource in &workspace.resources {
            let size_bytes = resource
                .size_bytes
                .map(i64::try_from)
                .transpose()
                .map_err(|_| {
                    WorkspaceError::Validation("resource size exceeds SQLite i64".into())
                })?;
            transaction.execute(
                "INSERT INTO resource (
                    id, workspace_id, project_id, resource_type, uri, name, size_bytes,
                    capabilities, provider_key, metadata, content, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?15)",
                params![
                    resource.id.to_string(),
                    workspace.id.to_string(),
                    resource.project_id.map(|id| id.to_string()),
                    resource.resource_type.as_str(),
                    resource.uri,
                    resource.name,
                    size_bytes,
                    serde_json::to_string(&resource.capabilities)?,
                    resource.provider_key,
                    serde_json::to_string(&resource.metadata)?,
                    serde_json::to_string(resource)?,
                    resource.created_at.to_rfc3339(),
                    resource.updated_at.to_rfc3339(),
                    audit_time,
                    actor,
                ],
            )?;
        }
        Ok(())
    }

    fn insert_environment(
        transaction: &Transaction<'_>,
        workspace: &Workspace,
        actor: &str,
        audit_time: &str,
    ) -> WorkspaceResult<()> {
        let Some(environment) = &workspace.environment else {
            return Ok(());
        };
        transaction.execute(
            "INSERT INTO environment (
                id, workspace_id, os, shell, git, languages, runtimes, package_managers,
                variable_names, metadata, content, detected_at, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16, ?16)",
            params![
                environment.id.to_string(),
                workspace.id.to_string(),
                environment.os,
                environment.shell,
                environment.git,
                serde_json::to_string(&environment.languages)?,
                serde_json::to_string(&environment.runtimes)?,
                serde_json::to_string(&environment.package_managers)?,
                serde_json::to_string(&environment.variable_names)?,
                serde_json::to_string(&environment.metadata)?,
                serde_json::to_string(environment)?,
                environment.detected_at.to_rfc3339(),
                environment.created_at.to_rfc3339(),
                environment.updated_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        Ok(())
    }
}

fn load_projects(connection: &Connection, workspace_id: Uuid) -> WorkspaceResult<Vec<Project>> {
    let mut statement = connection.prepare(
        "SELECT content, id, workspace_id, name, project_kind, root_uri, module_count,
                markers, metadata, created_at, updated_at
         FROM project WHERE workspace_id = ?1 ORDER BY root_uri",
    )?;
    let rows = statement
        .query_map([workspace_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(
                content,
                id,
                stored_workspace_id,
                name,
                kind,
                root_uri,
                module_count,
                markers,
                metadata,
                created_at,
                updated_at,
            )| {
                let project: Project = serde_json::from_str(&content)?;
                let matches = project.id.to_string() == id
                    && project.workspace_id == workspace_id
                    && stored_workspace_id == workspace_id.to_string()
                    && project.name == name
                    && project.kind.as_str() == kind
                    && project.root_uri == root_uri
                    && i64::from(project.module_count) == module_count
                    && project.markers == serde_json::from_str::<Vec<String>>(&markers)?
                    && project.metadata == serde_json::from_str::<WorkspaceMetadata>(&metadata)?
                    && project.created_at.to_rfc3339() == created_at
                    && project.updated_at.to_rfc3339() == updated_at;
                if !matches {
                    return Err(WorkspaceError::Validation(
                        "project columns do not match serialized entity".into(),
                    ));
                }
                project.validate()?;
                Ok(project)
            },
        )
        .collect()
}

fn load_resources(connection: &Connection, workspace_id: Uuid) -> WorkspaceResult<Vec<Resource>> {
    let mut statement = connection.prepare(
        "SELECT content, id, workspace_id, project_id, resource_type, uri, name, size_bytes,
                capabilities, provider_key, metadata, created_at, updated_at
         FROM resource WHERE workspace_id = ?1 ORDER BY uri",
    )?;
    let rows = statement
        .query_map([workspace_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(
                content,
                id,
                stored_workspace_id,
                project_id,
                resource_type,
                uri,
                name,
                size_bytes,
                capabilities,
                provider_key,
                metadata,
                created_at,
                updated_at,
            )| {
                let resource: Resource = serde_json::from_str(&content)?;
                let stored_size = size_bytes
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| WorkspaceError::Validation("negative resource size".into()))?;
                let matches = resource.id.to_string() == id
                    && resource.workspace_id == workspace_id
                    && stored_workspace_id == workspace_id.to_string()
                    && resource.project_id.map(|value| value.to_string()) == project_id
                    && resource.resource_type.as_str() == resource_type
                    && resource.uri == uri
                    && resource.name == name
                    && resource.size_bytes == stored_size
                    && resource.capabilities
                        == serde_json::from_str::<BTreeSet<ResourceCapability>>(&capabilities)?
                    && resource.provider_key == provider_key
                    && resource.metadata == serde_json::from_str::<WorkspaceMetadata>(&metadata)?
                    && resource.created_at.to_rfc3339() == created_at
                    && resource.updated_at.to_rfc3339() == updated_at;
                if !matches {
                    return Err(WorkspaceError::Validation(
                        "resource columns do not match serialized entity".into(),
                    ));
                }
                resource.validate()?;
                Ok(resource)
            },
        )
        .collect()
}

fn load_environment(
    connection: &Connection,
    workspace_id: Uuid,
) -> WorkspaceResult<Option<Environment>> {
    let row = connection
        .query_row(
            "SELECT content, id, workspace_id, os, shell, git, languages, runtimes,
                    package_managers, variable_names, metadata, detected_at, created_at, updated_at
             FROM environment WHERE workspace_id = ?1",
            [workspace_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                ))
            },
        )
        .optional()?;
    row.map(
        |(
            content,
            id,
            stored_workspace_id,
            os,
            shell,
            git,
            languages,
            runtimes,
            package_managers,
            variable_names,
            metadata,
            detected_at,
            created_at,
            updated_at,
        )| {
            let environment: Environment = serde_json::from_str(&content)?;
            let matches = environment.id.to_string() == id
                && environment.workspace_id == workspace_id
                && stored_workspace_id == workspace_id.to_string()
                && environment.os == os
                && environment.shell == shell
                && environment.git == git
                && environment.languages == serde_json::from_str(&languages)?
                && environment.runtimes == serde_json::from_str(&runtimes)?
                && environment.package_managers == serde_json::from_str(&package_managers)?
                && environment.variable_names == serde_json::from_str(&variable_names)?
                && environment.metadata == serde_json::from_str::<WorkspaceMetadata>(&metadata)?
                && environment.detected_at.to_rfc3339() == detected_at
                && environment.created_at.to_rfc3339() == created_at
                && environment.updated_at.to_rfc3339() == updated_at;
            if !matches {
                return Err(WorkspaceError::Validation(
                    "environment columns do not match serialized entity".into(),
                ));
            }
            environment.validate()?;
            Ok(environment)
        },
    )
    .transpose()
}

#[async_trait]
impl WorkspaceCatalog for SqliteWorkspaceStore {
    async fn save_workspace(&self, workspace: &Workspace, actor: &str) -> WorkspaceResult<()> {
        workspace.validate()?;
        validate_actor(actor)?;
        let audit_time = Utc::now().to_rfc3339();
        let mut base = workspace.clone();
        base.projects.clear();
        base.resources.clear();
        base.environment = None;
        let content = serde_json::to_string(&base)?;
        let metadata = serde_json::to_string(&workspace.metadata)?;

        let mut connection = self.pool.get()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO workspace (
                id, name, provider_key, uri, state, metadata, content, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?11)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name, provider_key=excluded.provider_key, uri=excluded.uri,
                state=excluded.state, metadata=excluded.metadata, content=excluded.content,
                updated_at=excluded.updated_at, update_time=excluded.update_time,
                update_user=excluded.update_user",
            params![
                workspace.id.to_string(),
                workspace.name,
                workspace.provider_key,
                workspace.uri,
                workspace.state.as_str(),
                metadata,
                content,
                workspace.created_at.to_rfc3339(),
                workspace.updated_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        transaction.execute(
            "DELETE FROM project WHERE workspace_id = ?1",
            [workspace.id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM resource WHERE workspace_id = ?1",
            [workspace.id.to_string()],
        )?;
        transaction.execute(
            "DELETE FROM environment WHERE workspace_id = ?1",
            [workspace.id.to_string()],
        )?;
        Self::insert_projects(&transaction, workspace, actor, &audit_time)?;
        Self::insert_resources(&transaction, workspace, actor, &audit_time)?;
        Self::insert_environment(&transaction, workspace, actor, &audit_time)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_workspace(&self, id: Uuid) -> WorkspaceResult<Option<Workspace>> {
        let connection = self.pool.get()?;
        Self::load_workspace(&connection, id)
    }

    async fn find_workspace_by_uri(&self, uri: &str) -> WorkspaceResult<Option<Workspace>> {
        let connection = self.pool.get()?;
        let id = connection
            .query_row("SELECT id FROM workspace WHERE uri = ?1", [uri], |row| {
                row.get::<_, String>(0)
            })
            .optional()?;
        id.map(|id| {
            Uuid::parse_str(&id)
                .map_err(|error| {
                    WorkspaceError::Validation(format!("invalid workspace UUID: {error}"))
                })
                .and_then(|id| Self::load_workspace(&connection, id))
        })
        .transpose()
        .map(Option::flatten)
    }

    async fn list_workspaces(&self) -> WorkspaceResult<Vec<Workspace>> {
        let connection = self.pool.get()?;
        let ids = {
            let mut statement = connection.prepare("SELECT id FROM workspace ORDER BY name, id")?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                let id = Uuid::parse_str(&id).map_err(|error| {
                    WorkspaceError::Validation(format!("invalid workspace UUID: {error}"))
                })?;
                Self::load_workspace(&connection, id)?.ok_or_else(|| {
                    WorkspaceError::Internal("workspace disappeared during list".into())
                })
            })
            .collect()
    }

    async fn remove_workspace(&self, id: Uuid) -> WorkspaceResult<bool> {
        let mut connection = self.pool.get()?;
        let transaction = connection.transaction()?;
        for table in ["project", "resource", "environment", "workspace_snapshot"] {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE workspace_id = ?1"),
                [id.to_string()],
            )?;
        }
        let removed =
            transaction.execute("DELETE FROM workspace WHERE id = ?1", [id.to_string()])?;
        transaction.commit()?;
        Ok(removed > 0)
    }

    async fn save_snapshot(&self, snapshot: &Snapshot, actor: &str) -> WorkspaceResult<()> {
        snapshot.validate()?;
        validate_actor(actor)?;
        let resource_count = i64::try_from(snapshot.resource_count).map_err(|_| {
            WorkspaceError::Validation("snapshot resource count exceeds i64".into())
        })?;
        let total_bytes = i64::try_from(snapshot.total_bytes)
            .map_err(|_| WorkspaceError::Validation("snapshot size exceeds i64".into()))?;
        let audit_time = Utc::now().to_rfc3339();
        let connection = self.pool.get()?;
        let workspace_exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace WHERE id = ?1)",
            [snapshot.workspace_id.to_string()],
            |row| row.get(0),
        )?;
        if !workspace_exists {
            return Err(WorkspaceError::NotFound(snapshot.workspace_id.to_string()));
        }
        connection.execute(
            "INSERT INTO workspace_snapshot (
                id, workspace_id, label, storage_uri, resource_count, total_bytes, metadata,
                content, created_at, updated_at, create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10, ?11, ?11)
             ON CONFLICT(id) DO UPDATE SET
                label=excluded.label, storage_uri=excluded.storage_uri,
                resource_count=excluded.resource_count, total_bytes=excluded.total_bytes,
                metadata=excluded.metadata, content=excluded.content,
                updated_at=excluded.updated_at, update_time=excluded.update_time,
                update_user=excluded.update_user",
            params![
                snapshot.id.to_string(),
                snapshot.workspace_id.to_string(),
                snapshot.label,
                snapshot.storage_uri,
                resource_count,
                total_bytes,
                serde_json::to_string(&snapshot.metadata)?,
                serde_json::to_string(snapshot)?,
                snapshot.created_at.to_rfc3339(),
                audit_time,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> WorkspaceResult<Option<Snapshot>> {
        let connection = self.pool.get()?;
        let row = connection
            .query_row(
                "SELECT content, id, workspace_id, label, storage_uri, resource_count,
                        total_bytes, metadata, created_at
                 FROM workspace_snapshot WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()?;
        row.map(|row| decode_snapshot_row(row, Some(id), None))
            .transpose()
    }

    async fn list_snapshots(&self, workspace_id: Uuid) -> WorkspaceResult<Vec<Snapshot>> {
        let connection = self.pool.get()?;
        let rows = {
            let mut statement = connection.prepare(
                "SELECT content, id, workspace_id, label, storage_uri, resource_count,
                        total_bytes, metadata, created_at
                 FROM workspace_snapshot
                 WHERE workspace_id = ?1 ORDER BY created_at DESC, id",
            )?;
            let values = statement
                .query_map([workspace_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        rows.into_iter()
            .map(|row| decode_snapshot_row(row, None, Some(workspace_id)))
            .collect()
    }

    async fn remove_snapshot(&self, id: Uuid) -> WorkspaceResult<bool> {
        let connection = self.pool.get()?;
        Ok(connection.execute(
            "DELETE FROM workspace_snapshot WHERE id = ?1",
            [id.to_string()],
        )? > 0)
    }
}

type SnapshotRow = (
    String,
    String,
    String,
    String,
    String,
    i64,
    i64,
    String,
    String,
);

fn decode_snapshot_row(
    row: SnapshotRow,
    expected_id: Option<Uuid>,
    expected_workspace_id: Option<Uuid>,
) -> WorkspaceResult<Snapshot> {
    let (
        content,
        id,
        workspace_id,
        label,
        storage_uri,
        resource_count,
        total_bytes,
        metadata,
        created_at,
    ) = row;
    let snapshot: Snapshot = serde_json::from_str(&content)?;
    let resource_count = u64::try_from(resource_count)
        .map_err(|_| WorkspaceError::Validation("negative snapshot resource count".into()))?;
    let total_bytes = u64::try_from(total_bytes)
        .map_err(|_| WorkspaceError::Validation("negative snapshot size".into()))?;
    let matches = snapshot.id.to_string() == id
        && snapshot.workspace_id.to_string() == workspace_id
        && expected_id.is_none_or(|value| snapshot.id == value)
        && expected_workspace_id.is_none_or(|value| snapshot.workspace_id == value)
        && snapshot.label == label
        && snapshot.storage_uri == storage_uri
        && snapshot.resource_count == resource_count
        && snapshot.total_bytes == total_bytes
        && snapshot.metadata == serde_json::from_str::<WorkspaceMetadata>(&metadata)?
        && snapshot.created_at.to_rfc3339() == created_at;
    if !matches {
        return Err(WorkspaceError::Validation(
            "snapshot columns do not match serialized entity".into(),
        ));
    }
    snapshot.validate()?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::domain::{
        Environment, GraphEdge, GraphNode, GraphNodeKind, GraphRelation, Snapshot, WorkspaceGraph,
        WorkspaceState,
    };
    use crate::infrastructure::{DefaultWorkspaceLifecycle, WorkspaceLifecycle};

    fn ready_workspace() -> Workspace {
        let mut workspace =
            Workspace::new("demo", "local", "file:///demo/", BTreeMap::new()).unwrap();
        let lifecycle = DefaultWorkspaceLifecycle;
        lifecycle
            .transition(&mut workspace, WorkspaceState::Loaded)
            .unwrap();
        let environment = Environment::new(workspace.id, "test");
        workspace.graph = WorkspaceGraph {
            nodes: vec![
                GraphNode::new(
                    format!("workspace:{}", workspace.id),
                    GraphNodeKind::Workspace,
                    "demo",
                    Some(workspace.uri.clone()),
                ),
                GraphNode::new(
                    format!("environment:{}", environment.id),
                    GraphNodeKind::Environment,
                    "test",
                    None,
                ),
            ],
            edges: vec![GraphEdge {
                source: format!("workspace:{}", workspace.id),
                target: format!("environment:{}", environment.id),
                relation: GraphRelation::DetectedIn,
            }],
        };
        workspace.environment = Some(environment);
        lifecycle
            .transition(&mut workspace, WorkspaceState::Ready)
            .unwrap();
        workspace
    }

    #[tokio::test]
    async fn sqlite_round_trip_preserves_workspace() {
        let store = SqliteWorkspaceStore::new(":memory:").unwrap();
        let workspace = ready_workspace();
        store.save_workspace(&workspace, "tester").await.unwrap();
        let loaded = store.find_workspace(workspace.id).await.unwrap().unwrap();
        assert_eq!(loaded.id, workspace.id);
        assert_eq!(loaded.state, WorkspaceState::Ready);
        assert_eq!(loaded.environment.unwrap().os, "test");
    }

    #[test]
    fn all_tables_have_audit_columns_and_indexes() {
        let store = SqliteWorkspaceStore::new(":memory:").unwrap();
        let connection = store.pool.get().unwrap();
        for table in [
            "workspace",
            "project",
            "resource",
            "environment",
            "workspace_snapshot",
        ] {
            let columns = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap()
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<BTreeSet<_>, _>>()
                .unwrap();
            for required in [
                "id",
                "create_time",
                "update_time",
                "create_user",
                "update_user",
            ] {
                assert!(columns.contains(required), "{table}.{required} is missing");
            }
        }
        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='index' AND name LIKE 'idx_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_count >= 12);
        let foreign_keys: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_list('resource')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0);
    }

    #[tokio::test]
    async fn corrupt_workspace_content_is_reported() {
        let store = SqliteWorkspaceStore::new(":memory:").unwrap();
        let workspace = ready_workspace();
        store.save_workspace(&workspace, "tester").await.unwrap();
        store
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE workspace SET content = '{broken' WHERE id = ?1",
                [workspace.id.to_string()],
            )
            .unwrap();
        assert!(store.find_workspace(workspace.id).await.is_err());
    }

    #[tokio::test]
    async fn corrupt_structured_columns_and_missing_children_are_reported() {
        let store = SqliteWorkspaceStore::new(":memory:").unwrap();
        let workspace = ready_workspace();
        store.save_workspace(&workspace, "tester").await.unwrap();
        store
            .pool
            .get()
            .unwrap()
            .execute(
                "UPDATE workspace SET name = 'tampered' WHERE id = ?1",
                [workspace.id.to_string()],
            )
            .unwrap();
        assert!(store.find_workspace(workspace.id).await.is_err());

        store.save_workspace(&workspace, "tester").await.unwrap();
        store
            .pool
            .get()
            .unwrap()
            .execute(
                "DELETE FROM environment WHERE workspace_id = ?1",
                [workspace.id.to_string()],
            )
            .unwrap();
        assert!(store.find_workspace(workspace.id).await.is_err());
    }

    #[tokio::test]
    async fn orphan_snapshot_is_rejected_without_foreign_keys() {
        let store = SqliteWorkspaceStore::new(":memory:").unwrap();
        let snapshot = Snapshot::new(Uuid::new_v4(), "orphan", "file:///snapshot/", 0, 0);
        assert!(matches!(
            store.save_snapshot(&snapshot, "tester").await,
            Err(WorkspaceError::NotFound(_))
        ));
    }
}
