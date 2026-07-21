use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{AuditEvent, AuditQuery, AuditSeverity, AuditSnapshot, validate_actor};
use crate::error::{AuditError, AuditResult};
use crate::infrastructure::AuditStore;

#[derive(Default)]
pub struct InMemoryAuditStore {
    events: RwLock<Vec<AuditEvent>>,
}

#[async_trait]
impl AuditStore for InMemoryAuditStore {
    async fn record(&self, event: &AuditEvent, actor: &str) -> AuditResult<()> {
        validate_actor("audit writer", actor)?;
        event.validate()?;
        let mut events = self.events
            .write()
            .map_err(|_| AuditError::Internal("audit store lock poisoned".into()))?;
        if events.iter().any(|e| e.id == event.id) {
            return Err(AuditError::Conflict(
                "audit event already exists".into(),
            ));
        }
        events.push(event.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AuditResult<Option<AuditEvent>> {
        let events = self.events
            .read()
            .map_err(|_| AuditError::Internal("audit store lock poisoned".into()))?;
        Ok(events.iter().find(|e| e.id == id).cloned())
    }

    async fn list(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEvent>> {
        query.validate()?;
        let events = self.events
            .read()
            .map_err(|_| AuditError::Internal("audit store lock poisoned".into()))?;
        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|e| {
                query.tenant_id.map_or(true, |t| e.tenant_id == t)
                    && query.actor.as_ref().map_or(true, |a| e.actor == *a)
                    && query.event_type.map_or(true, |t| e.event_type == t)
                    && query.action.as_ref().map_or(true, |a| e.action == *a)
                    && query.resource.as_ref().map_or(true, |r| e.resource == *r)
                    && query.severity.map_or(true, |s| e.severity == s)
                    && query.from.map_or(true, |f| e.occurred_at >= f)
                    && query.to.map_or(true, |t| e.occurred_at <= t)
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn count(&self, query: &AuditQuery) -> AuditResult<u64> {
        query.validate()?;
        let events = self.events
            .read()
            .map_err(|_| AuditError::Internal("audit store lock poisoned".into()))?;
        let count = events
            .iter()
            .filter(|e| {
                query.tenant_id.map_or(true, |t| e.tenant_id == t)
                    && query.actor.as_ref().map_or(true, |a| e.actor == *a)
                    && query.event_type.map_or(true, |t| e.event_type == t)
                    && query.action.as_ref().map_or(true, |a| e.action == *a)
                    && query.severity.map_or(true, |s| e.severity == s)
                    && query.from.map_or(true, |f| e.occurred_at >= f)
                    && query.to.map_or(true, |t| e.occurred_at <= t)
            })
            .count() as u64;
        Ok(count)
    }

    async fn snapshot(&self, tenant_id: Uuid) -> AuditResult<AuditSnapshot> {
        let events = self.events
            .read()
            .map_err(|_| AuditError::Internal("audit store lock poisoned".into()))?;
        let tenant_events: Vec<&AuditEvent> = events
            .iter()
            .filter(|e| e.tenant_id == tenant_id)
            .collect();

        let mut by_type = BTreeMap::new();
        let mut by_severity = BTreeMap::new();
        let mut min_time: Option<DateTime<Utc>> = None;
        let mut max_time: Option<DateTime<Utc>> = None;

        for event in &tenant_events {
            *by_type.entry(event.event_type.as_str().to_string()).or_insert(0u64) += 1;
            *by_severity.entry(event.severity.as_str().to_string()).or_insert(0u64) += 1;
            if min_time.map_or(true, |t| event.occurred_at < t) {
                min_time = Some(event.occurred_at);
            }
            if max_time.map_or(true, |t| event.occurred_at > t) {
                max_time = Some(event.occurred_at);
            }
        }

        Ok(AuditSnapshot {
            tenant_id,
            total_events: tenant_events.len() as u64,
            by_event_type: by_type,
            by_severity: by_severity,
            from: min_time,
            to: max_time,
        })
    }
}

pub struct NoopAuditObserver;

impl crate::infrastructure::AuditObserver for NoopAuditObserver {
    fn on_audit(&self, _event: &AuditEvent) {}
}