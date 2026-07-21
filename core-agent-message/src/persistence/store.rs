//! P2 message SQLite store. Follows standard P<N> store pattern.

use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use rusqlite::Connection;
use uuid::Uuid;

use crate::domain::{AgentMessage, MessageStatus};
use crate::error::{MessageError, MessageResult};
use crate::infrastructure::MessageStore;

pub struct SqliteMessageStore {
    conn: Mutex<Connection>,
}

impl SqliteMessageStore {
    pub fn new(path: impl AsRef<Path>) -> MessageResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA journal_mode = WAL;")?;
        conn.execute_batch(crate::persistence::schema::SCHEMA_SQL)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl MessageStore for SqliteMessageStore {
    async fn save(
        &self,
        message: &AgentMessage,
        expected_version: Option<u64>,
        actor: &str,
    ) -> MessageResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let content = serde_json::to_string(message)?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let id = message.id.to_string();
        let from = message.from_agent_id.to_string();
        let to = message.to_agent_id.to_string();
        let correlation = message.correlation_id.map(|v| v.to_string());
        let msg_type = message.message_type.as_str().to_string();
        let intent = &message.intent;
        let priority = message.priority.as_str().to_string();
        let status = message.status.as_str().to_string();
        let version = message.version;

        let existing: Option<u64> = conn
            .query_row("SELECT version FROM agent_message WHERE id = ?1", [&id], |row| row.get(0))
            .optional()?;

        match existing {
            None => {
                conn.execute(
                    "INSERT INTO agent_message (id, from_agent_id, to_agent_id, correlation_id, message_type, intent, priority, status, version, content, created_at, updated_at, create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?11, ?11, ?12, ?12)",
                    rusqlite::params![id, from, to, correlation, msg_type, intent, priority, status, version, content, now, actor],
                )?;
            }
            Some(current_version) => {
                if let Some(expected) = expected_version {
                    if current_version != expected {
                        return Err(MessageError::Conflict("message version conflict".into()));
                    }
                }
                conn.execute(
                    "UPDATE agent_message SET from_agent_id=?2, to_agent_id=?3, correlation_id=?4, message_type=?5, intent=?6, priority=?7, status=?8, version=?9, content=?10, updated_at=?11, update_user=?12 WHERE id=?1",
                    rusqlite::params![id, from, to, correlation, msg_type, intent, priority, status, version, content, now, actor],
                )?;
            }
        }
        Ok(())
    }

    async fn find(&self, id: Uuid) -> MessageResult<Option<AgentMessage>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let id = id.to_string();
        let mut stmt = conn.prepare("SELECT content FROM agent_message WHERE id = ?1")?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str::<AgentMessage>(&{
                let content: String = row.get(0)?;
                content
            })?)),
            None => Ok(None),
        }
    }

    async fn list_by_to_agent(&self, agent_id: Uuid, limit: usize) -> MessageResult<Vec<AgentMessage>> {
        let conn = self.conn.lock().map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let agent = agent_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_message WHERE to_agent_id = ?1 ORDER BY created_at DESC, id LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![agent, limit as i64], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<AgentMessage>(&row.unwrap()).map_err(|e| {
                MessageError::Serialization(e)
            })?);
        }
        Ok(values)
    }

    async fn list_by_from_agent(&self, agent_id: Uuid, limit: usize) -> MessageResult<Vec<AgentMessage>> {
        let conn = self.conn.lock().map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let agent = agent_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_message WHERE from_agent_id = ?1 ORDER BY created_at DESC, id LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![agent, limit as i64], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<AgentMessage>(&row.unwrap())?);
        }
        Ok(values)
    }

    async fn list_by_correlation(&self, correlation_id: Uuid) -> MessageResult<Vec<AgentMessage>> {
        let conn = self.conn.lock().map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let correlation = correlation_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_message WHERE correlation_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map(rusqlite::params![correlation], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<AgentMessage>(&row.unwrap())?);
        }
        Ok(values)
    }

    async fn mark_read(&self, message_id: Uuid, actor: &str) -> MessageResult<bool> {
        let conn = self.conn.lock().map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let id = message_id.to_string();
        let existing: Option<String> = conn
            .query_row("SELECT content FROM agent_message WHERE id = ?1", [&id], |row| row.get(0))
            .optional()?;
        match existing {
            Some(content) => {
                let mut msg = serde_json::from_str::<AgentMessage>(&content)?;
                if msg.status == MessageStatus::Read {
                    return Ok(true);
                }
                msg.status = MessageStatus::Read;
                msg.version = msg.version.saturating_add(1);
                msg.updated_at = chrono::Utc::now();
                let new_content = serde_json::to_string(&msg)?;
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                conn.execute(
                    "UPDATE agent_message SET status=?2, version=?3, content=?4, updated_at=?5, update_user=?6 WHERE id=?1",
                    rusqlite::params![id, MessageStatus::Read.as_str().to_string(), msg.version, new_content, now, actor],
                )?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn list_inbox(&self, agent_id: Uuid, limit: usize) -> MessageResult<Vec<AgentMessage>> {
        let conn = self.conn.lock().map_err(|_| MessageError::Internal("SQLite lock poisoned".into()))?;
        let agent = agent_id.to_string();
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_message WHERE to_agent_id = ?1 AND (status = 'PENDING' OR status = 'DELIVERED') ORDER BY CASE priority WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 WHEN 'NORMAL' THEN 2 ELSE 3 END, created_at DESC, id LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![agent, limit as i64], |row| {
            let content: String = row.get(0)?;
            Ok(content)
        })?;
        let mut values = Vec::new();
        for row in rows {
            values.push(serde_json::from_str::<AgentMessage>(&row.unwrap())?);
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