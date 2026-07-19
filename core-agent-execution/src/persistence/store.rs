use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use uuid::Uuid;

use crate::domain::{
    Execution, ExecutionCheckpoint, ExecutionStateRecord, ExecutionStatus, RetryRecord,
    RetryStatus, RollbackRecord, RollbackStatus,
};
use crate::error::{ExecutionError, ExecutionResult};
use crate::infrastructure::{ExecutionCommit, ExecutionStore};

use super::schema::SCHEMA_SQL;

pub struct SqliteExecutionStore {
    connection: Mutex<Connection>,
}

impl SqliteExecutionStore {
    pub fn new(path: impl AsRef<Path>) -> ExecutionResult<Self> {
        let connection = if path.as_ref() == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA journal_mode = WAL;")?;
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> ExecutionResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| ExecutionError::Internal("execution database lock poisoned".into()))
    }
}

#[async_trait]
impl ExecutionStore for SqliteExecutionStore {
    async fn commit(&self, commit: &ExecutionCommit, actor: &str) -> ExecutionResult<()> {
        commit.validate(actor)?;

        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        write_execution(&transaction, commit, actor)?;
        if let Some(value) = &commit.state {
            insert_state(&transaction, value, actor)?;
        }
        if let Some(value) = &commit.checkpoint {
            insert_checkpoint(&transaction, value, actor)?;
        }
        if let Some(value) = &commit.retry {
            insert_retry(&transaction, value, actor)?;
        }
        if let Some(value) = &commit.rollback {
            insert_rollback(&transaction, value, actor)?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn find_execution(&self, id: Uuid) -> ExecutionResult<Option<Execution>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT plan_id, plan_version, plan_hash, status, version, current_task_id, current_step_id, content, created_at, updated_at FROM execution WHERE id = ?1",
                [id.to_string()],
                parse_execution,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_executions(&self, plan_id: Uuid) -> ExecutionResult<Vec<Execution>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT plan_id, plan_version, plan_hash, status, version, current_task_id, current_step_id, content, created_at, updated_at FROM execution WHERE plan_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([plan_id.to_string()], parse_execution)?;
        collect_rows(rows)
    }

    async fn list_checkpoints(
        &self,
        execution_id: Uuid,
    ) -> ExecutionResult<Vec<ExecutionCheckpoint>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT execution_id, sequence, label, hash, content, created_at FROM checkpoint WHERE execution_id = ?1 ORDER BY sequence, created_at, id",
        )?;
        let rows = statement.query_map([execution_id.to_string()], parse_checkpoint)?;
        collect_rows(rows)
    }

    async fn find_checkpoint(&self, id: Uuid) -> ExecutionResult<Option<ExecutionCheckpoint>> {
        let connection = self.lock()?;
        let value = connection
            .query_row(
                "SELECT execution_id, sequence, label, hash, content, created_at FROM checkpoint WHERE id = ?1",
                [id.to_string()],
                parse_checkpoint,
            )
            .optional()?;
        value.transpose()
    }

    async fn list_states(&self, execution_id: Uuid) -> ExecutionResult<Vec<ExecutionStateRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT execution_id, sequence, from_status, to_status, reason, content, created_at FROM execution_state WHERE execution_id = ?1 ORDER BY sequence, created_at, id",
        )?;
        let rows = statement.query_map([execution_id.to_string()], parse_state)?;
        collect_rows(rows)
    }

    async fn list_retries(&self, execution_id: Uuid) -> ExecutionResult<Vec<RetryRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT execution_id, step_id, action_id, attempt, delay_ms, error_kind, status, content, created_at FROM retry WHERE execution_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([execution_id.to_string()], parse_retry)?;
        collect_rows(rows)
    }

    async fn list_rollbacks(&self, execution_id: Uuid) -> ExecutionResult<Vec<RollbackRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT execution_id, step_id, action_id, command_id, status, error_kind, content, created_at FROM rollback WHERE execution_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([execution_id.to_string()], parse_rollback)?;
        collect_rows(rows)
    }
}

fn write_execution(
    transaction: &Transaction<'_>,
    commit: &ExecutionCommit,
    actor: &str,
) -> ExecutionResult<()> {
    let value = &commit.execution;
    let content = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();
    match commit.expected_version {
        None => {
            let inserted = transaction.execute(
                "INSERT INTO execution (id, plan_id, plan_version, plan_hash, status, version, current_task_id, current_step_id, content, created_at, updated_at, create_time, update_time, create_user, update_user) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?13)",
                params![
                    value.id.to_string(), value.plan_id.to_string(), value.plan_version as i64,
                    value.plan_hash, value.status.as_str(), value.version as i64,
                    value.current_task_id.map(|id| id.to_string()),
                    value.current_step_id.map(|id| id.to_string()), content,
                    value.created_at.to_rfc3339(), value.updated_at.to_rfc3339(), now, actor,
                ],
            );
            map_unique(inserted, value.id)?;
        }
        Some(expected) => {
            let changed = transaction.execute(
                "UPDATE execution SET plan_id=?2, plan_version=?3, plan_hash=?4, status=?5, version=?6, current_task_id=?7, current_step_id=?8, content=?9, updated_at=?10, update_time=?11, update_user=?12 WHERE id=?1 AND version=?13",
                params![
                    value.id.to_string(), value.plan_id.to_string(), value.plan_version as i64,
                    value.plan_hash, value.status.as_str(), value.version as i64,
                    value.current_task_id.map(|id| id.to_string()),
                    value.current_step_id.map(|id| id.to_string()), content,
                    value.updated_at.to_rfc3339(), now, actor, expected as i64,
                ],
            )?;
            if changed != 1 {
                return Err(ExecutionError::Conflict(format!(
                    "execution {} expected version {expected}",
                    value.id
                )));
            }
        }
    }
    Ok(())
}

