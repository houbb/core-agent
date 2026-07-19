use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Memory, MemoryIndexEntry, MemoryPolicyDefinition, MemorySnapshot, MemoryState,
};
use crate::error::{MemoryError, MemoryResult};
use crate::infrastructure::{MemoryCommit, MemoryStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteMemoryStore {
    connection: Mutex<Connection>,
}

impl SqliteMemoryStore {
    pub fn new(path: impl AsRef<Path>) -> MemoryResult<Self> {
        let connection = if path.as_ref() == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> MemoryResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| MemoryError::Internal("memory database lock poisoned".into()))
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn commit_batch(&self, commits: &[MemoryCommit], actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        let mut seen = HashSet::new();
        for commit in commits {
            commit.validate()?;
            if !seen.insert(commit.memory.id) {
                return Err(MemoryError::Conflict(
                    "memory batch contains duplicate identity".into(),
                ));
            }
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        for commit in commits {
            write_commit(&transaction, commit, actor)?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn forget(&self, commit: &MemoryCommit, actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        if commit.memory.state != MemoryState::Forgotten {
            return Err(MemoryError::Validation(
                "forget commit must contain a tombstone".into(),
            ));
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_commit(&transaction, commit, actor)?;
        transaction.execute(
            "DELETE FROM memory_snapshot WHERE memory_id = ?1",
            params![commit.memory.id.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_memory(&self, id: Uuid) -> MemoryResult<Option<Memory>> {
        let connection = self.lock()?;
        read_memory(&connection, id)
    }

    async fn find_by_event(&self, event_id: Uuid) -> MemoryResult<Option<Memory>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT id FROM memory WHERE event_id = ?1",
                params![event_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        id.map(|value| parse_uuid("memory id", &value))
            .transpose()?
            .map(|id| read_memory(&connection, id))
            .transpose()
            .map(Option::flatten)
    }

    async fn list_namespace(&self, namespace: &str) -> MemoryResult<Vec<Memory>> {
        let connection = self.lock()?;
        let ids = {
            let mut statement = connection.prepare(
                "SELECT id FROM memory WHERE namespace = ?1 ORDER BY updated_at DESC, id ASC",
            )?;
            let values = statement
                .query_map(params![namespace], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                read_memory(&connection, parse_uuid("memory id", &id)?)?
                    .ok_or_else(|| MemoryError::Internal("listed memory disappeared".into()))
            })
            .collect()
    }

    async fn save_snapshot(&self, snapshot: &MemorySnapshot, actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        snapshot.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let current = read_memory(&transaction, snapshot.memory_id)?
            .ok_or_else(|| MemoryError::NotFound(snapshot.memory_id.to_string()))?;
        if current != snapshot.content {
            return Err(MemoryError::Conflict(
                "snapshot must match the current memory version".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO memory_snapshot (
                id, memory_id, memory_version, label, hash, content, created_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
            params![
                snapshot.id.to_string(),
                snapshot.memory_id.to_string(),
                u64_to_i64(snapshot.memory_version)?,
                snapshot.label,
                snapshot.hash,
                serde_json::to_string(snapshot)?,
                snapshot.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_snapshot(&self, id: Uuid) -> MemoryResult<Option<MemorySnapshot>> {
        let connection = self.lock()?;
        read_snapshot(&connection, id)
    }

    async fn list_snapshots(&self, memory_id: Uuid) -> MemoryResult<Vec<MemorySnapshot>> {
        let connection = self.lock()?;
        let ids = {
            let mut statement = connection.prepare(
                "SELECT id FROM memory_snapshot WHERE memory_id = ?1 ORDER BY created_at DESC, id ASC",
            )?;
            let values = statement
                .query_map(params![memory_id.to_string()], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                read_snapshot(&connection, parse_uuid("snapshot id", &id)?)?.ok_or_else(|| {
                    MemoryError::Internal("listed memory snapshot disappeared".into())
                })
            })
            .collect()
    }

    async fn save_policy(&self, policy: &MemoryPolicyDefinition, actor: &str) -> MemoryResult<()> {
        validate_actor(actor)?;
        policy.validate()?;
        let connection = self.lock()?;
        if let Some(current) = read_policy(&connection, policy.id)? {
            validate_policy_update(&current, policy)?;
            let changed = connection.execute(
                "UPDATE memory_policy SET
                    name = ?1, version = ?2, content = ?3, updated_at = ?4,
                    update_time = ?5, update_user = ?6
                 WHERE id = ?7 AND version = ?8",
                params![
                    policy.name,
                    u64_to_i64(policy.version)?,
                    serde_json::to_string(policy)?,
                    policy.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    policy.id.to_string(),
                    u64_to_i64(current.version)?,
                ],
            )?;
            if changed != 1 {
                return Err(MemoryError::Conflict(format!(
                    "memory policy {} changed concurrently",
                    policy.id
                )));
            }
        } else {
            let now = Utc::now().to_rfc3339();
            connection.execute(
                "INSERT INTO memory_policy (
                    id, policy_key, name, version, content, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?9)",
                params![
                    policy.id.to_string(),
                    policy.key,
                    policy.name,
                    u64_to_i64(policy.version)?,
                    serde_json::to_string(policy)?,
                    policy.created_at.to_rfc3339(),
                    policy.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Ok(())
    }

    async fn find_policy(&self, id: Uuid) -> MemoryResult<Option<MemoryPolicyDefinition>> {
        let connection = self.lock()?;
        read_policy(&connection, id)
    }

    async fn list_policies(&self) -> MemoryResult<Vec<MemoryPolicyDefinition>> {
        let connection = self.lock()?;
        let ids = {
            let mut statement =
                connection.prepare("SELECT id FROM memory_policy ORDER BY policy_key, id")?;
            let values = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            values
        };
        ids.into_iter()
            .map(|id| {
                read_policy(&connection, parse_uuid("policy id", &id)?)?
                    .ok_or_else(|| MemoryError::Internal("listed memory policy disappeared".into()))
            })
            .collect()
    }
}

fn write_commit(
    transaction: &Transaction<'_>,
    commit: &MemoryCommit,
    actor: &str,
) -> MemoryResult<()> {
    commit.validate()?;
    let memory = &commit.memory;
    let now = Utc::now().to_rfc3339();
    match commit.expected_version {
        None => {
            let duplicate = transaction
                .query_row(
                    "SELECT id FROM memory WHERE id = ?1 OR event_id = ?2 LIMIT 1",
                    params![memory.id.to_string(), memory.event_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if duplicate.is_some() {
                return Err(MemoryError::Conflict(
                    "memory or event identity already exists".into(),
                ));
            }
            transaction.execute(
                "INSERT INTO memory (
                    id, event_id, namespace, memory_kind, memory_type, source_kind,
                    importance, state, workspace_id, agent_id, goal_id, execution_id,
                    policy_id, version, expires_at, content, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19, ?20, ?20
                 )",
                params![
                    memory.id.to_string(),
                    memory.event_id.to_string(),
                    memory.namespace,
                    memory.kind.as_str(),
                    memory.memory_type.as_str(),
                    memory.source.kind.as_str(),
                    memory.importance.as_str(),
                    memory.state.as_str(),
                    memory.source.workspace_id.map(|value| value.to_string()),
                    memory.source.agent_id.map(|value| value.to_string()),
                    memory.source.goal_id.map(|value| value.to_string()),
                    memory.source.execution_id.map(|value| value.to_string()),
                    memory.policy.as_ref().map(|value| value.id.to_string()),
                    u64_to_i64(memory.version)?,
                    memory.expires_at.map(|value| value.to_rfc3339()),
                    serde_json::to_string(memory)?,
                    memory.created_at.to_rfc3339(),
                    memory.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Some(expected) => {
            let current = read_memory(transaction, memory.id)?
                .ok_or_else(|| MemoryError::NotFound(memory.id.to_string()))?;
            validate_memory_update(&current, memory)?;
            if current.version != expected {
                return Err(MemoryError::Conflict(format!(
                    "memory {} expected version {expected}, found {}",
                    memory.id, current.version
                )));
            }
            let changed = transaction.execute(
                "UPDATE memory SET
                    memory_kind = ?1, memory_type = ?2, importance = ?3, state = ?4,
                    version = ?5, expires_at = ?6, content = ?7, updated_at = ?8,
                    update_time = ?9, update_user = ?10
                 WHERE id = ?11 AND version = ?12",
                params![
                    memory.kind.as_str(),
                    memory.memory_type.as_str(),
                    memory.importance.as_str(),
                    memory.state.as_str(),
                    u64_to_i64(memory.version)?,
                    memory.expires_at.map(|value| value.to_rfc3339()),
                    serde_json::to_string(memory)?,
                    memory.updated_at.to_rfc3339(),
                    now,
                    actor,
                    memory.id.to_string(),
                    u64_to_i64(expected)?,
                ],
            )?;
            if changed != 1 {
                return Err(MemoryError::Conflict(format!(
                    "memory {} changed concurrently",
                    memory.id
                )));
            }
        }
    }
    transaction.execute(
        "DELETE FROM memory_index WHERE memory_id = ?1",
        params![memory.id.to_string()],
    )?;
    transaction.execute(
        "DELETE FROM memory_tag WHERE memory_id = ?1",
        params![memory.id.to_string()],
    )?;
    if memory.state == MemoryState::Forgotten {
        transaction.execute(
            "DELETE FROM memory_snapshot WHERE memory_id = ?1",
            params![memory.id.to_string()],
        )?;
    }
    if let Some(index) = &commit.index {
        write_index(transaction, index, actor)?;
        for tag in &memory.tags {
            let tag_id = Uuid::new_v4();
            transaction.execute(
                "INSERT INTO memory_tag (
                    id, memory_id, namespace, tag, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5, ?5, ?6, ?6)",
                params![
                    tag_id.to_string(),
                    memory.id.to_string(),
                    memory.namespace,
                    tag,
                    now,
                    actor,
                ],
            )?;
        }
    }
    Ok(())
}

fn write_index(
    transaction: &Transaction<'_>,
    index: &MemoryIndexEntry,
    actor: &str,
) -> MemoryResult<()> {
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO memory_index (
            id, memory_id, namespace, normalized_text, memory_kind, memory_type,
            source_kind, importance, state, workspace_id, agent_id, goal_id,
            memory_version, created_at, updated_at, content, create_time,
            update_time, create_user, update_user
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?17, ?18, ?18
         )",
        params![
            index.id.to_string(),
            index.memory_id.to_string(),
            index.namespace,
            index.normalized_text,
            index.kind.as_str(),
            index.memory_type.as_str(),
            index.source.as_str(),
            index.importance.as_str(),
            index.state.as_str(),
            index.workspace_id.map(|value| value.to_string()),
            index.agent_id.map(|value| value.to_string()),
            index.goal_id.map(|value| value.to_string()),
            u64_to_i64(index.memory_version)?,
            index.created_at.to_rfc3339(),
            index.updated_at.to_rfc3339(),
            serde_json::to_string(index)?,
            now,
            actor,
        ],
    )?;
    Ok(())
}

struct RawMemoryRow {
    id: String,
    event_id: String,
    namespace: String,
    memory_kind: String,
    memory_type: String,
    source_kind: String,
    importance: String,
    state: String,
    workspace_id: Option<String>,
    agent_id: Option<String>,
    goal_id: Option<String>,
    execution_id: Option<String>,
    policy_id: Option<String>,
    version: i64,
    expires_at: Option<String>,
    content: String,
    created_at: String,
    updated_at: String,
}

fn read_memory(connection: &Connection, id: Uuid) -> MemoryResult<Option<Memory>> {
    let raw = connection
        .query_row(
            "SELECT id, event_id, namespace, memory_kind, memory_type, source_kind,
                    importance, state, workspace_id, agent_id, goal_id, execution_id,
                    policy_id, version, expires_at, content, created_at, updated_at
             FROM memory WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok(RawMemoryRow {
                    id: row.get(0)?,
                    event_id: row.get(1)?,
                    namespace: row.get(2)?,
                    memory_kind: row.get(3)?,
                    memory_type: row.get(4)?,
                    source_kind: row.get(5)?,
                    importance: row.get(6)?,
                    state: row.get(7)?,
                    workspace_id: row.get(8)?,
                    agent_id: row.get(9)?,
                    goal_id: row.get(10)?,
                    execution_id: row.get(11)?,
                    policy_id: row.get(12)?,
                    version: row.get(13)?,
                    expires_at: row.get(14)?,
                    content: row.get(15)?,
                    created_at: row.get(16)?,
                    updated_at: row.get(17)?,
                })
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let memory: Memory = serde_json::from_str(&raw.content)?;
    memory.validate()?;
    let policy_id = memory.policy.as_ref().map(|value| value.id.to_string());
    if raw.id != memory.id.to_string()
        || raw.event_id != memory.event_id.to_string()
        || raw.namespace != memory.namespace
        || raw.memory_kind != memory.kind.as_str()
        || raw.memory_type != memory.memory_type.as_str()
        || raw.source_kind != memory.source.kind.as_str()
        || raw.importance != memory.importance.as_str()
        || raw.state != memory.state.as_str()
        || raw.workspace_id != memory.source.workspace_id.map(|value| value.to_string())
        || raw.agent_id != memory.source.agent_id.map(|value| value.to_string())
        || raw.goal_id != memory.source.goal_id.map(|value| value.to_string())
        || raw.execution_id != memory.source.execution_id.map(|value| value.to_string())
        || raw.policy_id != policy_id
        || raw.version != u64_to_i64(memory.version)?
        || raw.expires_at != memory.expires_at.map(|value| value.to_rfc3339())
        || raw.created_at != memory.created_at.to_rfc3339()
        || raw.updated_at != memory.updated_at.to_rfc3339()
    {
        return Err(MemoryError::Validation(
            "memory structured columns do not match serialized aggregate".into(),
        ));
    }
    validate_relations(connection, &memory)?;
    Ok(Some(memory))
}

struct RawIndexRow {
    id: String,
    namespace: String,
    normalized_text: String,
    memory_kind: String,
    memory_type: String,
    source_kind: String,
    importance: String,
    state: String,
    workspace_id: Option<String>,
    agent_id: Option<String>,
    goal_id: Option<String>,
    memory_version: i64,
    created_at: String,
    updated_at: String,
    content: String,
}

fn validate_relations(connection: &Connection, memory: &Memory) -> MemoryResult<()> {
    let tag_values = {
        let mut statement =
            connection.prepare("SELECT tag FROM memory_tag WHERE memory_id = ?1 ORDER BY tag")?;
        let values = statement
            .query_map(params![memory.id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<BTreeSet<_>, _>>()?;
        values
    };
    if memory.state == MemoryState::Forgotten {
        let index_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_index WHERE memory_id = ?1",
            params![memory.id.to_string()],
            |row| row.get(0),
        )?;
        let snapshot_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM memory_snapshot WHERE memory_id = ?1",
            params![memory.id.to_string()],
            |row| row.get(0),
        )?;
        if index_count != 0 || snapshot_count != 0 || !tag_values.is_empty() {
            return Err(MemoryError::Validation(
                "forgotten memory retained index, tag or snapshot content".into(),
            ));
        }
        return Ok(());
    }
    if tag_values != memory.tags {
        return Err(MemoryError::Validation(
            "memory tags do not match aggregate".into(),
        ));
    }
    let raw = connection
        .query_row(
            "SELECT id, namespace, normalized_text, memory_kind, memory_type,
                    source_kind, importance, state, workspace_id, agent_id, goal_id,
                    memory_version, created_at, updated_at, content
             FROM memory_index WHERE memory_id = ?1",
            params![memory.id.to_string()],
            |row| {
                Ok(RawIndexRow {
                    id: row.get(0)?,
                    namespace: row.get(1)?,
                    normalized_text: row.get(2)?,
                    memory_kind: row.get(3)?,
                    memory_type: row.get(4)?,
                    source_kind: row.get(5)?,
                    importance: row.get(6)?,
                    state: row.get(7)?,
                    workspace_id: row.get(8)?,
                    agent_id: row.get(9)?,
                    goal_id: row.get(10)?,
                    memory_version: row.get(11)?,
                    created_at: row.get(12)?,
                    updated_at: row.get(13)?,
                    content: row.get(14)?,
                })
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Err(MemoryError::Validation(
            "memory aggregate is missing its index".into(),
        ));
    };
    let index: MemoryIndexEntry = serde_json::from_str(&raw.content)?;
    index.validate_for(memory)?;
    if raw.id != index.id.to_string()
        || raw.namespace != index.namespace
        || raw.normalized_text != index.normalized_text
        || raw.memory_kind != index.kind.as_str()
        || raw.memory_type != index.memory_type.as_str()
        || raw.source_kind != index.source.as_str()
        || raw.importance != index.importance.as_str()
        || raw.state != index.state.as_str()
        || raw.workspace_id != index.workspace_id.map(|value| value.to_string())
        || raw.agent_id != index.agent_id.map(|value| value.to_string())
        || raw.goal_id != index.goal_id.map(|value| value.to_string())
        || raw.memory_version != u64_to_i64(index.memory_version)?
        || raw.created_at != index.created_at.to_rfc3339()
        || raw.updated_at != index.updated_at.to_rfc3339()
    {
        return Err(MemoryError::Validation(
            "memory index does not match aggregate".into(),
        ));
    }
    Ok(())
}

fn read_snapshot(connection: &Connection, id: Uuid) -> MemoryResult<Option<MemorySnapshot>> {
    let raw = connection
        .query_row(
            "SELECT memory_id, memory_version, label, hash, content, created_at
             FROM memory_snapshot WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let snapshot: MemorySnapshot = serde_json::from_str(&raw.4)?;
    snapshot.validate()?;
    if snapshot.id != id
        || raw.0 != snapshot.memory_id.to_string()
        || raw.1 != u64_to_i64(snapshot.memory_version)?
        || raw.2 != snapshot.label
        || raw.3 != snapshot.hash
        || raw.5 != snapshot.created_at.to_rfc3339()
    {
        return Err(MemoryError::Validation(
            "memory snapshot columns do not match content".into(),
        ));
    }
    Ok(Some(snapshot))
}

fn read_policy(connection: &Connection, id: Uuid) -> MemoryResult<Option<MemoryPolicyDefinition>> {
    let raw = connection
        .query_row(
            "SELECT policy_key, name, version, content, created_at, updated_at
             FROM memory_policy WHERE id = ?1",
            params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let policy: MemoryPolicyDefinition = serde_json::from_str(&raw.3)?;
    policy.validate()?;
    if policy.id != id
        || raw.0 != policy.key
        || raw.1 != policy.name
        || raw.2 != u64_to_i64(policy.version)?
        || raw.4 != policy.created_at.to_rfc3339()
        || raw.5 != policy.updated_at.to_rfc3339()
    {
        return Err(MemoryError::Validation(
            "memory policy columns do not match content".into(),
        ));
    }
    Ok(Some(policy))
}

fn validate_memory_update(current: &Memory, next: &Memory) -> MemoryResult<()> {
    if current.id != next.id
        || current.event_id != next.event_id
        || current.namespace != next.namespace
        || current.source != next.source
        || current.policy != next.policy
        || current.created_at != next.created_at
    {
        return Err(MemoryError::Validation(
            "memory update changed immutable identity or ownership".into(),
        ));
    }
    Ok(())
}

fn validate_policy_update(
    current: &MemoryPolicyDefinition,
    next: &MemoryPolicyDefinition,
) -> MemoryResult<()> {
    if current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.version != current.version.saturating_add(1)
        || next.updated_at <= current.updated_at
    {
        return Err(MemoryError::Validation(
            "memory policy update changed identity or version sequence".into(),
        ));
    }
    Ok(())
}

fn parse_uuid(label: &str, value: &str) -> MemoryResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|_| MemoryError::Validation(format!("{label} is not a valid UUID")))
}

fn u64_to_i64(value: u64) -> MemoryResult<i64> {
    i64::try_from(value)
        .map_err(|_| MemoryError::Validation("memory version exceeds SQLite range".into()))
}
