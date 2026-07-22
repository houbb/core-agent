use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    AutonomousGoal, AutonomousLoopState, AutonomousLoopStatus, AutonomousQuery, AutonomousSnapshot,
    validate_actor,
};
use crate::error::{AutonomousError, AutonomousResult};
use crate::infrastructure::AutonomousStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteAutonomousStore {
    connection: Mutex<Connection>,
}

impl SqliteAutonomousStore {
    pub fn new(path: impl AsRef<Path>) -> AutonomousResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> AutonomousResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> AutonomousResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> AutonomousResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AutonomousError::Internal("autonomous SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl AutonomousStore for SqliteAutonomousStore {
    async fn save_goal(&self, goal: &AutonomousGoal, actor: &str) -> AutonomousResult<()> {
        validate_actor(actor)?;
        goal.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO autonomous_goal (
                id, agent_id, description, priority, constraints, deadline,
                autonomy_level, active, metadata, version, actor, content,
                created_at, updated_at, create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?13, ?14, ?14, ?15, ?15)",
            params![
                goal.id.to_string(),
                goal.agent_id.to_string(),
                goal.description,
                i64::from(goal.priority),
                serde_json::to_string(&goal.constraints)?,
                goal.deadline.map(|d| d.to_rfc3339()),
                goal.autonomy_level.as_str(),
                i64::from(goal.active),
                serde_json::to_string(&goal.metadata)?,
                u64_i64(goal.version)?,
                goal.actor,
                serde_json::to_string(goal)?,
                goal.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_goal(&self, id: Uuid) -> AutonomousResult<Option<AutonomousGoal>> {
        let connection = self.lock()?;
        let raw: Option<(
            String, String, String, i64, String, Option<String>, String, i64, String, i64,
            String, String, String,
        )> = connection
            .query_row(
                "SELECT id, agent_id, description, priority, constraints, deadline,
                        autonomy_level, active, metadata, version, actor, content, created_at
                 FROM autonomous_goal WHERE id = ?1",
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
        let value: AutonomousGoal = serde_json::from_str(&raw.11)?;
        value.validate()?;
        Ok(Some(value))
    }

    async fn save_loop(&self, state: &AutonomousLoopState, actor: &str) -> AutonomousResult<()> {
        validate_actor(actor)?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO autonomous_loop (
                id, agent_id, status, current_cycle, last_trigger, last_trigger_at,
                autonomy_level, metadata, version, content, created_at, updated_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12, ?13, ?13)",
            params![
                state.id.to_string(),
                state.agent_id.to_string(),
                state.status.as_str(),
                u64_i64(state.current_cycle)?,
                state.last_trigger.map(|t| t.as_str()),
                state.last_trigger_at.map(|d| d.to_rfc3339()),
                state.autonomy_level.as_str(),
                serde_json::to_string(&state.metadata)?,
                u64_i64(state.version)?,
                serde_json::to_string(state)?,
                state.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_loop(&self, agent_id: Uuid) -> AutonomousResult<Option<AutonomousLoopState>> {
        let connection = self.lock()?;
        let raw: Option<(
            String, String, String, i64, Option<String>, Option<String>, String, String, i64,
            String, String, String,
        )> = connection
            .query_row(
                "SELECT id, agent_id, status, current_cycle, last_trigger, last_trigger_at,
                        autonomy_level, metadata, version, content, created_at, updated_at
                 FROM autonomous_loop WHERE agent_id = ?1",
                [agent_id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: AutonomousLoopState = serde_json::from_str(&raw.9)?;
        Ok(Some(value))
    }

    async fn list_goals(&self, query: &AutonomousQuery) -> AutonomousResult<Vec<AutonomousGoal>> {
        let ids = {
            let connection = self.lock()?;
            list_goal_ids_sync(&connection, query)?
        };
        let mut goals = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| AutonomousError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(goal) = self.find_goal(uuid).await? {
                goals.push(goal);
            }
        }
        Ok(goals)
    }

    async fn snapshot(&self, agent_id: Uuid) -> AutonomousResult<AutonomousSnapshot> {
        let connection = self.lock()?;
        let active: i64 = connection.query_row(
            "SELECT COUNT(*) FROM autonomous_goal WHERE agent_id = ?1 AND active = 1",
            [agent_id.to_string()],
            |row| row.get(0),
        )?;
        let (cycle, status, level): (i64, String, String) = connection
            .query_row(
                "SELECT COALESCE(current_cycle, 0), COALESCE(status, 'IDLE'), COALESCE(autonomy_level, 'L0_SUGGEST')
                 FROM autonomous_loop WHERE agent_id = ?1",
                [agent_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or((0, "IDLE".into(), "L0_SUGGEST".into()));

        Ok(AutonomousSnapshot {
            agent_id,
            total_cycles: cycle as u64,
            current_status: status,
            autonomy_level: level,
            active_goals: active as u64,
        })
    }
}

fn u64_i64(value: u64) -> AutonomousResult<i64> {
    i64::try_from(value)
        .map_err(|_| AutonomousError::Validation("integer exceeds SQLite range".into()))
}

fn list_goal_ids_sync(connection: &Connection, query: &AutonomousQuery) -> AutonomousResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM autonomous_goal");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(agent_id) = &query.agent_id {
        clauses.push(format!("agent_id = '{}'", agent_id));
    }
    if let Some(autonomy_level) = &query.autonomy_level {
        clauses.push(format!("autonomy_level = '{}'", autonomy_level.as_str()));
    }
    if let Some(active) = &query.active {
        clauses.push(format!("active = {}", i64::from(*active)));
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    sql.push_str(&format!(
        " ORDER BY priority DESC, id LIMIT {} OFFSET {}",
        query.limit, query.offset
    ));
    let mut statement = connection.prepare(&sql)?;
    let result: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}