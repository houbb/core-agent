use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    LearningQuery, LearningRecord, LearningSnapshot, LearningStatus, validate_actor,
};
use crate::error::{LearningError, LearningResult};
use crate::infrastructure::LearningStore;

#[derive(Default)]
pub struct InMemoryLearningStore {
    records: RwLock<Vec<LearningRecord>>,
}

#[async_trait]
impl LearningStore for InMemoryLearningStore {
    async fn record(&self, record: &LearningRecord, actor: &str) -> LearningResult<()> {
        validate_actor(actor)?;
        record.validate()?;
        let mut records = self
            .records
            .write()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        if records.iter().any(|r| r.id == record.id) {
            return Err(LearningError::Conflict("record already exists".into()));
        }
        records.push(record.clone());
        Ok(())
    }

    async fn update(&self, record: &LearningRecord, actor: &str) -> LearningResult<()> {
        validate_actor(actor)?;
        record.validate()?;
        let mut records = self
            .records
            .write()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        if let Some(pos) = records.iter().position(|r| r.id == record.id) {
            records[pos] = record.clone();
            Ok(())
        } else {
            Err(LearningError::NotFound(record.id.to_string()))
        }
    }

    async fn find(&self, id: Uuid) -> LearningResult<Option<LearningRecord>> {
        let records = self
            .records
            .read()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        Ok(records.iter().find(|r| r.id == id).cloned())
    }

    async fn list(&self, query: &LearningQuery) -> LearningResult<Vec<LearningRecord>> {
        query.validate()?;
        let records = self
            .records
            .read()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        Ok(records
            .iter()
            .filter(|r| {
                query.agent_id.map_or(true, |a| r.agent_id == a)
                    && query
                        .learning_type
                        .map_or(true, |t| r.learning_type == t)
                    && query.status.map_or(true, |s| r.status == s)
                    && query.source.map_or(true, |s| r.source == s)
                    && query
                        .confidence_min
                        .map_or(true, |c| r.confidence >= c)
                    && query.from.map_or(true, |f| r.created_at >= f)
                    && query.to.map_or(true, |t| r.created_at <= t)
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect())
    }

    async fn count(&self, query: &LearningQuery) -> LearningResult<u64> {
        query.validate()?;
        let records = self
            .records
            .read()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        Ok(records
            .iter()
            .filter(|r| {
                query.agent_id.map_or(true, |a| r.agent_id == a)
                    && query
                        .learning_type
                        .map_or(true, |t| r.learning_type == t)
                    && query.status.map_or(true, |s| r.status == s)
                    && query
                        .confidence_min
                        .map_or(true, |c| r.confidence >= c)
            })
            .count() as u64)
    }

    async fn snapshot(&self, agent_id: Uuid) -> LearningResult<LearningSnapshot> {
        let records = self
            .records
            .read()
            .map_err(|_| LearningError::Internal("lock poisoned".into()))?;
        let agent: Vec<&LearningRecord> =
            records.iter().filter(|r| r.agent_id == agent_id).collect();

        let mut by_type = BTreeMap::new();
        let mut by_status = BTreeMap::new();
        let mut applied = 0u64;
        let mut total_conf = 0.0;

        for r in &agent {
            *by_type
                .entry(r.learning_type.as_str().to_string())
                .or_insert(0u64) += 1;
            *by_status
                .entry(r.status.as_str().to_string())
                .or_insert(0u64) += 1;
            total_conf += r.confidence;
            if r.status == LearningStatus::Applied {
                applied += 1;
            }
        }

        let avg = if agent.is_empty() {
            0.0
        } else {
            total_conf / agent.len() as f64
        };

        Ok(LearningSnapshot {
            agent_id,
            total_records: agent.len() as u64,
            by_type,
            by_status,
            avg_confidence: (avg * 100.0).round() / 100.0,
            applied_count: applied,
        })
    }
}

pub struct NoopLearningObserver;

impl crate::infrastructure::LearningObserver for NoopLearningObserver {
    fn on_learning(&self, _record: &LearningRecord) {}
}