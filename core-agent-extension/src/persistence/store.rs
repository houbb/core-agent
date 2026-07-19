use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::defaults::validate_timeline;
use crate::domain::{
    validate_actor, Capability, Extension, ExtensionManifestRecord, ExtensionState,
    ExtensionStateRecord, Provider,
};
use crate::error::{ExtensionError, ExtensionResult};
use crate::infrastructure::{ExtensionRegistrationCommit, ExtensionStateCommit, ExtensionStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteExtensionStore {
    connection: Mutex<Connection>,
}

impl SqliteExtensionStore {
    pub fn new(path: impl AsRef<Path>) -> ExtensionResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn open_in_memory() -> ExtensionResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> ExtensionResult<Self> {
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys=OFF;")?;
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
    fn lock(&self) -> ExtensionResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| ExtensionError::Internal("SQLite Extension lock poisoned".into()))
    }
}

#[async_trait]
impl ExtensionStore for SqliteExtensionStore {
    async fn save_registration(
        &self,
        commit: &ExtensionRegistrationCommit,
        actor: &str,
    ) -> ExtensionResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        validate_registration(&transaction, commit)?;
        write_extension(
            &transaction,
            &commit.extension,
            commit.expected_extension_version,
            actor,
        )?;
        insert_manifest(&transaction, &commit.manifest, actor)?;
        for capability in &commit.capabilities {
            insert_capability(&transaction, capability, actor)?;
        }
        for provider in &commit.providers {
            insert_provider(&transaction, provider, actor)?;
        }
        insert_state(&transaction, &commit.state_record, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn commit_state(
        &self,
        commit: &ExtensionStateCommit,
        actor: &str,
    ) -> ExtensionResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let current = read_extension(&transaction, commit.extension.id)?
            .ok_or_else(|| ExtensionError::not_found(commit.extension.id))?;
        validate_extension_update(&current, &commit.extension, commit.expected_version)?;
        if commit.state_record.from_state != Some(current.state) {
            return Err(ExtensionError::Validation(
                "Extension state transition lacks matching timeline record".into(),
            ));
        }
        write_extension(
            &transaction,
            &commit.extension,
            Some(commit.expected_version),
            actor,
        )?;
        for capability in &commit.capabilities {
            update_capability(&transaction, capability, actor)?;
        }
        for provider in &commit.providers {
            update_provider(&transaction, provider, actor)?;
        }
        insert_state(&transaction, &commit.state_record, actor)?;
        transaction.commit()?;
        Ok(())
    }

    async fn find_extension(&self, id: Uuid) -> ExtensionResult<Option<Extension>> {
        let connection = self.lock()?;
        let value = read_extension(&connection, id)?;
        if let Some(value) = &value {
            validate_extension_owner(&connection, value)?;
        }
        Ok(value)
    }