fn insert_state(
    transaction: &Transaction<'_>,
    value: &ExecutionStateRecord,
    actor: &str,
) -> ExecutionResult<()> {
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO execution_state (id, execution_id, sequence, from_status, to_status, reason, content, created_at, create_time, update_time, create_user, update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",
        params![
            value.id.to_string(), value.execution_id.to_string(), value.sequence as i64,
            value.from_status.map(|status| status.as_str()), value.to_status.as_str(), value.reason,
            serde_json::to_string(value)?, value.created_at.to_rfc3339(), now, actor,
        ],
    )?;
    Ok(())
}

fn insert_checkpoint(
    transaction: &Transaction<'_>,
    value: &ExecutionCheckpoint,
    actor: &str,
) -> ExecutionResult<()> {
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO checkpoint (id, execution_id, sequence, label, hash, content, created_at, create_time, update_time, create_user, update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8,?9,?9)",
        params![
            value.id.to_string(), value.execution_id.to_string(), value.sequence as i64,
            value.label, value.hash, serde_json::to_string(value)?, value.created_at.to_rfc3339(),
            now, actor,
        ],
    )?;
    Ok(())
}

fn insert_retry(
    transaction: &Transaction<'_>,
    value: &RetryRecord,
    actor: &str,
) -> ExecutionResult<()> {
    let now = Utc::now().to_rfc3339();
    let status = match value.status {
        RetryStatus::Scheduled => "SCHEDULED",
        RetryStatus::Resumed => "RESUMED",
    };
    transaction.execute(
        "INSERT INTO retry (id, execution_id, step_id, action_id, attempt, delay_ms, error_kind, status, content, created_at, create_time, update_time, create_user, update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,?12)",
        params![
            value.id.to_string(), value.execution_id.to_string(), value.step_id.to_string(),
            value.action_id.to_string(), value.attempt as i64, value.delay_ms as i64,
            value.error_kind, status, serde_json::to_string(value)?, value.created_at.to_rfc3339(),
            now, actor,
        ],
    )?;
    Ok(())
}

fn insert_rollback(
    transaction: &Transaction<'_>,
    value: &RollbackRecord,
    actor: &str,
) -> ExecutionResult<()> {
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        "INSERT INTO rollback (id, execution_id, step_id, action_id, command_id, status, error_kind, content, created_at, create_time, update_time, create_user, update_user) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?11)",
        params![
            value.id.to_string(), value.execution_id.to_string(), value.step_id.to_string(),
            value.action_id.to_string(), value.command_id.to_string(), value.status.as_str(),
            value.error_kind, serde_json::to_string(value)?, value.created_at.to_rfc3339(), now, actor,
        ],
    )?;
    Ok(())
}

fn parse_execution(row: &Row<'_>) -> rusqlite::Result<ExecutionResult<Execution>> {
    let plan_id: String = row.get(0)?;
    let plan_version: i64 = row.get(1)?;
    let plan_hash: String = row.get(2)?;
    let status: String = row.get(3)?;
    let version: i64 = row.get(4)?;
    let current_task_id: Option<String> = row.get(5)?;
    let current_step_id: Option<String> = row.get(6)?;
    let content: String = row.get(7)?;
    let created_at: String = row.get(8)?;
    let updated_at: String = row.get(9)?;
    Ok((|| {
        let value: Execution = serde_json::from_str(&content)?;
        value.validate()?;
        if plan_id != value.plan_id.to_string()
            || plan_version < 0
            || plan_version as u64 != value.plan_version
            || plan_hash != value.plan_hash
            || status != value.status.as_str()
            || version < 0
            || version as u64 != value.version
            || current_task_id != value.current_task_id.map(|id| id.to_string())
            || current_step_id != value.current_step_id.map(|id| id.to_string())
            || parse_time(&created_at)? != value.created_at
            || parse_time(&updated_at)? != value.updated_at
        {
            return Err(ExecutionError::Validation(
                "execution columns do not match serialized aggregate".into(),
            ));
        }
        Ok(value)
    })())
}

