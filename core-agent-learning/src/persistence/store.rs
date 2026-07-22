use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    LearningQuery, LearningRecord, LearningSnapshot, LearningStatus, validate_actor,
};
use crate::error::{LearningError, LearningResult};
use crate::infrastructure::LearningStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteLearningStore {
    connection: Mutex<Connection>,
}

impl SqliteLearningStore {
    pub fn new(path: impl AsRef<Path>) -> LearningResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> LearningResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> LearningResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> LearningResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| LearningError::Internal("learning SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl LearningStore for SqliteLearningStore {
    async fn record(&self, record: &LearningRecord, actor: &str) -> LearningResult<()> {
        validate_actor(actor)?;
        record.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM learning_record WHERE id = ?1",
                [record.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(LearningError::Conflict(
                "learning record already exists".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO learning_record (
                id, agent_id, source, learning_type, status, title, description,
                experience, improvement, confidence, source_id, metadata,
                version, actor, content, created_at, updated_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?14, ?15, ?16, ?16, ?17, ?17, ?18, ?18)",
            params![
                record.id.to_string(),
                record.agent_id.to_string(),
                record.source.as_str(),
                record.learning_type.as_str(),
                record.status.as_str(),
                record.title,
                record.description,
                serde_json::to_string(&record.experience)?,
                serde_json::to_string(&record.improvement)?,
                record.confidence,
                record.source_id.map(|id| id.to_string()),
                serde_json::to_string(&record.metadata)?,
                u64_i64(record.version)?,
                record.actor,
                serde_json::to_string(record)?,
                record.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn update(&self, record: &LearningRecord, actor: &str) -> LearningResult<()> {
        validate_actor(actor)?;
        record.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = connection.execute(
            "UPDATE learning_record SET status = ?1, confidence = ?2, version = ?3, actor = ?4, content = ?5, updated_at = ?6, update_time = ?7, update_user = ?8 WHERE id = ?9",
            rusqlite::params![
                record.status.as_str(),
                record.confidence,
                u64_i64(record.version)?,
                record.actor,
                serde_json::to_string(record)?,
                record.updated_at.to_rfc3339(),
                now,
                actor,
                record.id.to_string(),
            ],
        )?;
        if rows == 0 {
            return Err(LearningError::NotFound(record.id.to_string()));
        }
        Ok(())
    }

    async fn find(&self, id: Uuid) -> LearningResult<Option<LearningRecord>> {
        let connection = self.lock()?;
        let raw: Option<(String, String, String, String, String, String, String, String, String, f64, Option<String>, String, i64, String, String, String)> = connection
            .query_row(
                "SELECT id, agent_id, source, learning_type, status, title, description,
                        experience, improvement, confidence, source_id, metadata,
                        version, actor, content, created_at
                 FROM learning_record WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                        row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: LearningRecord = serde_json::from_str(&raw.14)?;
        value.validate()?;
        Ok(Some(value))
    }

    async fn list(&self, query: &LearningQuery) -> LearningResult<Vec<LearningRecord>> {
        query.validate()?;
        let ids = {
            let connection = self.lock()?;
            list_ids_sync(&connection, query)?
        };
        let mut records = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| LearningError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(record) = self.find(uuid).await? {
                records.push(record);
            }
        }
        Ok(records)
    }

    async fn count(&self, query: &LearningQuery) -> LearningResult<u64> {
        query.validate()?;
        let connection = self.lock()?;
        let mut sql = String::from("SELECT COUNT(*) FROM learning_record");
        let mut clauses: Vec<String> = Vec::new();
        if let Some(agent_id) = &query.agent_id {
            clauses.push(format!("agent_id = '{}'", agent_id));
        }
        if let Some(learning_type) = &query.learning_type {
            clauses.push(format!("learning_type = '{}'", learning_type.as_str()));
        }
        if let Some(status) = &query.status {
            clauses.push(format!("status = '{}'", status.as_str()));
        }
        if !clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
        }
        let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as u64)
    }

    async fn snapshot(&self, agent_id: Uuid) -> LearningResult<LearningSnapshot> {
        let connection = self.lock()?;
        let total: i64 = connection.query_row(
            "SELECT COUNT(*) FROM learning_record WHERE agent_id = ?1",
            [agent_id.to_string()],
            |row| row.get(0),
        )?;
        let applied: i64 = connection.query_row(
            "SELECT COUNT(*) FROM learning_record WHERE agent_id = ?1 AND status = 'APPLIED'",
            [agent_id.to_string()],
            |row| row.get(0),
        )?;
        let avg_conf: f64 = connection
            .query_row(
                "SELECT COALESCE(AVG(confidence), 0.0) FROM learning_record WHERE agent_id = ?1",
                [agent_id.to_string()],
                |row| row.get(0),
            )?;

        let mut by_type = std::collections::BTreeMap::new();
        let mut statement =
            connection.prepare("SELECT learning_type, COUNT(*) FROM learning_record WHERE agent_id = ?1 GROUP BY learning_type")?;
        let rows = statement.query_map([agent_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        for row in rows {
            let (k, v) = row?;
            by_type.insert(k, v);
        }

        let mut by_status = std::collections::BTreeMap::new();
        let mut statement =
            connection.prepare("SELECT status, COUNT(*) FROM learning_record WHERE agent_id = ?1 GROUP BY status")?;
        let rows = statement.query_map([agent_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        for row in rows {
            let (k, v) = row?;
            by_status.insert(k, v);
        }

        Ok(LearningSnapshot {
            agent_id,
            total_records: total as u64,
            by_type,
            by_status,
            avg_confidence: (avg_conf * 100.0).round() / 100.0,
            applied_count: applied as u64,
        })
    }
}

fn u64_i64(value: u64) -> LearningResult<i64> {
    i64::try_from(value)
        .map_err(|_| LearningError::Validation("integer exceeds SQLite range".into()))
}

fn list_ids_sync(connection: &Connection, query: &LearningQuery) -> LearningResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM learning_record");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(agent_id) = &query.agent_id {
        clauses.push(format!("agent_id = '{}'", agent_id));
    }
    if let Some(learning_type) = &query.learning_type {
        clauses.push(format!("learning_type = '{}'", learning_type.as_str()));
    }
    if let Some(status) = &query.status {
        clauses.push(format!("status = '{}'", status.as_str()));
    }
    if let Some(source) = &query.source {
        clauses.push(format!("source = '{}'", source.as_str()));
    }
    if let Some(from) = &query.from {
        clauses.push(format!("created_at >= '{}'", from.to_rfc3339()));
    }
    if let Some(to) = &query.to {
        clauses.push(format!("created_at <= '{}'", to.to_rfc3339()));
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    sql.push_str(&format!(
        " ORDER BY created_at DESC, id LIMIT {} OFFSET {}",
        query.limit, query.offset
    ));
    let mut statement = connection.prepare(&sql)?;
    let result: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}