    async fn find_extension_by_key(&self, key: &str) -> ExtensionResult<Option<Extension>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT id FROM extension WHERE extension_key=?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        let id = parse_uuid("Extension id", &id)?;
        let value = read_extension(&connection, id)?;
        if let Some(value) = &value {
            validate_extension_owner(&connection, value)?;
        }
        Ok(value)
    }

    async fn list_extensions(&self) -> ExtensionResult<Vec<Extension>> {
        let connection = self.lock()?;
        query_ids(
            &connection,
            "SELECT id FROM extension ORDER BY extension_key,id",
            [],
        )?
        .into_iter()
        .map(|id| {
            let value =
                read_extension(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id))?;
            validate_extension_owner(&connection, &value)?;
            Ok(value)
        })
        .collect()
    }

    async fn find_manifest(&self, id: Uuid) -> ExtensionResult<Option<ExtensionManifestRecord>> {
        let connection = self.lock()?;
        let value = read_manifest(&connection, id)?;
        if let Some(value) = &value {
            require_extension(&connection, value.extension_id)?;
        }
        Ok(value)
    }

    async fn list_manifests(
        &self,
        extension_id: Uuid,
    ) -> ExtensionResult<Vec<ExtensionManifestRecord>> {
        let connection = self.lock()?;
        require_extension(&connection, extension_id)?;
        query_ids(
            &connection,
            "SELECT id FROM extension_manifest WHERE extension_id=?1 ORDER BY revision,id",
            [extension_id.to_string()],
        )?
        .into_iter()
        .map(|id| read_manifest(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id)))
        .collect()
    }

    async fn find_capability(&self, key: &str) -> ExtensionResult<Option<Capability>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT c.id FROM capability c JOIN extension e ON e.id=c.extension_id
                 WHERE c.capability_key=?1 AND c.enabled=1 AND c.manifest_id=e.current_manifest_id
                   AND e.state IN ('ENABLED','RUNNING') ORDER BY c.id LIMIT 1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        let id = parse_uuid("Capability id", &id)?;
        let value =
            read_capability(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id))?;
        validate_capability_owner(&connection, &value)?;
        Ok(Some(value))
    }

    async fn list_capabilities(&self, extension_id: Uuid) -> ExtensionResult<Vec<Capability>> {
        let connection = self.lock()?;
        require_extension(&connection, extension_id)?;
        query_ids(
            &connection,
            "SELECT id FROM capability WHERE extension_id=?1 ORDER BY capability_key,id",
            [extension_id.to_string()],
        )?
        .into_iter()
        .map(|id| {
            let value =
                read_capability(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id))?;
            validate_capability_owner(&connection, &value)?;
            Ok(value)
        })
        .collect()
    }

    async fn find_provider(&self, id: Uuid) -> ExtensionResult<Option<Provider>> {
        let connection = self.lock()?;
        let value = read_provider(&connection, id)?;
        if let Some(value) = &value {
            validate_provider_owner(&connection, value)?;
        }
        Ok(value)
    }

    async fn list_providers(&self, extension_id: Uuid) -> ExtensionResult<Vec<Provider>> {
        let connection = self.lock()?;
        require_extension(&connection, extension_id)?;
        query_ids(
            &connection,
            "SELECT id FROM provider WHERE extension_id=?1 ORDER BY priority,provider_key,id",
            [extension_id.to_string()],
        )?
        .into_iter()
        .map(|id| {
            let value =
                read_provider(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id))?;
            validate_provider_owner(&connection, &value)?;
            Ok(value)
        })
        .collect()
    }

    async fn list_states(&self, extension_id: Uuid) -> ExtensionResult<Vec<ExtensionStateRecord>> {
        let connection = self.lock()?;
        let extension = require_extension(&connection, extension_id)?;
        let values = query_ids(
            &connection,
            "SELECT id FROM extension_state WHERE extension_id=?1 ORDER BY sequence,id",
            [extension_id.to_string()],
        )?
        .into_iter()
        .map(|id| read_state(&connection, id)?.ok_or_else(|| ExtensionError::not_found(id)))
        .collect::<ExtensionResult<Vec<_>>>()?;
        validate_timeline(&extension, &values)?;
        Ok(values)
    }
}

