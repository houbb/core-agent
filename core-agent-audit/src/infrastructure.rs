use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{AuditEvent, AuditQuery, AuditSnapshot};
use crate::error::AuditResult;

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn record(&self, event: &AuditEvent, actor: &str) -> AuditResult<()>;
    async fn find(&self, id: Uuid) -> AuditResult<Option<AuditEvent>>;
    async fn list(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEvent>>;
    async fn count(&self, query: &AuditQuery) -> AuditResult<u64>;
    async fn snapshot(&self, tenant_id: Uuid) -> AuditResult<AuditSnapshot>;
}

pub trait AuditObserver: Send + Sync {
    fn on_audit(&self, event: &AuditEvent);
}