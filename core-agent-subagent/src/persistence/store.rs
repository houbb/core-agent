use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use rusqlite::Connection;
use uuid::Uuid;

use crate::domain::{AgentInstance, SubAgentStatus};
use crate::error::{SubAgentError, SubAgentResult};
use crate::infrastructure::SubAgentStore;

pub struct SqliteSubAgentStore {
    conn: Mutex<Connection>,
}

impl SqliteSubAgentStore {
    pub fn new(path: impl AsRef<Path>) -> SubAgentResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(crate::persistence::schema::SCHEMA_SQL)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl SubAgentStore for SqliteSubAgentStore {
    async fn save(
        &self,
        instance: &AgentInstance,
        expected_version: Option<u64>,
        actor: &str,
    ) -> SubAgentResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let content = serde_json::to_string(instance)?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let id = uuid_text(instance.id);
        let parent = instance.parent_agent_id.map(uuid_text);
        let supervisor = instance.supervisor_agent_id.map(uuid_text);

        let existing: Option<u64> = conn
            .query_row("SELECT version FROM agent_instance WHERE id = ?1", [&id], |row| {
                row.get(0)
            })
            .optional()?;

        match existing {
            None => {
                conn.execute(
                    "INSERT INTO agent_instance (id, name, instance_type, role, parent_agent_id, supervisor_agent_id, status, version, content, created_at, updated_at, create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?10, ?10, ?11, ?11)",
                    rusqlite::params![
                        id, instance.name, instance.instance_type.as_str(), instance.role.as_str(),
                        parent, supervisor, instance.status.as_str(), instance.version,
                        content, now, actor,
                    ],
                )?;
            }
            Some(current_version) => {
                if let Some(expected) = expected_version {
                    if current_version != expected {
                        return Err(SubAgentError::Conflict(
                            "subagent version conflict".into(),
                        ));
                    }
                }
                conn.execute(
                    "UPDATE agent_instance SET name=?2, instance_type=?3, role=?4, parent_agent_id=?5, supervisor_agent_id=?6, status=?7, version=?8, content=?9, updated_at=?10, update_user=?11 WHERE id=?1",
                    rusqlite::params![
                        id, instance.name, instance.instance_type.as_str(), instance.role.as_str(),
                        parent, supervisor, instance.status.as_str(), instance.version,
                        content, now, actor,
                    ],
                )?;
            }
        }
        Ok(())
    }

    async fn find(&self, id: Uuid) -> SubAgentResult<Option<AgentInstance>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let id = uuid_text(id);
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_instance WHERE id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => {
                let content: String = row.get(0)?;
                Ok(Some(serde_json::from_str(&content)?))
            }
            None => Ok(None),
        }
    }

    async fn list_by_parent(&self, parent_id: Uuid) -> SubAgentResult<Vec<AgentInstance>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let parent = uuid_text(parent_id);
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_instance WHERE parent_agent_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![parent], |row| {
            let content: String = row.get(0)?;
            Ok(serde_json::from_str::<AgentInstance>(&content).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            })?)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(|_| {
                SubAgentError::Internal("failed to parse agent_instance from SQLite".into())
            })?);
        }
        Ok(values)
    }

    async fn list_by_supervisor(&self, supervisor_id: Uuid) -> SubAgentResult<Vec<AgentInstance>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let supervisor = uuid_text(supervisor_id);
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_instance WHERE supervisor_agent_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![supervisor], |row| {
            let content: String = row.get(0)?;
            Ok(serde_json::from_str::<AgentInstance>(&content).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            })?)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(|_| {
                SubAgentError::Internal("failed to parse agent_instance from SQLite".into())
            })?);
        }
        Ok(values)
    }

    async fn list_by_status(&self, status: SubAgentStatus) -> SubAgentResult<Vec<AgentInstance>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let status_str = status.as_str().to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_instance WHERE status = ?1 ORDER BY updated_at DESC, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![status_str], |row| {
            let content: String = row.get(0)?;
            Ok(serde_json::from_str::<AgentInstance>(&content).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            })?)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(|_| {
                SubAgentError::Internal("failed to parse agent_instance from SQLite".into())
            })?);
        }
        Ok(values)
    }

    async fn list_all(&self) -> SubAgentResult<Vec<AgentInstance>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| SubAgentError::Internal("SQLite lock poisoned".into()))?;
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_instance ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], |row| {
            let content: String = row.get(0)?;
            Ok(serde_json::from_str::<AgentInstance>(&content).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(e))
            })?)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(row.map_err(|_| {
                SubAgentError::Internal("failed to parse agent_instance from SQLite".into())
            })?);
        }
        Ok(values)
    }
}

fn uuid_text(id: Uuid) -> String {
    id.to_string()
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }
}