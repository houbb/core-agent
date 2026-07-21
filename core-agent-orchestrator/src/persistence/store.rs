//! P2 orchestration SQLite store.

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use rusqlite::Connection;
use uuid::Uuid;

use crate::domain::{Orchestration, OrchestrationStatus};
use crate::error::{OrchestratorError, OrchestratorResult};
use crate::infrastructure::OrchestrationStore;

pub struct SqliteOrchestrationStore {
    conn: Mutex<Connection>,
}

impl SqliteOrchestrationStore {
    pub fn new(path: impl AsRef<Path>) -> OrchestratorResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(crate::persistence::schema::SCHEMA_SQL)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl OrchestrationStore for SqliteOrchestrationStore {
    async fn save(
        &self,
        orchestration: &Orchestration,
        expected_version: Option<u64>,
        actor: &str,
    ) -> OrchestratorResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| OrchestratorError::Internal("SQLite lock poisoned".into()))?;
        let content = serde_json::to_string(orchestration)?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let id = orchestration.id.to_string();
        let goal = &orchestration.goal;
        let supervisor = orchestration.supervisor_agent_id.to_string();
        let strategy = orchestration.strategy.as_str().to_string();
        let status = orchestration.status.as_str().to_string();
        let version = orchestration.version;

        let existing: Option<u64> = conn
            .query_row("SELECT version FROM orchestration WHERE id = ?1", [&id], |row| row.get(0))
            .optional()?;

        match existing {
            None => {
                conn.execute(
                    "INSERT INTO orchestration (id, goal, supervisor_agent_id, strategy, status, version, content, created_at, updated_at, create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?8, ?8, ?9, ?9)",
                    rusqlite::params![id, goal, supervisor, strategy, status, version, content, now, actor],
                )?;
            }
            Some(current_version) => {
                if let Some(expected) = expected_version {
                    if current_version != expected {
                        return Err(OrchestratorError::Conflict(
                            "orchestration version conflict".into(),
                        ));
                    }
                }
                conn.execute(
                    "UPDATE orchestration SET goal=?2, supervisor_agent_id=?3, strategy=?4, status=?5, version=?6, content=?7, updated_at=?8, update_user=?9 WHERE id=?1",
                    rusqlite::params![id, goal, supervisor, strategy, status, version, content, now, actor],
                )?;
            }
        }
        Ok(())
    }

    async fn find(&self, id: Uuid) -> OrchestratorResult<Option<Orchestration>> {
        let conn = self.conn.lock().map_err(|_| OrchestratorError::Internal("SQLite lock poisoned".into()))?;
        let id = id.to_string();
        let mut stmt = conn.prepare("SELECT content FROM orchestration WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str::<Orchestration>(&{
                let content: String = row.get(0)?;
                content
            })?)),
            None => Ok(None),
        }
    }

    async fn list_by_supervisor(&self, supervisor_id: Uuid) -> OrchestratorResult<Vec<Orchestration>> {
        let conn = self.conn.lock().map_err(|_| OrchestratorError::Internal("SQLite lock poisoned".into()))?;
        let supervisor = supervisor_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM orchestration WHERE supervisor_agent_id = ?1 ORDER BY created_at DESC, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![supervisor], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<Orchestration>(&row.unwrap())?);
        }
        Ok(values)
    }

    async fn list_by_status(&self, status: OrchestrationStatus) -> OrchestratorResult<Vec<Orchestration>> {
        let conn = self.conn.lock().map_err(|_| OrchestratorError::Internal("SQLite lock poisoned".into()))?;
        let status_str = status.as_str().to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM orchestration WHERE status = ?1 ORDER BY updated_at DESC, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![status_str], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<Orchestration>(&row.unwrap())?);
        }
        Ok(values)
    }

    async fn list_all(&self) -> OrchestratorResult<Vec<Orchestration>> {
        let conn = self.conn.lock().map_err(|_| OrchestratorError::Internal("SQLite lock poisoned".into()))?;
        let mut stmt = conn.prepare("SELECT content FROM orchestration ORDER BY created_at DESC, id")?;
        let rows = stmt.query_map([], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<Orchestration>(&row.unwrap())?);
        }
        Ok(values)
    }
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