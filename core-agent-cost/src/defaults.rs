use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    Budget, BudgetScope, BudgetState, CostRecord, CostSummary,
    validate_actor,
};
use crate::error::{CostError, CostResult};
use crate::infrastructure::{CostControlStrategy, CostObserver, CostStore};

#[derive(Default)]
pub struct InMemoryCostStore {
    records: RwLock<BTreeMap<String, CostRecord>>,
    budgets: RwLock<BTreeMap<String, Budget>>,
}

#[async_trait]
impl CostStore for InMemoryCostStore {
    async fn record(&self, record: &CostRecord, actor: &str) -> CostResult<()> {
        validate_actor("cost recorder", actor)?;
        record.validate()?;
        let mut records = self.records
            .write()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        if records.contains_key(&record.event_key) {
            return Err(CostError::Conflict(
                "cost event already recorded".into(),
            ));
        }
        records.insert(record.event_key.clone(), record.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> CostResult<Option<CostRecord>> {
        let records = self.records
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        Ok(records.values().find(|r| r.id == id).cloned())
    }

    async fn find_by_event_key(&self, event_key: &str) -> CostResult<Option<CostRecord>> {
        let records = self.records
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        Ok(records.get(event_key).cloned())
    }

    async fn aggregate(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        let records = self.records
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        let filtered: Vec<&CostRecord> = records
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.occurred_at >= from && r.occurred_at <= to)
            .collect();

        let mut by_currency = BTreeMap::new();
        let mut by_agent = BTreeMap::new();
        let mut by_model = BTreeMap::new();
        let mut total_amount = 0u64;
        let mut total_input = 0u64;
        let mut total_output = 0u64;

        for record in &filtered {
            total_amount = total_amount.saturating_add(record.amount_micros);
            total_input = total_input.saturating_add(record.input_tokens);
            total_output = total_output.saturating_add(record.output_tokens);
            *by_currency.entry(record.currency.clone()).or_insert(0u64) += record.amount_micros;
            if let Some(agent) = &record.agent_id {
                *by_agent.entry(agent.to_string()).or_insert(0u64) += record.amount_micros;
            }
            if let Some(model) = &record.model_key {
                *by_model.entry(model.clone()).or_insert(0u64) += record.amount_micros;
            }
        }

        Ok(CostSummary {
            tenant_id,
            total_amount_micros: total_amount,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            by_currency,
            by_agent,
            by_model,
            period_start: from,
            period_end: to,
            record_count: filtered.len() as u64,
        })
    }

    async fn aggregate_by_agent(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        let records = self.records
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        let filtered: Vec<&CostRecord> = records
            .values()
            .filter(|r| r.agent_id == Some(agent_id) && r.occurred_at >= from && r.occurred_at <= to)
            .collect();

        let mut by_currency = BTreeMap::new();
        let mut total_amount = 0u64;
        let mut total_input = 0u64;
        let mut total_output = 0u64;

        for record in &filtered {
            total_amount = total_amount.saturating_add(record.amount_micros);
            total_input = total_input.saturating_add(record.input_tokens);
            total_output = total_output.saturating_add(record.output_tokens);
            *by_currency.entry(record.currency.clone()).or_insert(0u64) += record.amount_micros;
        }

        Ok(CostSummary {
            tenant_id: Uuid::default(),
            total_amount_micros: total_amount,
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            by_currency,
            by_agent: BTreeMap::new(),
            by_model: BTreeMap::new(),
            period_start: from,
            period_end: to,
            record_count: filtered.len() as u64,
        })
    }

    async fn find_budget(&self, scope: BudgetScope, scope_id: &str) -> CostResult<Option<Budget>> {
        let budgets = self.budgets
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        let key = format!("{}/{}", scope.as_str(), scope_id);
        Ok(budgets.get(&key).cloned())
    }

    async fn upsert_budget(&self, budget: &Budget, actor: &str) -> CostResult<()> {
        validate_actor("budget author", actor)?;
        budget.validate()?;
        let mut budgets = self.budgets
            .write()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        let key = format!("{}/{}", budget.scope.as_str(), &budget.scope_id);
        if let Some(existing) = budgets.get(&key) {
            if existing.version != budget.version.saturating_sub(1) && budget.version != 1 {
                return Err(CostError::Conflict("budget changed concurrently".into()));
            }
        }
        budgets.insert(key, budget.clone());
        Ok(())
    }

    async fn list_budgets(&self, tenant_id: Uuid) -> CostResult<Vec<Budget>> {
        let budgets = self.budgets
            .read()
            .map_err(|_| CostError::Internal("cost store lock poisoned".into()))?;
        Ok(budgets
            .values()
            .filter(|b| b.tenant_id == tenant_id)
            .cloned()
            .collect())
    }
}

pub struct DefaultCostControlStrategy;

impl CostControlStrategy for DefaultCostControlStrategy {
    fn check_allowed(&self, record: &CostRecord, budgets: &[Budget]) -> CostResult<()> {
        for budget in budgets {
            if budget.state == BudgetState::Suspended {
                return Err(CostError::BudgetExceeded(format!(
                    "budget for {:?} {} is suspended",
                    budget.scope, budget.scope_id
                )));
            }
            let projected = budget.monthly_used_micros.saturating_add(record.amount_micros);
            if projected > budget.monthly_limit_micros {
                return Err(CostError::BudgetExceeded(format!(
                    "budget for {:?} {} would exceed limit ({} > {})",
                    budget.scope, budget.scope_id, projected, budget.monthly_limit_micros
                )));
            }
        }
        Ok(())
    }
}

pub struct NoopCostObserver;

impl CostObserver for NoopCostObserver {
    fn on_cost_recorded(&self, _record: &CostRecord) {}
    fn on_budget_exceeded(&self, _budget: &Budget) {}
}