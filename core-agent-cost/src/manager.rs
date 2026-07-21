use std::sync::Arc;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::defaults::{InMemoryCostStore, DefaultCostControlStrategy, NoopCostObserver};
use crate::domain::{
    Budget, BudgetScope, CostRecord, CostSummary, validate_actor,
};
use crate::error::{CostError, CostResult};
use crate::infrastructure::{CostControlStrategy, CostObserver, CostStore};

pub struct CostManagerBuilder {
    store: Arc<dyn CostStore>,
    control_strategy: Arc<dyn CostControlStrategy>,
    observers: Vec<Arc<dyn CostObserver>>,
}

impl Default for CostManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryCostStore::default()),
            control_strategy: Arc::new(DefaultCostControlStrategy),
            observers: Vec::new(),
        }
    }
}

impl CostManagerBuilder {
    pub fn store(mut self, value: Arc<dyn CostStore>) -> Self {
        self.store = value;
        self
    }

    pub fn control_strategy(mut self, value: Arc<dyn CostControlStrategy>) -> Self {
        self.control_strategy = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn CostObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> CostManager {
        CostManager {
            store: self.store,
            control_strategy: self.control_strategy,
            observers: self.observers,
        }
    }
}

pub struct CostManager {
    store: Arc<dyn CostStore>,
    control_strategy: Arc<dyn CostControlStrategy>,
    observers: Vec<Arc<dyn CostObserver>>,
}

impl CostManager {
    pub fn builder() -> CostManagerBuilder {
        CostManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn CostStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn record_cost(
        &self,
        record: &CostRecord,
        actor: &str,
    ) -> CostResult<CostRecord> {
        record.validate()?;
        validate_actor("cost recorder", actor)?;

        // Check budgets before recording
        let budgets = self.store.list_budgets(record.tenant_id).await?;
        self.control_strategy.check_allowed(record, &budgets)?;

        self.store.record(record, actor).await?;

        // Update budget usage
        for budget in &budgets {
            if budget.scope_id == record.agent_id.map(|id| id.to_string()).unwrap_or_default()
                || budget.scope_id == record.project_id.map(|id| id.to_string()).unwrap_or_default()
            {
                let mut updated = budget.clone();
                updated.monthly_used_micros = updated.monthly_used_micros.saturating_add(record.amount_micros);
                if updated.monthly_used_micros >= updated.monthly_limit_micros {
                    updated.state = crate::BudgetState::Exceeded;
                }
                updated.version = updated.version.saturating_add(1);
                updated.updated_at = chrono::Utc::now().max(budget.updated_at);
                self.store.upsert_budget(&updated, actor).await?;
                if updated.state == crate::BudgetState::Exceeded {
                    for observer in &self.observers {
                        observer.on_budget_exceeded(&updated);
                    }
                }
            }
        }

        for observer in &self.observers {
            observer.on_cost_recorded(record);
        }
        Ok(record.clone())
    }

    pub async fn aggregate(
        &self,
        tenant_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        self.store.aggregate(tenant_id, from, to).await
    }

    pub async fn aggregate_by_agent(
        &self,
        agent_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> CostResult<CostSummary> {
        self.store.aggregate_by_agent(agent_id, from, to).await
    }

    pub async fn set_budget(&self, budget: &Budget, actor: &str) -> CostResult<Budget> {
        budget.validate()?;
        validate_actor("budget setter", actor)?;
        self.store.upsert_budget(budget, actor).await?;
        Ok(budget.clone())
    }

    pub async fn find_budget(
        &self,
        scope: BudgetScope,
        scope_id: &str,
    ) -> CostResult<Option<Budget>> {
        self.store.find_budget(scope, scope_id).await
    }

    pub async fn list_budgets(&self, tenant_id: Uuid) -> CostResult<Vec<Budget>> {
        self.store.list_budgets(tenant_id).await
    }

    pub async fn find(&self, id: Uuid) -> CostResult<Option<CostRecord>> {
        self.store.find(id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_and_aggregate_costs() {
        let manager = CostManager::builder().build();
        let tenant = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let record = CostRecord::new(tenant, "evt-001", "USD", 5000, "rca-agent")
            .with_agent(agent_id)
            .with_model("gpt-5");
        manager.record_cost(&record, "system").await.unwrap();

        let record2 = CostRecord::new(tenant, "evt-002", "USD", 3000, "rca-agent")
            .with_agent(agent_id)
            .with_model("gpt-5");
        manager.record_cost(&record2, "system").await.unwrap();

        let now = Utc::now();
        let from = now - chrono::Duration::hours(1);
        let to = now + chrono::Duration::hours(1);

        let summary = manager.aggregate(tenant, from, to).await.unwrap();
        assert_eq!(summary.record_count, 2);
        assert_eq!(summary.total_amount_micros, 8000);

        let agent_summary = manager.aggregate_by_agent(agent_id, from, to).await.unwrap();
        assert_eq!(agent_summary.record_count, 2);
    }

    #[tokio::test]
    async fn duplicate_event_key_is_rejected() {
        let manager = CostManager::builder().build();
        let tenant = Uuid::new_v4();
        let record = CostRecord::new(tenant, "evt-001", "USD", 1000, "agent");
        manager.record_cost(&record, "system").await.unwrap();
        let result = manager.record_cost(&record, "system").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn budget_limits_are_enforced() {
        let manager = CostManager::builder().build();
        let tenant = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 5000, "admin");
        manager.set_budget(&budget, "admin").await.unwrap();

        let record = CostRecord::new(tenant, "evt-001", "USD", 3000, "agent")
            .with_agent(agent_id);
        manager.record_cost(&record, "system").await.unwrap();

        let record2 = CostRecord::new(tenant, "evt-002", "USD", 3000, "agent")
            .with_agent(agent_id);
        let result = manager.record_cost(&record2, "system").await;
        assert!(matches!(result, Err(CostError::BudgetExceeded(_))));
    }

    #[tokio::test]
    async fn budget_usage_is_updated_after_recording() {
        let manager = CostManager::builder().build();
        let tenant = Uuid::new_v4();
        let agent_id = Uuid::new_v4();

        let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 10000, "admin");
        manager.set_budget(&budget, "admin").await.unwrap();

        let record = CostRecord::new(tenant, "evt-001", "USD", 3000, "agent")
            .with_agent(agent_id);
        manager.record_cost(&record, "system").await.unwrap();

        let found = manager.find_budget(BudgetScope::Agent, &agent_id.to_string()).await.unwrap().unwrap();
        assert_eq!(found.monthly_used_micros, 3000);
    }

    #[tokio::test]
    async fn cost_sqlite_persistence() {
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("cost.db");
        let store = Arc::new(crate::persistence::SqliteCostStore::new(&db_path).unwrap());
        let manager = CostManager::new(store);
        let tenant = Uuid::new_v4();

        let record = CostRecord::new(tenant, "evt-001", "USD", 5000, "agent")
            .with_tokens(100, 20, 5);
        manager.record_cost(&record, "system").await.unwrap();

        let found = manager.find(record.id).await.unwrap().unwrap();
        assert_eq!(found.id, record.id);

        // Re-open
        let store2 = Arc::new(crate::persistence::SqliteCostStore::new(&db_path).unwrap());
        let manager2 = CostManager::new(store2);
        let found2 = manager2.find(record.id).await.unwrap().unwrap();
        assert_eq!(found2.id, record.id);
        assert_eq!(found2.amount_micros, 600); // (100 + 20) * 5
    }
}