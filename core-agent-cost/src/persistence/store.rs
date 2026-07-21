use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    Budget, BudgetScope, BudgetState, CostRecord, CostSummary,
    validate_actor,
};
use crate::error::{CostError, CostResult};
use crate::infrastructure::CostStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteCostStore {
    connection: Mutex<Connection>,
}

impl SqliteCostStore {
    pub fn new(path: impl AsRef<Path>) -> CostResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> CostResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> CostResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> CostResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| CostError::Internal("cost SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl CostStore for SqliteCostStore {
    async fn record(&self, record: &CostRecord, actor: &str) -> CostResult<()> {
        validate_actor("cost recorder", actor)?;
        record.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row("SELECT 1 FROM cost_record WHERE event_key = ?1", [&record.event_key], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            return Err(CostError::Conflict("cost event already recorded".into()));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO cost_record (
                id, tenant_id, organization_id, project_id, agent_id, session_id, model_key,
                input_tokens, output_tokens, price_per_token_micros, amount_micros, currency,
                event_key, actor, occurred_at, version, content, created_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19, ?20, ?20)",
            params![
                record.id.to_string(),
                record.tenant_id.to_string(),
                record.organization_id.map(|id| id.to_string()),
                record.project_id.map(|id| id.to_string()),
                record.agent_id.map(|id| id.to_string()),
                record.session_id.map(|id| id.to_string()),
                record.model_key,
                u64_i64(record.input_tokens)?,
                u64_i64(record.output_tokens)?,
                u64_i64(record.price_per_token_micros)?,
                u64_i64(record.amount_micros)?,
                record.currency,
                record.event_key,
                record.actor,
                record.occurred_at.to_rfc3339(),
                u64_i64(record.version)?,
                serde_json::to_string(record)?,
                record.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> CostResult<Option<CostRecord>> {
        let connection = self.lock()?;
        let raw: Option<(String, String, String, i64, i64, i64, i64, String, String, String, String, String)> = connection
            .query_row(
                "SELECT id, content, event_key, input_tokens, output_tokens, amount_micros, version,
                        currency, actor, occurred_at, created_at, tenant_id
                 FROM cost_record WHERE id = ?1",
                [id.to_string()],
                |row| Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?,
                    row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                )),
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: CostRecord = serde_json::from_str(&raw.1)?;
        value.validate()?;
        if raw.0 != value.id.to_string()
            || raw.2 != value.event_key
            || raw.3 != u64_i64(value.input_tokens)?
            || raw.5 != u64_i64(value.amount_micros)?
            || raw.6 != u64_i64(value.version)?
            || raw.9 != value.occurred_at.to_rfc3339()
        {
            return Err(CostError::Validation("cost columns do not match content".into()));
        }
        Ok(Some(value))
    }

    async fn find_by_event_key(&self, event_key: &str) -> CostResult<Option<CostRecord>> {
        let id: Option<String> = {
            let connection = self.lock()?;
            connection
                .query_row("SELECT id FROM cost_record WHERE event_key = ?1", [event_key], |row| row.get(0))
                .optional()?
        };
        match id {
            Some(id) => {
                let uuid = Uuid::parse_str(&id)
                    .map_err(|e| CostError::Validation(format!("invalid uuid: {e}")))?;
                self.find(uuid).await
            }
            None => Ok(None),
        }
    }

    async fn aggregate(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        let connection = self.lock()?;
        let (total_amount, total_input, total_output, count): (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT COALESCE(SUM(amount_micros), 0), COALESCE(SUM(input_tokens), 0),
                        COALESCE(SUM(output_tokens), 0), COUNT(*)
                 FROM cost_record WHERE tenant_id = ?1 AND occurred_at >= ?2 AND occurred_at <= ?3",
                params![tenant_id.to_string(), from.to_rfc3339(), to.to_rfc3339()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        let mut statement = connection.prepare(
            "SELECT currency, SUM(amount_micros) FROM cost_record
             WHERE tenant_id = ?1 AND occurred_at >= ?2 AND occurred_at <= ?3 GROUP BY currency"
        )?;
        let by_currency: std::collections::BTreeMap<String, u64> = statement
            .query_map(params![tenant_id.to_string(), from.to_rfc3339(), to.to_rfc3339()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<_, _>>()?;

        let mut statement = connection.prepare(
            "SELECT COALESCE(model_key, 'unknown'), SUM(amount_micros) FROM cost_record
             WHERE tenant_id = ?1 AND occurred_at >= ?2 AND occurred_at <= ?3 GROUP BY model_key"
        )?;
        let by_model: std::collections::BTreeMap<String, u64> = statement
            .query_map(params![tenant_id.to_string(), from.to_rfc3339(), to.to_rfc3339()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<_, _>>()?;

        Ok(CostSummary {
            tenant_id,
            total_amount_micros: total_amount as u64,
            total_input_tokens: total_input as u64,
            total_output_tokens: total_output as u64,
            by_currency,
            by_agent: std::collections::BTreeMap::new(),
            by_model,
            period_start: from,
            period_end: to,
            record_count: count as u64,
        })
    }

    async fn aggregate_by_agent(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        let connection = self.lock()?;
        let (total_amount, total_input, total_output, count): (i64, i64, i64, i64) = connection
            .query_row(
                "SELECT COALESCE(SUM(amount_micros), 0), COALESCE(SUM(input_tokens), 0),
                        COALESCE(SUM(output_tokens), 0), COUNT(*)
                 FROM cost_record WHERE agent_id = ?1 AND occurred_at >= ?2 AND occurred_at <= ?3",
                params![agent_id.to_string(), from.to_rfc3339(), to.to_rfc3339()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

        Ok(CostSummary {
            tenant_id: Uuid::default(),
            total_amount_micros: total_amount as u64,
            total_input_tokens: total_input as u64,
            total_output_tokens: total_output as u64,
            by_currency: std::collections::BTreeMap::new(),
            by_agent: std::collections::BTreeMap::new(),
            by_model: std::collections::BTreeMap::new(),
            period_start: from,
            period_end: to,
            record_count: count as u64,
        })
    }

    async fn find_budget(&self, scope: BudgetScope, scope_id: &str) -> CostResult<Option<Budget>> {
        let connection = self.lock()?;
        let raw: Option<(String, String, String, String, i64, String, i64, String, i64, String, String, String)> = connection
            .query_row(
                "SELECT id, content, scope, scope_id, monthly_limit_micros, state, version,
                        currency, alert_threshold, created_at, updated_at, tenant_id
                 FROM budget WHERE scope = ?1 AND scope_id = ?2",
                params![scope.as_str(), scope_id],
                |row| Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    row.get(10)?, row.get(11)?,
                )),
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: Budget = serde_json::from_str(&raw.1)?;
        value.validate()?;
        if raw.0 != value.id.to_string() || raw.4 != u64_i64(value.monthly_limit_micros)? || raw.6 != u64_i64(value.version)? {
            return Err(CostError::Validation("budget columns do not match content".into()));
        }
        Ok(Some(value))
    }

    async fn upsert_budget(&self, budget: &Budget, actor: &str) -> CostResult<()> {
        validate_actor("budget author", actor)?;
        budget.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let existing: Option<bool> = connection
            .query_row("SELECT 1 FROM budget WHERE scope = ?1 AND scope_id = ?2",
                params![budget.scope.as_str(), budget.scope_id], |_| Ok(true))
            .optional()?;
        if existing.is_some() {
            connection.execute(
                "UPDATE budget SET monthly_limit_micros = ?1, monthly_used_micros = ?2, state = ?3,
                    version = ?4, content = ?5, updated_at = ?6, update_time = ?7, update_user = ?8
                 WHERE scope = ?9 AND scope_id = ?10 AND version = ?11",
                params![
                    u64_i64(budget.monthly_limit_micros)?,
                    u64_i64(budget.monthly_used_micros)?,
                    budget.state.as_str(),
                    u64_i64(budget.version)?,
                    serde_json::to_string(budget)?,
                    budget.updated_at.to_rfc3339(),
                    now,
                    actor,
                    budget.scope.as_str(),
                    budget.scope_id,
                    u64_i64(budget.version.saturating_sub(1))?,
                ],
            )?;
        } else {
            connection.execute(
                "INSERT INTO budget (id, tenant_id, scope, scope_id, monthly_limit_micros,
                    monthly_used_micros, currency, alert_threshold, state, version, content,
                    created_at, updated_at, create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?15)",
                params![
                    budget.id.to_string(),
                    budget.tenant_id.to_string(),
                    budget.scope.as_str(),
                    budget.scope_id,
                    u64_i64(budget.monthly_limit_micros)?,
                    u64_i64(budget.monthly_used_micros)?,
                    budget.currency,
                    budget.alert_threshold,
                    budget.state.as_str(),
                    u64_i64(budget.version)?,
                    serde_json::to_string(budget)?,
                    budget.created_at.to_rfc3339(),
                    budget.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Ok(())
    }

    async fn list_budgets(&self, tenant_id: Uuid) -> CostResult<Vec<Budget>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id FROM budget WHERE tenant_id = ?1 ORDER BY scope, scope_id, id",
        )?;
        let ids: Vec<String> = statement
            .query_map([tenant_id.to_string()], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut budgets = Vec::new();
        for id_str in ids {
            let uuid = Uuid::parse_str(&id_str)
                .map_err(|e| CostError::Validation(format!("invalid uuid: {e}")))?;
            let raw: Option<(String, String, i64, String, String)> = connection
                .query_row(
                    "SELECT id, content, version, scope, scope_id FROM budget WHERE id = ?1",
                    [uuid.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )
                .optional()?;
            if let Some(raw) = raw {
                let value: Budget = serde_json::from_str(&raw.1)?;
                value.validate()?;
                budgets.push(value);
            }
        }
        Ok(budgets)
    }
}

fn u64_i64(value: u64) -> CostResult<i64> {
    i64::try_from(value)
        .map_err(|_| CostError::Validation("cost integer exceeds SQLite range".into()))
}