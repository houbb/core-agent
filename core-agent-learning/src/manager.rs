use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::defaults::{InMemoryLearningStore, NoopLearningObserver};
use crate::domain::{
    LearningQuery, LearningRecord, LearningSnapshot, LearningSource, LearningStatus, LearningType,
    validate_actor,
};
use crate::error::{LearningError, LearningResult};
use crate::infrastructure::{LearningObserver, LearningStore};

pub struct LearningManagerBuilder {
    store: Arc<dyn LearningStore>,
    observers: Vec<Arc<dyn LearningObserver>>,
}

impl Default for LearningManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryLearningStore::default()),
            observers: Vec::new(),
        }
    }
}

impl LearningManagerBuilder {
    pub fn store(mut self, value: Arc<dyn LearningStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn LearningObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> LearningManager {
        LearningManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct LearningManager {
    store: Arc<dyn LearningStore>,
    observers: Vec<Arc<dyn LearningObserver>>,
}

impl LearningManager {
    pub fn builder() -> LearningManagerBuilder {
        LearningManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn LearningStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn create_record(
        &self,
        agent_id: Uuid,
        source: LearningSource,
        learning_type: LearningType,
        title: &str,
        description: &str,
        experience: Value,
        improvement: Value,
        actor: &str,
    ) -> LearningResult<LearningRecord> {
        let record =
            LearningRecord::new(agent_id, source, learning_type, title, description, experience, improvement, actor)?;
        self.store.record(&record, actor).await?;
        for observer in &self.observers {
            observer.on_learning(&record);
        }
        Ok(record)
    }

    pub async fn approve(&self, id: Uuid, actor: &str) -> LearningResult<LearningRecord> {
        validate_actor(actor)?;
        let mut record = self
            .store
            .find(id)
            .await?
            .ok_or_else(|| LearningError::NotFound(id.to_string()))?;
        if record.status != LearningStatus::Candidate
            && record.status != LearningStatus::Reviewing
        {
            return Err(LearningError::Conflict(
                "only Candidate/Reviewing records can be approved".into(),
            ));
        }
        record.status = LearningStatus::Approved;
        record.updated_at = Utc::now();
        record.version += 1;
        record.actor = actor.into();
        self.store.update(&record, actor).await?;
        Ok(record)
    }

    pub async fn apply(&self, id: Uuid, actor: &str) -> LearningResult<LearningRecord> {
        validate_actor(actor)?;
        let mut record = self
            .store
            .find(id)
            .await?
            .ok_or_else(|| LearningError::NotFound(id.to_string()))?;
        if record.status != LearningStatus::Approved {
            return Err(LearningError::Conflict(
                "only Approved records can be applied".into(),
            ));
        }
        record.status = LearningStatus::Applied;
        record.updated_at = Utc::now();
        record.version += 1;
        record.actor = actor.into();
        self.store.update(&record, actor).await?;
        Ok(record)
    }

    pub async fn reject(&self, id: Uuid, actor: &str) -> LearningResult<LearningRecord> {
        validate_actor(actor)?;
        let mut record = self
            .store
            .find(id)
            .await?
            .ok_or_else(|| LearningError::NotFound(id.to_string()))?;
        record.status = LearningStatus::Rejected;
        record.updated_at = Utc::now();
        record.version += 1;
        record.actor = actor.into();
        self.store.update(&record, actor).await?;
        Ok(record)
    }

    pub async fn find(&self, id: Uuid) -> LearningResult<Option<LearningRecord>> {
        self.store.find(id).await
    }

    pub async fn list(&self, query: &LearningQuery) -> LearningResult<Vec<LearningRecord>> {
        self.store.list(query).await
    }

    pub async fn count(&self, query: &LearningQuery) -> LearningResult<u64> {
        self.store.count(query).await
    }

    pub async fn snapshot(&self, agent_id: Uuid) -> LearningResult<LearningSnapshot> {
        self.store.snapshot(agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_list() {
        let manager = LearningManager::builder().build();
        let agent_id = Uuid::new_v4();
        let record = manager
            .create_record(
                agent_id,
                LearningSource::Evaluation,
                LearningType::Skill,
                "redis-slowlog",
                "Always check slowlog first",
                serde_json::json!({"observation": "slow query"}),
                serde_json::json!({"skill": "redis-diagnosis"}),
                "system",
            )
            .await
            .unwrap();
        assert_eq!(record.title, "redis-slowlog");

        let list = manager
            .list(&LearningQuery {
                agent_id: Some(agent_id),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn lifecycle_approve_then_apply() {
        let manager = LearningManager::builder().build();
        let agent_id = Uuid::new_v4();
        let record = manager
            .create_record(
                agent_id,
                LearningSource::Evaluation,
                LearningType::Skill,
                "test",
                "desc",
                Value::Null,
                Value::Null,
                "system",
            )
            .await
            .unwrap();

        let approved = manager.approve(record.id, "reviewer").await.unwrap();
        assert_eq!(approved.status, LearningStatus::Approved);

        let applied = manager.apply(record.id, "system").await.unwrap();
        assert_eq!(applied.status, LearningStatus::Applied);
    }

    #[tokio::test]
    async fn reject_non_approved() {
        let manager = LearningManager::builder().build();
        let agent_id = Uuid::new_v4();
        let record = manager
            .create_record(
                agent_id,
                LearningSource::Evaluation,
                LearningType::Skill,
                "test",
                "desc",
                Value::Null,
                Value::Null,
                "system",
            )
            .await
            .unwrap();

        let result = manager.apply(record.id, "system").await;
        assert!(result.is_err()); // Can't apply a Candidate directly
    }

    #[tokio::test]
    async fn snapshot_aggregates() {
        let manager = LearningManager::builder().build();
        let agent_id = Uuid::new_v4();
        for i in 0..3 {
            manager
                .create_record(
                    agent_id,
                    LearningSource::Evaluation,
                    LearningType::Skill,
                    &format!("skill-{i}"),
                    "desc",
                    Value::Null,
                    Value::Null,
                    "system",
                )
                .await
                .unwrap();
        }
        let snap = manager.snapshot(agent_id).await.unwrap();
        assert_eq!(snap.total_records, 3);
    }
}