use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    Evaluation, EvaluationQuery, EvaluationSnapshot, validate_actor,
};
use crate::error::{EvaluationError, EvaluationResult};
use crate::infrastructure::EvaluationStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteEvaluationStore {
    connection: Mutex<Connection>,
}

impl SqliteEvaluationStore {
    pub fn new(path: impl AsRef<Path>) -> EvaluationResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> EvaluationResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> EvaluationResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> EvaluationResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| EvaluationError::Internal("evaluation SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl EvaluationStore for SqliteEvaluationStore {
    async fn record(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()> {
        validate_actor("evaluation writer", actor)?;
        evaluation.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM evaluation WHERE id = ?1",
                [evaluation.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(EvaluationError::Conflict(
                "evaluation already exists".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO evaluation (
                id, agent_id, task_id, execution_id, criteria, feedback,
                total_score, passed, metadata, evaluator, version, content,
                created_at, create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?14, ?14, ?15, ?15)",
            params![
                evaluation.id.to_string(),
                evaluation.agent_id.to_string(),
                evaluation.task_id.to_string(),
                evaluation.execution_id.to_string(),
                serde_json::to_string(&evaluation.criteria)?,
                serde_json::to_string(&evaluation.feedback)?,
                i64::from(evaluation.total_score.get()),
                i64::from(evaluation.passed),
                serde_json::to_string(&evaluation.metadata)?,
                evaluation.evaluator,
                u64_i64(evaluation.version)?,
                serde_json::to_string(evaluation)?,
                evaluation.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn update(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()> {
        validate_actor("evaluation writer", actor)?;
        evaluation.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let rows = connection.execute(
            "UPDATE evaluation SET feedback = ?1, total_score = ?2, passed = ?3, version = ?4, content = ?5, update_time = ?6, update_user = ?7 WHERE id = ?8",
            params![
                serde_json::to_string(&evaluation.feedback)?,
                i64::from(evaluation.total_score.get()),
                i64::from(evaluation.passed),
                u64_i64(evaluation.version)?,
                serde_json::to_string(evaluation)?,
                now,
                actor,
                evaluation.id.to_string(),
            ],
        )?;
        if rows == 0 {
            return Err(EvaluationError::NotFound(evaluation.id.to_string()));
        }
        Ok(())
    }

    async fn find(&self, id: Uuid) -> EvaluationResult<Option<Evaluation>> {
        let connection = self.lock()?;
        let raw: Option<(String, String, String, String, String, String, i64, i64, String, String, i64, String, String)> = connection
            .query_row(
                "SELECT id, agent_id, task_id, execution_id, criteria, feedback,
                        total_score, passed, metadata, evaluator, version, content, created_at
                 FROM evaluation WHERE id = ?1",
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
        let value: Evaluation = serde_json::from_str(&raw.11)?;
        value.validate()?;
        if raw.0 != value.id.to_string()
            || raw.1 != value.agent_id.to_string()
            || raw.9 != value.evaluator
        {
            return Err(EvaluationError::Validation(
                "evaluation columns do not match serialized content".into(),
            ));
        }
        Ok(Some(value))
    }

    async fn list(&self, query: &EvaluationQuery) -> EvaluationResult<Vec<Evaluation>> {
        query.validate()?;
        let ids = {
            let connection = self.lock()?;
            list_ids_sync(&connection, query)?
        };
        let mut evals = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| EvaluationError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(eval) = self.find(uuid).await? {
                evals.push(eval);
            }
        }
        Ok(evals)
    }

    async fn count(&self, query: &EvaluationQuery) -> EvaluationResult<u64> {
        query.validate()?;
        let connection = self.lock()?;
        let mut sql = String::from("SELECT COUNT(*) FROM evaluation");
        let mut clauses: Vec<String> = Vec::new();
        if let Some(agent_id) = &query.agent_id {
            clauses.push(format!("agent_id = '{}'", agent_id));
        }
        if let Some(task_id) = &query.task_id {
            clauses.push(format!("task_id = '{}'", task_id));
        }
        if let Some(passed) = &query.passed {
            clauses.push(format!("passed = {}", i64::from(*passed)));
        }
        if let Some(evaluator) = &query.evaluator {
            clauses.push(format!(
                "evaluator = '{}'",
                evaluator.replace('\'', "''")
            ));
        }
        if !clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
        }
        let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as u64)
    }

    async fn snapshot(&self, agent_id: Uuid) -> EvaluationResult<EvaluationSnapshot> {
        let connection = self.lock()?;
        let total: i64 = connection.query_row(
            "SELECT COUNT(*) FROM evaluation WHERE agent_id = ?1",
            [agent_id.to_string()],
            |row| row.get(0),
        )?;
        let passed: i64 = connection.query_row(
            "SELECT COUNT(*) FROM evaluation WHERE agent_id = ?1 AND passed = 1",
            [agent_id.to_string()],
            |row| row.get(0),
        )?;
        let avg: f64 = connection
            .query_row(
                "SELECT COALESCE(AVG(CAST(total_score AS REAL)), 0.0) FROM evaluation WHERE agent_id = ?1",
                [agent_id.to_string()],
                |row| row.get(0),
            )?;

        Ok(EvaluationSnapshot {
            agent_id,
            total_evaluations: total as u64,
            passed_count: passed as u64,
            average_score: (avg * 100.0).round() / 100.0,
            by_dimension: std::collections::BTreeMap::new(),
            from: None,
            to: None,
        })
    }
}

fn u64_i64(value: u64) -> EvaluationResult<i64> {
    i64::try_from(value)
        .map_err(|_| EvaluationError::Validation("integer exceeds SQLite range".into()))
}

fn list_ids_sync(connection: &Connection, query: &EvaluationQuery) -> EvaluationResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM evaluation");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(agent_id) = &query.agent_id {
        clauses.push(format!("agent_id = '{}'", agent_id));
    }
    if let Some(task_id) = &query.task_id {
        clauses.push(format!("task_id = '{}'", task_id));
    }
    if let Some(execution_id) = &query.execution_id {
        clauses.push(format!("execution_id = '{}'", execution_id));
    }
    if let Some(passed) = &query.passed {
        clauses.push(format!("passed = {}", i64::from(*passed)));
    }
    if let Some(evaluator) = &query.evaluator {
        clauses.push(format!(
            "evaluator = '{}'",
            evaluator.replace('\'', "''")
        ));
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