fn parse_checkpoint(row: &Row<'_>) -> rusqlite::Result<ExecutionResult<ExecutionCheckpoint>> {
    let execution_id: String = row.get(0)?;
    let sequence: i64 = row.get(1)?;
    let label: String = row.get(2)?;
    let hash: String = row.get(3)?;
    let content: String = row.get(4)?;
    let created_at: String = row.get(5)?;
    Ok((|| {
        let value: ExecutionCheckpoint = serde_json::from_str(&content)?;
        value.validate()?;
        if execution_id != value.execution_id.to_string()
            || sequence < 0
            || sequence as u64 != value.sequence
            || label != value.label
            || hash != value.hash
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("checkpoint"));
        }
        Ok(value)
    })())
}

fn parse_state(row: &Row<'_>) -> rusqlite::Result<ExecutionResult<ExecutionStateRecord>> {
    let execution_id: String = row.get(0)?;
    let sequence: i64 = row.get(1)?;
    let from_status: Option<String> = row.get(2)?;
    let to_status: String = row.get(3)?;
    let reason: String = row.get(4)?;
    let content: String = row.get(5)?;
    let created_at: String = row.get(6)?;
    Ok((|| {
        let value: ExecutionStateRecord = serde_json::from_str(&content)?;
        let parsed_from = from_status.as_deref().map(parse_status).transpose()?;
        if execution_id != value.execution_id.to_string()
            || sequence < 0
            || sequence as u64 != value.sequence
            || parsed_from != value.from_status
            || parse_status(&to_status)? != value.to_status
            || reason != value.reason
            || reason.trim().is_empty()
            || reason.len() > 1024
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("execution_state"));
        }
        Ok(value)
    })())
}

fn parse_retry(row: &Row<'_>) -> rusqlite::Result<ExecutionResult<RetryRecord>> {
    let execution_id: String = row.get(0)?;
    let step_id: String = row.get(1)?;
    let action_id: String = row.get(2)?;
    let attempt: i64 = row.get(3)?;
    let delay_ms: i64 = row.get(4)?;
    let error_kind: String = row.get(5)?;
    let status: String = row.get(6)?;
    let content: String = row.get(7)?;
    let created_at: String = row.get(8)?;
    Ok((|| {
        let value: RetryRecord = serde_json::from_str(&content)?;
        let parsed_status = match status.as_str() {
            "SCHEDULED" => RetryStatus::Scheduled,
            "RESUMED" => RetryStatus::Resumed,
            _ => return Err(corrupt("retry")),
        };
        if execution_id != value.execution_id.to_string()
            || step_id != value.step_id.to_string()
            || action_id != value.action_id.to_string()
            || attempt <= 0
            || attempt as u32 != value.attempt
            || delay_ms < 0
            || delay_ms as u64 != value.delay_ms
            || error_kind != value.error_kind
            || parsed_status != value.status
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("retry"));
        }
        Ok(value)
    })())
}

fn parse_rollback(row: &Row<'_>) -> rusqlite::Result<ExecutionResult<RollbackRecord>> {
    let execution_id: String = row.get(0)?;
    let step_id: String = row.get(1)?;
    let action_id: String = row.get(2)?;
    let command_id: String = row.get(3)?;
    let status: String = row.get(4)?;
    let error_kind: Option<String> = row.get(5)?;
    let content: String = row.get(6)?;
    let created_at: String = row.get(7)?;
    Ok((|| {
        let value: RollbackRecord = serde_json::from_str(&content)?;
        let parsed_status = match status.as_str() {
            "SUCCESS" => RollbackStatus::Success,
            "FAILED" => RollbackStatus::Failed,
            "SKIPPED" => RollbackStatus::Skipped,
            _ => return Err(corrupt("rollback")),
        };
        if execution_id != value.execution_id.to_string()
            || step_id != value.step_id.to_string()
            || action_id != value.action_id.to_string()
            || command_id != value.command_id.to_string()
            || parsed_status != value.status
            || error_kind != value.error_kind
            || parse_time(&created_at)? != value.created_at
        {
            return Err(corrupt("rollback"));
        }
        Ok(value)
    })())
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&Row<'_>) -> rusqlite::Result<ExecutionResult<T>>>,
) -> ExecutionResult<Vec<T>> {
    let mut values = Vec::new();
    for row in rows {
        values.push(row??);
    }
    Ok(values)
}

fn parse_time(value: &str) -> ExecutionResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| ExecutionError::Validation(format!("invalid timestamp: {error}")))
}

fn parse_status(value: &str) -> ExecutionResult<ExecutionStatus> {
    ExecutionStatus::parse(value)
        .ok_or_else(|| ExecutionError::Validation(format!("invalid execution status {value}")))
}

fn corrupt(table: &str) -> ExecutionError {
    ExecutionError::Validation(format!("{table} columns do not match serialized record"))
}

fn map_unique(result: rusqlite::Result<usize>, id: Uuid) -> ExecutionResult<()> {
    match result {
        Ok(1) => Ok(()),
        Ok(_) => Err(ExecutionError::Internal(
            "execution insert count mismatch".into(),
        )),
        Err(error)
            if error.sqlite_error_code() == Some(rusqlite::ErrorCode::ConstraintViolation) =>
        {
            Err(ExecutionError::Conflict(format!(
                "execution {id} already exists"
            )))
        }
        Err(error) => Err(error.into()),
    }
}