fn validate_registration(
    connection: &Connection,
    commit: &ExtensionRegistrationCommit,
) -> ExtensionResult<()> {
    let current = read_extension(connection, commit.extension.id)?;
    match (current.as_ref(), commit.expected_extension_version) {
        (None, None)
            if commit.extension.version == 1
                && commit.extension.state == ExtensionState::Installed
                && commit.manifest.revision == 1
                && commit.state_record.from_state.is_none() => {}
        (Some(current), Some(expected)) => {
            validate_extension_update(current, &commit.extension, expected)?;
            let max_revision: i64 = connection.query_row(
                "SELECT COALESCE(MAX(revision),0) FROM extension_manifest WHERE extension_id=?1",
                [current.id.to_string()],
                |row| row.get(0),
            )?;
            if current.state != ExtensionState::Disabled
                || commit.extension.state != ExtensionState::Installed
                || commit.manifest.revision
                    != i64_u64("Manifest revision", max_revision)?.saturating_add(1)
                || commit.state_record.from_state != Some(ExtensionState::Disabled)
            {
                return Err(ExtensionError::InvalidState(
                    "invalid offline Extension upgrade".into(),
                ));
            }
        }
        _ => {
            return Err(ExtensionError::Conflict(
                "Extension registration version conflict".into(),
            ))
        }
    }
    for capability in &commit.capabilities {
        let owner: Option<String> = connection.query_row(
            "SELECT extension_id FROM capability WHERE capability_key=?1 AND extension_id<>?2 LIMIT 1",
            params![capability.key, commit.extension.id.to_string()], |row| row.get(0)).optional()?;
        if owner.is_some() {
            return Err(ExtensionError::Conflict(
                "Capability key belongs to another Extension".into(),
            ));
        }
    }
    Ok(())
}

fn write_extension(
    tx: &Transaction<'_>,
    value: &Extension,
    expected: Option<u64>,
    actor: &str,
) -> ExtensionResult<()> {
    let current = read_extension(tx, value.id)?;
    match expected {
        None if current.is_none() && value.version == 1 => {
            let now = Utc::now().to_rfc3339();
            tx.execute("INSERT INTO extension (id,extension_key,current_manifest_id,current_version,
                state,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)
                VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?11)",params![
                value.id.to_string(),value.key,value.current_manifest_id.to_string(),value.current_version,
                value.state.as_str(),u64_i64(value.version)?,serde_json::to_string(value)?,
                value.created_at.to_rfc3339(),value.updated_at.to_rfc3339(),now,actor])?;
        }
        Some(expected) => {
            let current = current.ok_or_else(|| ExtensionError::not_found(value.id))?;
            validate_extension_update(&current, value, expected)?;
            let changed = tx.execute(
                "UPDATE extension SET current_manifest_id=?1,current_version=?2,
                state=?3,version=?4,content=?5,updated_at=?6,update_time=?7,
                update_user=?8 WHERE id=?9 AND version=?10",
                params![
                    value.current_manifest_id.to_string(),
                    value.current_version,
                    value.state.as_str(),
                    u64_i64(value.version)?,
                    serde_json::to_string(value)?,
                    value.updated_at.to_rfc3339(),
                    Utc::now().to_rfc3339(),
                    actor,
                    value.id.to_string(),
                    u64_i64(expected)?
                ],
            )?;
            if changed != 1 {
                return Err(ExtensionError::Conflict("stale Extension writer".into()));
            }
        }
        _ => {
            return Err(ExtensionError::Conflict(
                "Extension insert version conflict".into(),
            ))
        }
    }
    Ok(())
}

fn insert_manifest(
    tx: &Transaction<'_>,
    value: &ExtensionManifestRecord,
    actor: &str,
) -> ExtensionResult<()> {
    value.validate()?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO extension_manifest (id,extension_id,revision,version_name,source_uri,
        checksum,content,created_at,create_time,update_time,create_user,update_user)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
        params![
            value.id.to_string(),
            value.extension_id.to_string(),
            u64_i64(value.revision)?,
            value.manifest.version,
            value.source_uri,
            value.checksum,
            serde_json::to_string(value)?,
            value.created_at.to_rfc3339(),
            now,
            actor
        ],
    )?;
    Ok(())
}

fn insert_capability(tx: &Transaction<'_>, value: &Capability, actor: &str) -> ExtensionResult<()> {
    value.validate()?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO capability (id,extension_id,manifest_id,capability_key,version_name,enabled,
        version,content,created_at,updated_at,create_time,update_time,create_user,update_user)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,?12)",
        params![
            value.id.to_string(),
            value.extension_id.to_string(),
            value.manifest_id.to_string(),
            value.key,
            value.version_name,
            bool_i64(value.enabled),
            u64_i64(value.version)?,
            serde_json::to_string(value)?,
            value.created_at.to_rfc3339(),
            value.updated_at.to_rfc3339(),
            now,
            actor
        ],
    )?;
    Ok(())
}

