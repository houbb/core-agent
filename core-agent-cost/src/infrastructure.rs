use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{Budget, BudgetScope, CostRecord, CostSummary};
use crate::error::CostResult;

#[async_trait]
pub trait CostStore: Send + Sync {
    async fn record(&self, record: &CostRecord, actor: &str) -> CostResult<()>;
    async fn find(&self, id: Uuid) -> CostResult<Option<CostRecord>>;
    async fn find_by_event_key(&self, event_key: &str) -> CostResult<Option<CostRecord>>;
    async fn aggregate(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary>;
    async fn aggregate_by_agent(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary>;
    async fn find_budget(&self, scope: BudgetScope, scope_id: &str) -> CostResult<Option<Budget>>;
    async fn upsert_budget(&self, budget: &Budget, actor: &str) -> CostResult<()>;
    async fn list_budgets(&self, tenant_id: Uuid) -> CostResult<Vec<Budget>>;
}

pub trait CostControlStrategy: Send + Sync {
    fn check_allowed(&self, record: &CostRecord, budgets: &[Budget]) -> CostResult<()>;
}

pub trait CostObserver: Send + Sync {
    fn on_cost_recorded(&self, record: &CostRecord);
    fn on_budget_exceeded(&self, budget: &Budget);
}