use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, validate_timeline, WorkflowDefinition, WorkflowIdentity, WorkflowInstance,
    WorkflowSnapshot, WorkflowState, WorkflowStateRecord,
};
use crate::error::{WorkflowError, WorkflowResult};
use crate::infrastructure::{WorkflowInstanceCommit, WorkflowRegistrationCommit, WorkflowStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteWorkflowStore {
    connection: Mutex<Connection>,
}

impl SqliteWorkflowStore {
    pub fn new(path: impl AsRef<Path>) -> WorkflowResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> WorkflowResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> WorkflowResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> WorkflowResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| WorkflowError::Internal("workflow SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl WorkflowStore for SqliteWorkflowStore {
    async fn save_registration(
        &self,
        commit: &WorkflowRegistrationCommit,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_registration(&transaction, commit, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_workflow(&self, id: Uuid) -> WorkflowResult<Option<WorkflowIdentity>> {
        let connection = self.lock()?;
        read_workflow(&connection, id)
    }

    async fn find_workflow_by_key(&self, key: &str) -> WorkflowResult<Option<WorkflowIdentity>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT id FROM workflow WHERE workflow_key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        id.map(|value| read_workflow(&connection, parse_uuid("workflow id", &value)?))
            .transpose()
            .map(Option::flatten)
    }

    async fn list_workflows(&self) -> WorkflowResult<Vec<WorkflowIdentity>> {
        let connection = self.lock()?;
        query_ids(
            &connection,
            "SELECT id FROM workflow ORDER BY workflow_key, id",
        )?
        .into_iter()
        .map(|id| {
            read_workflow(&connection, parse_uuid("workflow id", &id)?)?
                .ok_or_else(|| WorkflowError::Internal("listed Workflow disappeared".into()))
        })
        .collect()
    }

    async fn find_definition(
        &self,
        workflow_id: Uuid,
        version: u64,
    ) -> WorkflowResult<Option<WorkflowDefinition>> {
        let connection = self.lock()?;
        if read_workflow(&connection, workflow_id)?.is_none() {
            return Ok(None);
        }
        read_definition(&connection, workflow_id, version)
    }

    async fn list_definitions(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowDefinition>> {
        let connection = self.lock()?;
        if read_workflow(&connection, workflow_id)?.is_none() {
            return Ok(Vec::new());
        }
        let mut statement = connection.prepare(
            "SELECT definition_version FROM workflow_definition
             WHERE workflow_id = ?1 ORDER BY definition_version",
        )?;
        let versions = statement
            .query_map([workflow_id.to_string()], |row| row.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        versions
            .into_iter()
            .map(|version| {
                let version = i64_u64("definition version", version)?;
                read_definition(&connection, workflow_id, version)?.ok_or_else(|| {
                    WorkflowError::Internal("listed Workflow Definition disappeared".into())
                })
            })
            .collect()
    }

    async fn commit_instance(
        &self,
        commit: &WorkflowInstanceCommit,
        actor: &str,
    ) -> WorkflowResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_instance(&transaction, commit, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_instance(&self, id: Uuid) -> WorkflowResult<Option<WorkflowInstance>> {
        let connection = self.lock()?;
        read_instance(&connection, id)
    }

    async fn list_instances(&self, workflow_id: Uuid) -> WorkflowResult<Vec<WorkflowInstance>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id FROM workflow_instance WHERE workflow_id = ?1
             ORDER BY created_at DESC, id",
        )?;
        let ids = statement
            .query_map([workflow_id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_instance(&connection, parse_uuid("instance id", &id)?)?.ok_or_else(|| {
                    WorkflowError::Internal("listed Workflow Instance disappeared".into())
                })
            })
            .collect()
    }

    async fn list_states(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowStateRecord>> {
        let connection = self.lock()?;
        let instance = read_instance(&connection, instance_id)?
            .ok_or_else(|| WorkflowError::NotFound(instance_id.to_string()))?;
        let mut statement = connection.prepare(
            "SELECT id FROM workflow_state WHERE instance_id = ?1 ORDER BY sequence, id",
        )?;
        let ids = statement
            .query_map([instance_id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let values = ids
            .into_iter()
            .map(|id| {
                read_state(&connection, parse_uuid("state id", &id)?)?.ok_or_else(|| {
                    WorkflowError::Internal("listed Workflow state disappeared".into())
                })
            })
            .collect::<WorkflowResult<Vec<_>>>()?;
        validate_timeline(&instance, &values)?;
        Ok(values)
    }

    async fn save_snapshot(&self, value: &WorkflowSnapshot, actor: &str) -> WorkflowResult<()> {
        validate_actor(actor)?;
        value.validate()?;
        let connection = self.lock()?;
        let instance = read_instance(&connection, value.instance_id)?
            .ok_or_else(|| WorkflowError::NotFound(value.instance_id.to_string()))?;
        if value.sequence > instance.version {
            return Err(WorkflowError::Conflict(
                "Workflow Snapshot is newer than its Instance".into(),
            ));
        }
        if connection
            .query_row(
                "SELECT 1 FROM workflow_snapshot WHERE id = ?1",
                [value.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Err(WorkflowError::Conflict(
                "Workflow Snapshot already exists".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO workflow_snapshot (
                id, instance_id, sequence, label, hash, content, created_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
            params![
                value.id.to_string(),
                value.instance_id.to_string(),
                u64_i64(value.sequence)?,
                value.label,
                value.hash,
                serde_json::to_string(value)?,
                value.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> WorkflowResult<Option<WorkflowSnapshot>> {
        let connection = self.lock()?;
        read_snapshot(&connection, id)
    }

    async fn list_snapshots(&self, instance_id: Uuid) -> WorkflowResult<Vec<WorkflowSnapshot>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id FROM workflow_snapshot WHERE instance_id = ?1
             ORDER BY sequence DESC, id",
        )?;
        let ids = statement
            .query_map([instance_id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter()
            .map(|id| {
                read_snapshot(&connection, parse_uuid("snapshot id", &id)?)?.ok_or_else(|| {
                    WorkflowError::Internal("listed Workflow Snapshot disappeared".into())
                })
            })
            .collect()
    }
}

fn write_registration(
    transaction: &Transaction<'_>,
    commit: &WorkflowRegistrationCommit,
    actor: &str,
) -> WorkflowResult<()> {
    match commit.expected_identity_version {
        None => {
            let duplicate = transaction
                .query_row(
                    "SELECT id FROM workflow WHERE id = ?1 OR workflow_key = ?2 LIMIT 1",
                    params![commit.identity.id.to_string(), commit.identity.key],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if duplicate.is_some() || commit.identity.version != 1 || commit.definition.version != 1
            {
                return Err(WorkflowError::Conflict(
                    "Workflow identity or key already exists".into(),
                ));
            }
            let now = Utc::now().to_rfc3339();
            transaction.execute(
                "INSERT INTO workflow (
                    id, workflow_key, name, current_definition_id,
                    current_definition_version, enabled, version, content,
                    created_at, updated_at, create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12)",
                params![
                    commit.identity.id.to_string(),
                    commit.identity.key,
                    commit.identity.name,
                    commit.identity.current_definition_id.to_string(),
                    u64_i64(commit.identity.current_definition_version)?,
                    bool_i64(commit.identity.enabled),
                    u64_i64(commit.identity.version)?,
                    serde_json::to_string(&commit.identity)?,
                    commit.identity.created_at.to_rfc3339(),
                    commit.identity.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            let current = read_workflow(transaction, commit.identity.id)?
                .ok_or_else(|| WorkflowError::NotFound(commit.identity.id.to_string()))?;
            validate_identity_update(&current, &commit.identity, &commit.definition, expected)?;
            let changed = transaction.execute(
                "UPDATE workflow SET name = ?1, current_definition_id = ?2,
                    current_definition_version = ?3, enabled = ?4, version = ?5,
                    content = ?6, updated_at = ?7, update_time = ?8, update_user = ?9
                 WHERE id = ?10 AND version = ?11",
                params![
                    commit.identity.name,
                    commit.identity.current_definition_id.to_string(),
                    u64_i64(commit.identity.current_definition_version)?,
                    bool_i64(commit.identity.enabled),
                    u64_i64(commit.identity.version)?,
                    serde_json::to_string(&commit.identity)?,
                    commit.identity.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    commit.identity.id.to_string(),
                    u64_i64(expected)?,
                ],
            )?;
            if changed != 1 {
                return Err(WorkflowError::Conflict(
                    "Workflow identity changed concurrently".into(),
                ));
            }
        }
    }
    if transaction
        .query_row(
            "SELECT 1 FROM workflow_definition WHERE id = ?1
             OR (workflow_id = ?2 AND definition_version = ?3) LIMIT 1",
            params![
                commit.definition.id.to_string(),
                commit.definition.workflow_id.to_string(),
                u64_i64(commit.definition.version)?,
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some()
    {
        return Err(WorkflowError::Conflict(
            "Workflow Definition version already exists".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO workflow_definition (
            id, workflow_id, definition_version, definition_key, name, content,
            created_at, create_time, update_time, create_user, update_user
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
        params![
            commit.definition.id.to_string(),
            commit.definition.workflow_id.to_string(),
            u64_i64(commit.definition.version)?,
            commit.definition.key,
            commit.definition.name,
            serde_json::to_string(&commit.definition)?,
            commit.definition.created_at.to_rfc3339(),
            now,
            actor,
        ],
    )?;
    Ok(())
}

fn write_instance(
    transaction: &Transaction<'_>,
    commit: &WorkflowInstanceCommit,
    actor: &str,
) -> WorkflowResult<()> {
    let definition = read_definition(
        transaction,
        commit.instance.workflow_id,
        commit.instance.definition_version,
    )?
    .ok_or_else(|| WorkflowError::NotFound(commit.instance.definition_id.to_string()))?;
    if definition != commit.instance.definition || definition.id != commit.instance.definition_id {
        return Err(WorkflowError::Validation(
            "Workflow Instance Definition snapshot does not match Catalog".into(),
        ));
    }
    let current_ids = commit.instance.current_ids();
    match commit.expected_version {
        None => {
            if read_instance(transaction, commit.instance.id)?.is_some()
                || commit.state_record.is_none()
            {
                return Err(WorkflowError::Conflict(
                    "Workflow Instance already exists or lacks initial state".into(),
                ));
            }
            let now = Utc::now().to_rfc3339();
            transaction.execute(
                "INSERT INTO workflow_instance (
                    id, workflow_id, definition_id, definition_version, state,
                    current_stage_id, current_activity_id, current_action_id,
                    version, content, started_at, completed_at, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                           ?13, ?14, ?15, ?15, ?16, ?16)",
                params![
                    commit.instance.id.to_string(),
                    commit.instance.workflow_id.to_string(),
                    commit.instance.definition_id.to_string(),
                    u64_i64(commit.instance.definition_version)?,
                    commit.instance.state.as_str(),
                    uuid_text(current_ids.0),
                    uuid_text(current_ids.1),
                    uuid_text(current_ids.2),
                    u64_i64(commit.instance.version)?,
                    serde_json::to_string(&commit.instance)?,
                    time_text(commit.instance.started_at),
                    time_text(commit.instance.completed_at),
                    commit.instance.created_at.to_rfc3339(),
                    commit.instance.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            let current = read_instance(transaction, commit.instance.id)?
                .ok_or_else(|| WorkflowError::NotFound(commit.instance.id.to_string()))?;
            validate_instance_update(&current, &commit.instance, expected)?;
            validate_state_record_change(&current, commit)?;
            let changed = transaction.execute(
                "UPDATE workflow_instance SET state = ?1, current_stage_id = ?2,
                    current_activity_id = ?3, current_action_id = ?4, version = ?5,
                    content = ?6, started_at = ?7, completed_at = ?8, updated_at = ?9,
                    update_time = ?10, update_user = ?11
                 WHERE id = ?12 AND version = ?13",
                params![
                    commit.instance.state.as_str(),
                    uuid_text(current_ids.0),
                    uuid_text(current_ids.1),
                    uuid_text(current_ids.2),
                    u64_i64(commit.instance.version)?,
                    serde_json::to_string(&commit.instance)?,
                    time_text(commit.instance.started_at),
                    time_text(commit.instance.completed_at),
                    commit.instance.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    commit.instance.id.to_string(),
                    u64_i64(expected)?,
                ],
            )?;
            if changed != 1 {
                return Err(WorkflowError::Conflict(
                    "Workflow Instance changed concurrently".into(),
                ));
            }
        }
    }
    if let Some(record) = &commit.state_record {
        transaction.execute(
            "INSERT INTO workflow_state (
                id, instance_id, sequence, from_state, to_state, reason, content,
                created_at, create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10)",
            params![
                record.id.to_string(),
                record.instance_id.to_string(),
                u64_i64(record.sequence)?,
                record.from_state.map(WorkflowState::as_str),
                record.to_state.as_str(),
                record.reason,
                serde_json::to_string(record)?,
                record.created_at.to_rfc3339(),
                Utc::now().to_rfc3339(),
                actor,
            ],
        )?;
    }
    Ok(())
}

fn read_workflow(connection: &Connection, id: Uuid) -> WorkflowResult<Option<WorkflowIdentity>> {
    type Row = (
        String,
        String,
        String,
        String,
        i64,
        i64,
        i64,
        String,
        String,
        String,
    );
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, workflow_key, name, current_definition_id,
                    current_definition_version, enabled, version, content, created_at, updated_at
             FROM workflow WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: WorkflowIdentity = serde_json::from_str(&raw.7)?;
    value.validate()?;
    if raw.0 != value.id.to_string()
        || raw.1 != value.key
        || raw.2 != value.name
        || raw.3 != value.current_definition_id.to_string()
        || raw.4 != u64_i64(value.current_definition_version)?
        || raw.5 != bool_i64(value.enabled)
        || raw.6 != u64_i64(value.version)?
        || raw.8 != value.created_at.to_rfc3339()
        || raw.9 != value.updated_at.to_rfc3339()
    {
        return Err(WorkflowError::Validation(
            "Workflow columns do not match serialized content".into(),
        ));
    }
    let definition = read_definition(connection, value.id, value.current_definition_version)?
        .ok_or_else(|| {
            WorkflowError::Validation("current Workflow Definition is missing".into())
        })?;
    if definition.id != value.current_definition_id || definition.key != value.key {
        return Err(WorkflowError::Validation(
            "Workflow current Definition reference is inconsistent".into(),
        ));
    }
    Ok(Some(value))
}

fn read_definition(
    connection: &Connection,
    workflow_id: Uuid,
    version: u64,
) -> WorkflowResult<Option<WorkflowDefinition>> {
    type Row = (String, String, i64, String, String, String, String);
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, workflow_id, definition_version, definition_key, name, content, created_at
             FROM workflow_definition WHERE workflow_id = ?1 AND definition_version = ?2",
            params![workflow_id.to_string(), u64_i64(version)?],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: WorkflowDefinition = serde_json::from_str(&raw.5)?;
    value.validate()?;
    if raw.0 != value.id.to_string()
        || raw.1 != value.workflow_id.to_string()
        || raw.2 != u64_i64(value.version)?
        || raw.3 != value.key
        || raw.4 != value.name
        || raw.6 != value.created_at.to_rfc3339()
    {
        return Err(WorkflowError::Validation(
            "Workflow Definition columns do not match serialized content".into(),
        ));
    }
    Ok(Some(value))
}

fn read_instance(connection: &Connection, id: Uuid) -> WorkflowResult<Option<WorkflowInstance>> {
    type Row = (
        String,
        String,
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        i64,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
    );
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, workflow_id, definition_id, definition_version, state,
                    current_stage_id, current_activity_id, current_action_id, version, content,
                    started_at, completed_at, created_at, updated_at
             FROM workflow_instance WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                    row.get(13)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: WorkflowInstance = serde_json::from_str(&raw.9)?;
    value.validate()?;
    let current_ids = value.current_ids();
    if raw.0 != value.id.to_string()
        || raw.1 != value.workflow_id.to_string()
        || raw.2 != value.definition_id.to_string()
        || raw.3 != u64_i64(value.definition_version)?
        || raw.4 != value.state.as_str()
        || raw.5 != uuid_text(current_ids.0)
        || raw.6 != uuid_text(current_ids.1)
        || raw.7 != uuid_text(current_ids.2)
        || raw.8 != u64_i64(value.version)?
        || raw.10 != time_text(value.started_at)
        || raw.11 != time_text(value.completed_at)
        || raw.12 != value.created_at.to_rfc3339()
        || raw.13 != value.updated_at.to_rfc3339()
    {
        return Err(WorkflowError::Validation(
            "Workflow Instance columns do not match serialized content".into(),
        ));
    }
    let definition = read_definition(connection, value.workflow_id, value.definition_version)?
        .ok_or_else(|| {
            WorkflowError::Validation("Workflow Instance Definition is missing".into())
        })?;
    if definition != value.definition || definition.id != value.definition_id {
        return Err(WorkflowError::Validation(
            "Workflow Instance Definition snapshot differs from Catalog".into(),
        ));
    }
    Ok(Some(value))
}

fn read_snapshot(connection: &Connection, id: Uuid) -> WorkflowResult<Option<WorkflowSnapshot>> {
    type Row = (String, String, i64, String, String, String, String);
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, instance_id, sequence, label, hash, content, created_at
             FROM workflow_snapshot WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: WorkflowSnapshot = serde_json::from_str(&raw.5)?;
    value.validate()?;
    if raw.0 != value.id.to_string()
        || raw.1 != value.instance_id.to_string()
        || raw.2 != u64_i64(value.sequence)?
        || raw.3 != value.label
        || raw.4 != value.hash
        || raw.6 != value.created_at.to_rfc3339()
    {
        return Err(WorkflowError::Validation(
            "Workflow Snapshot columns do not match serialized content".into(),
        ));
    }
    let instance = read_instance(connection, value.instance_id)?
        .ok_or_else(|| WorkflowError::Validation("Workflow Snapshot owner is missing".into()))?;
    if value.sequence > instance.version
        || value.content.workflow_id != instance.workflow_id
        || value.content.definition_id != instance.definition_id
    {
        return Err(WorkflowError::Validation(
            "Workflow Snapshot owner or sequence is inconsistent".into(),
        ));
    }
    Ok(Some(value))
}

fn read_state(connection: &Connection, id: Uuid) -> WorkflowResult<Option<WorkflowStateRecord>> {
    type Row = (
        String,
        String,
        i64,
        Option<String>,
        String,
        String,
        String,
        String,
    );
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, instance_id, sequence, from_state, to_state, reason, content, created_at
             FROM workflow_state WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: WorkflowStateRecord = serde_json::from_str(&raw.6)?;
    value.validate()?;
    if raw.0 != value.id.to_string()
        || raw.1 != value.instance_id.to_string()
        || raw.2 != u64_i64(value.sequence)?
        || raw.3.as_deref() != value.from_state.map(WorkflowState::as_str)
        || raw.4 != value.to_state.as_str()
        || raw.5 != value.reason
        || raw.7 != value.created_at.to_rfc3339()
    {
        return Err(WorkflowError::Validation(
            "Workflow state columns do not match serialized content".into(),
        ));
    }
    Ok(Some(value))
}

fn validate_identity_update(
    current: &WorkflowIdentity,
    next: &WorkflowIdentity,
    definition: &WorkflowDefinition,
    expected: u64,
) -> WorkflowResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || definition.version != current.current_definition_version.saturating_add(1)
        || next.current_definition_version != definition.version
        || next.updated_at < current.updated_at
    {
        return Err(WorkflowError::Conflict(
            "Workflow identity or Definition version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_instance_update(
    current: &WorkflowInstance,
    next: &WorkflowInstance,
    expected: u64,
) -> WorkflowResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.workflow_id != next.workflow_id
        || current.definition_id != next.definition_id
        || current.definition_version != next.definition_version
        || current.definition != next.definition
        || current.variables != next.variables
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(WorkflowError::Conflict(
            "Workflow Instance identity, Definition, Variables or version conflict".into(),
        ));
    }
    Ok(())
}

fn validate_state_record_change(
    current: &WorkflowInstance,
    commit: &WorkflowInstanceCommit,
) -> WorkflowResult<()> {
    let changed = current.state != commit.instance.state;
    if changed != commit.state_record.is_some()
        || commit.state_record.as_ref().is_some_and(|record| {
            record.from_state != Some(current.state) || record.to_state != commit.instance.state
        })
    {
        return Err(WorkflowError::Validation(
            "Workflow lifecycle changes require one matching Timeline record".into(),
        ));
    }
    Ok(())
}

fn query_ids(connection: &Connection, sql: &str) -> WorkflowResult<Vec<String>> {
    let mut statement = connection.prepare(sql)?;
    let values = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(values)
}

fn parse_uuid(label: &str, value: &str) -> WorkflowResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|error| WorkflowError::Validation(format!("invalid {label}: {error}")))
}

fn u64_i64(value: u64) -> WorkflowResult<i64> {
    i64::try_from(value)
        .map_err(|_| WorkflowError::Validation("workflow integer exceeds SQLite range".into()))
}

fn i64_u64(label: &str, value: i64) -> WorkflowResult<u64> {
    u64::try_from(value)
        .map_err(|_| WorkflowError::Validation(format!("{label} cannot be negative")))
}

fn bool_i64(value: bool) -> i64 {
    i64::from(value)
}

fn uuid_text(value: Option<Uuid>) -> Option<String> {
    value.map(|id| id.to_string())
}

fn time_text(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|time| time.to_rfc3339())
}
