use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::defaults::{InMemoryAuditStore, NoopAuditObserver};
use crate::domain::{AuditEvent, AuditQuery, AuditSnapshot, validate_actor};
use crate::error::{AuditError, AuditResult};
use crate::infrastructure::{AuditObserver, AuditStore};

pub struct AuditManagerBuilder {
    store: Arc<dyn AuditStore>,
    observers: Vec<Arc<dyn AuditObserver>>,
}

impl Default for AuditManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryAuditStore::default()),
            observers: Vec::new(),
        }
    }
}

impl AuditManagerBuilder {
    pub fn store(mut self, value: Arc<dyn AuditStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn AuditObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> AuditManager {
        AuditManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct AuditManager {
    store: Arc<dyn AuditStore>,
    observers: Vec<Arc<dyn AuditObserver>>,
}

impl AuditManager {
    pub fn builder() -> AuditManagerBuilder {
        AuditManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn AuditStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn record(&self, event: &AuditEvent, actor: &str) -> AuditResult<()> {
        event.validate()?;
        validate_actor("audit recorder", actor)?;
        self.store.record(event, actor).await?;
        for observer in &self.observers {
            observer.on_audit(event);
        }
        Ok(())
    }

    pub async fn find(&self, id: Uuid) -> AuditResult<Option<AuditEvent>> {
        self.store.find(id).await
    }

    pub async fn list(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEvent>> {
        self.store.list(query).await
    }

    pub async fn count(&self, query: &AuditQuery) -> AuditResult<u64> {
        self.store.count(query).await
    }

    pub async fn snapshot(&self, tenant_id: Uuid) -> AuditResult<AuditSnapshot> {
        self.store.snapshot(tenant_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn record_and_list_events() {
        let manager = AuditManager::builder().build();
        let tenant = Uuid::new_v4();
        let event = AuditEvent::new(tenant, "test-agent", crate::AuditEventType::ToolCall, "file.read", "test.txt");
        manager.record(&event, "system").await.unwrap();

        let events = manager.list(&AuditQuery {
            tenant_id: Some(tenant),
            ..Default::default()
        }).await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, "file.read");
    }

    #[tokio::test]
    async fn record_duplicate_is_rejected() {
        let manager = AuditManager::builder().build();
        let tenant = Uuid::new_v4();
        let event = AuditEvent::new(tenant, "test-agent", crate::AuditEventType::System, "test", "test");
        manager.record(&event, "system").await.unwrap();

        let dup = event.clone();
        let result = manager.record(&dup, "system").await;
        assert!(matches!(result, Err(AuditError::Conflict(_))));
    }

    #[tokio::test]
    async fn count_filters_correctly() {
        let manager = AuditManager::builder().build();
        let tenant = Uuid::new_v4();
        for i in 0..5 {
            let event = AuditEvent::new(tenant, "agent", crate::AuditEventType::System, &format!("action{i}"), "resource")
                .with_severity(crate::AuditSeverity::Info);
            manager.record(&event, "system").await.unwrap();
        }
        let count = manager.count(&AuditQuery {
            tenant_id: Some(tenant),
            limit: 100,
            offset: 0,
            ..Default::default()
        }).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn snapshot_aggregates_correctly() {
        let manager = AuditManager::builder().build();
        let tenant = Uuid::new_v4();
        for i in 0..3 {
            let event = AuditEvent::new(tenant, "agent", crate::AuditEventType::ToolCall, "file.read", "test")
                .with_severity(crate::AuditSeverity::Info);
            manager.record(&event, "system").await.unwrap();
        }
        let snap = manager.snapshot(tenant).await.unwrap();
        assert_eq!(snap.total_events, 3);
        assert_eq!(*snap.by_event_type.get("TOOL_CALL").unwrap(), 3);
    }
}