fn update_capability(tx: &Transaction<'_>, value: &Capability, actor: &str) -> ExtensionResult<()> {
    value.validate()?;
    let current =
        read_capability(tx, value.id)?.ok_or_else(|| ExtensionError::not_found(value.id))?;
    if current.version.saturating_add(1) != value.version
        || current.key != value.key
        || current.extension_id != value.extension_id
        || current.manifest_id != value.manifest_id
    {
        return Err(ExtensionError::Conflict(
            "Capability version conflict".into(),
        ));
    }
    let changed = tx.execute(
        "UPDATE capability SET enabled=?1,version=?2,content=?3,updated_at=?4,
        update_time=?5,update_user=?6 WHERE id=?7 AND version=?8",
        params![
            bool_i64(value.enabled),
            u64_i64(value.version)?,
            serde_json::to_string(value)?,
            value.updated_at.to_rfc3339(),
            Utc::now().to_rfc3339(),
            actor,
            value.id.to_string(),
            u64_i64(current.version)?
        ],
    )?;
    if changed != 1 {
        return Err(ExtensionError::Conflict("stale Capability writer".into()));
    }
    Ok(())
}

fn insert_provider(tx: &Transaction<'_>, value: &Provider, actor: &str) -> ExtensionResult<()> {
    value.validate()?;
    let now = Utc::now().to_rfc3339();
    tx.execute("INSERT INTO provider (id,extension_id,manifest_id,provider_key,provider_kind,priority,
        enabled,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?13,?13)",params![value.id.to_string(),
        value.extension_id.to_string(),value.manifest_id.to_string(),value.key,value.kind.as_str(),
        value.priority,bool_i64(value.enabled),u64_i64(value.version)?,serde_json::to_string(value)?,
        value.created_at.to_rfc3339(),value.updated_at.to_rfc3339(),now,actor])?;
    Ok(())
}

fn update_provider(tx: &Transaction<'_>, value: &Provider, actor: &str) -> ExtensionResult<()> {
    value.validate()?;
    let current =
        read_provider(tx, value.id)?.ok_or_else(|| ExtensionError::not_found(value.id))?;
    if current.version.saturating_add(1) != value.version
        || current.key != value.key
        || current.extension_id != value.extension_id
        || current.manifest_id != value.manifest_id
    {
        return Err(ExtensionError::Conflict("Provider version conflict".into()));
    }
    let changed = tx.execute(
        "UPDATE provider SET enabled=?1,version=?2,content=?3,updated_at=?4,
        update_time=?5,update_user=?6 WHERE id=?7 AND version=?8",
        params![
            bool_i64(value.enabled),
            u64_i64(value.version)?,
            serde_json::to_string(value)?,
            value.updated_at.to_rfc3339(),
            Utc::now().to_rfc3339(),
            actor,
            value.id.to_string(),
            u64_i64(current.version)?
        ],
    )?;
    if changed != 1 {
        return Err(ExtensionError::Conflict("stale Provider writer".into()));
    }
    Ok(())
}

fn insert_state(
    tx: &Transaction<'_>,
    value: &ExtensionStateRecord,
    actor: &str,
) -> ExtensionResult<()> {
    value.validate()?;
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO extension_state (id,extension_id,sequence,from_state,to_state,reason,
        content,created_at,create_time,update_time,create_user,update_user)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
        params![
            value.id.to_string(),
            value.extension_id.to_string(),
            u64_i64(value.sequence)?,
            value.from_state.map(ExtensionState::as_str),
            value.to_state.as_str(),
            value.reason,
            serde_json::to_string(value)?,
            value.created_at.to_rfc3339(),
            now,
            actor
        ],
    )?;
    Ok(())
}

