use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    AgentRegistration, AgentStatus, NetworkQuery, NetworkSnapshot, validate_actor,
};
use crate::error::{NetworkError, NetworkResult};
use crate::infrastructure::NetworkStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteNetworkStore {
    connection: Mutex<Connection>,
}

impl SqliteNetworkStore {
    pub fn new(path: impl AsRef<Path>) -> NetworkResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> NetworkResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> NetworkResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> NetworkResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| NetworkError::Internal("network SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl NetworkStore for SqliteNetworkStore {
    async fn register(&self, reg: &AgentRegistration, actor: &str) -> NetworkResult<()> {
        validate_actor(actor)?;
        reg.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM agent_registration WHERE id = ?1",
                [reg.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(NetworkError::Conflict("registration already exists".into()));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO agent_registration (
                id, agent_id, name, capabilities, status, trust_level, endpoint,
                reputation, metadata, version, actor, content, created_at, updated_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?13, ?14, ?14, ?15, ?15)",
            params![
                reg.id.to_string(),
                reg.agent_id.to_string(),
                reg.name,
                serde_json::to_string(&reg.capabilities)?,
                reg.status.as_str(),
                reg.trust_level.as_str(),
                reg.endpoint,
                reg.reputation,
                serde_json::to_string(&reg.metadata)?,
                u64_i64(reg.version)?,
                reg.actor,
                serde_json::to_string(reg)?,
                reg.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> NetworkResult<Option<AgentRegistration>> {
        let connection = self.lock()?;
        let raw: Option<(
            String, String, String, String, String, String, Option<String>, f64,
            String, i64, String, String, String,
        )> = connection
            .query_row(
                "SELECT id, agent_id, name, capabilities, status, trust_level, endpoint,
                        reputation, metadata, version, actor, content, created_at
                 FROM agent_registration WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: AgentRegistration = serde_json::from_str(&raw.11)?;
        value.validate()?;
        Ok(Some(value))
    }

    async fn find_by_agent(&self, agent_id: Uuid) -> NetworkResult<Option<AgentRegistration>> {
        let id: Option<String> = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT id FROM agent_registration WHERE agent_id = ?1",
                    [agent_id.to_string()],
                    |row| row.get(0),
                )
                .optional()?
        };
        match id {
            Some(id_str) => {
                let uuid = Uuid::parse_str(&id_str)
                    .map_err(|e| NetworkError::Validation(format!("invalid uuid: {e}")))?;
                self.find(uuid).await
            }
            None => Ok(None),
        }
    }

    async fn list(&self, query: &NetworkQuery) -> NetworkResult<Vec<AgentRegistration>> {
        let ids = {
            let connection = self.lock()?;
            list_ids_sync(&connection, query)?
        };
        let mut agents = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| NetworkError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(agent) = self.find(uuid).await? {
                agents.push(agent);
            }
        }
        Ok(agents)
    }

    async fn snapshot(&self) -> NetworkResult<NetworkSnapshot> {
        let connection = self.lock()?;
        let total: i64 =
            connection.query_row("SELECT COUNT(*) FROM agent_registration", [], |row| {
                row.get(0)
            })?;
        let online: i64 = connection.query_row(
            "SELECT COUNT(*) FROM agent_registration WHERE status IN ('ONLINE', 'BUSY')",
            [],
            |row| row.get(0),
        )?;
        let avg_rep: f64 = connection.query_row(
            "SELECT COALESCE(AVG(reputation), 0.0) FROM agent_registration",
            [],
            |row| row.get(0),
        )?;

        Ok(NetworkSnapshot {
            total_agents: total as u64,
            online_count: online as u64,
            by_capability: std::collections::BTreeMap::new(),
            avg_reputation: (avg_rep * 100.0).round() / 100.0,
        })
    }
}

fn u64_i64(value: u64) -> NetworkResult<i64> {
    i64::try_from(value)
        .map_err(|_| NetworkError::Validation("integer exceeds SQLite range".into()))
}

fn list_ids_sync(connection: &Connection, query: &NetworkQuery) -> NetworkResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM agent_registration");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(status) = &query.status {
        clauses.push(format!("status = '{}'", status.as_str()));
    }
    if let Some(trust_level) = &query.trust_level {
        clauses.push(format!("trust_level = '{}'", trust_level.as_str()));
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    sql.push_str(&format!(
        " ORDER BY reputation DESC, id LIMIT {} OFFSET {}",
        query.limit, query.offset
    ));
    let mut statement = connection.prepare(&sql)?;
    let result: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}