fn read_extension(c: &Connection, id: Uuid) -> ExtensionResult<Option<Extension>> {
    let raw = c
        .query_row(
            "SELECT extension_key,current_manifest_id,current_version,state,version,content,
        created_at,updated_at,update_user FROM extension WHERE id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: Extension = serde_json::from_str(&raw.5)?;
    v.validate()?;
    if v.id != id
        || v.key != raw.0
        || v.current_manifest_id != parse_uuid("Manifest", &raw.1)?
        || v.current_version != raw.2
        || v.state.as_str() != raw.3
        || v.version != i64_u64("Extension version", raw.4)?
        || v.created_at != parse_time("Extension created", &raw.6)?
        || v.updated_at != parse_time("Extension updated", &raw.7)?
        || v.actor != raw.8
    {
        return Err(ExtensionError::Validation(
            "Extension columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}

fn read_manifest(c: &Connection, id: Uuid) -> ExtensionResult<Option<ExtensionManifestRecord>> {
    let raw = c
        .query_row(
            "SELECT extension_id,revision,version_name,source_uri,checksum,content,created_at,
        update_user FROM extension_manifest WHERE id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: ExtensionManifestRecord = serde_json::from_str(&raw.5)?;
    v.validate()?;
    if v.id != id
        || v.extension_id != parse_uuid("Manifest owner", &raw.0)?
        || v.revision != i64_u64("revision", raw.1)?
        || v.manifest.version != raw.2
        || v.source_uri != raw.3
        || v.checksum != raw.4
        || v.created_at != parse_time("Manifest created", &raw.6)?
        || v.actor != raw.7
    {
        return Err(ExtensionError::Validation(
            "Manifest columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}

fn read_capability(c: &Connection, id: Uuid) -> ExtensionResult<Option<Capability>> {
    let raw = c
        .query_row(
            "SELECT extension_id,manifest_id,capability_key,version_name,enabled,version,content,
        created_at,updated_at,update_user FROM capability WHERE id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: Capability = serde_json::from_str(&raw.6)?;
    v.validate()?;
    if v.id != id
        || v.extension_id != parse_uuid("Capability owner", &raw.0)?
        || v.manifest_id != parse_uuid("Capability Manifest", &raw.1)?
        || v.key != raw.2
        || v.version_name != raw.3
        || v.enabled != (raw.4 == 1)
        || v.version != i64_u64("Capability version", raw.5)?
        || v.created_at != parse_time("Capability created", &raw.7)?
        || v.updated_at != parse_time("Capability updated", &raw.8)?
        || v.actor != raw.9
    {
        return Err(ExtensionError::Validation(
            "Capability columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}

fn read_provider(c: &Connection, id: Uuid) -> ExtensionResult<Option<Provider>> {
    let raw = c
        .query_row(
            "SELECT extension_id,manifest_id,provider_key,provider_kind,priority,enabled,version,
        content,created_at,updated_at,update_user FROM provider WHERE id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i32>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: Provider = serde_json::from_str(&raw.7)?;
    v.validate()?;
    if v.id != id
        || v.extension_id != parse_uuid("Provider owner", &raw.0)?
        || v.manifest_id != parse_uuid("Provider Manifest", &raw.1)?
        || v.key != raw.2
        || v.kind.as_str() != raw.3
        || v.priority != raw.4
        || v.enabled != (raw.5 == 1)
        || v.version != i64_u64("Provider version", raw.6)?
        || v.created_at != parse_time("Provider created", &raw.8)?
        || v.updated_at != parse_time("Provider updated", &raw.9)?
        || v.actor != raw.10
    {
        return Err(ExtensionError::Validation(
            "Provider columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}

fn read_state(c: &Connection, id: Uuid) -> ExtensionResult<Option<ExtensionStateRecord>> {
    let raw = c
        .query_row(
            "SELECT extension_id,sequence,from_state,to_state,reason,content,created_at,update_user
        FROM extension_state WHERE id=?1",
            [id.to_string()],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: ExtensionStateRecord = serde_json::from_str(&raw.5)?;
    v.validate()?;
    if v.id != id
        || v.extension_id != parse_uuid("State owner", &raw.0)?
        || v.sequence != i64_u64("State sequence", raw.1)?
        || v.from_state.map(ExtensionState::as_str) != raw.2.as_deref()
        || v.to_state.as_str() != raw.3
        || v.reason != raw.4
        || v.created_at != parse_time("State created", &raw.6)?
        || v.actor != raw.7
    {
        return Err(ExtensionError::Validation(
            "State columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}

fn validate_extension_owner(c: &Connection, v: &Extension) -> ExtensionResult<()> {
    let m = read_manifest(c, v.current_manifest_id)?
        .ok_or_else(|| ExtensionError::not_found(v.current_manifest_id))?;
    if m.extension_id != v.id || m.manifest.key != v.key || m.manifest.version != v.current_version
    {
        return Err(ExtensionError::Validation(
            "Extension current Manifest ownership mismatch".into(),
        ));
    }
    Ok(())
}
fn validate_capability_owner(c: &Connection, v: &Capability) -> ExtensionResult<()> {
    let m =
        read_manifest(c, v.manifest_id)?.ok_or_else(|| ExtensionError::not_found(v.manifest_id))?;
    if m.extension_id != v.extension_id
        || !m
            .manifest
            .capabilities
            .iter()
            .any(|x| x.key == v.key && x.version == v.version_name)
    {
        return Err(ExtensionError::Validation(
            "Capability Manifest ownership mismatch".into(),
        ));
    }
    Ok(())
}
fn validate_provider_owner(c: &Connection, v: &Provider) -> ExtensionResult<()> {
    let m =
        read_manifest(c, v.manifest_id)?.ok_or_else(|| ExtensionError::not_found(v.manifest_id))?;
    if m.extension_id != v.extension_id
        || !m
            .manifest
            .providers
            .iter()
            .any(|x| x.key == v.key && x.kind == v.kind)
    {
        return Err(ExtensionError::Validation(
            "Provider Manifest ownership mismatch".into(),
        ));
    }
    Ok(())
}
fn require_extension(c: &Connection, id: Uuid) -> ExtensionResult<Extension> {
    read_extension(c, id)?.ok_or_else(|| ExtensionError::not_found(id))
}
fn validate_extension_update(
    current: &Extension,
    next: &Extension,
    expected: u64,
) -> ExtensionResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(ExtensionError::Conflict(
            "Extension optimistic version conflict".into(),
        ));
    }
    Ok(())
}
fn query_ids<P: rusqlite::Params>(c: &Connection, sql: &str, p: P) -> ExtensionResult<Vec<Uuid>> {
    let mut s = c.prepare(sql)?;
    let values = s
        .query_map(p, |r| r.get::<_, String>(0))?
        .map(|v| parse_uuid("row id", &v?))
        .collect();
    values
}
fn parse_uuid(label: &str, v: &str) -> ExtensionResult<Uuid> {
    Uuid::parse_str(v).map_err(|e| ExtensionError::Validation(format!("invalid {label}: {e}")))
}
fn parse_time(label: &str, v: &str) -> ExtensionResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v)
        .map(|x| x.with_timezone(&Utc))
        .map_err(|e| ExtensionError::Validation(format!("invalid {label}: {e}")))
}
fn u64_i64(v: u64) -> ExtensionResult<i64> {
    i64::try_from(v).map_err(|_| ExtensionError::Validation("integer exceeds SQLite range".into()))
}
fn i64_u64(label: &str, v: i64) -> ExtensionResult<u64> {
    u64::try_from(v).map_err(|_| ExtensionError::Validation(format!("invalid {label}")))
}
fn bool_i64(v: bool) -> i64 {
    if v {
        1
    } else {
        0
    }
}
