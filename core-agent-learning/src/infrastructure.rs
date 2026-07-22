use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{LearningRecord, LearningQuery, LearningSnapshot};
use crate::error::LearningResult;

#[async_trait]
pub trait LearningStore: Send + Sync {
    async fn record(&self, record: &LearningRecord, actor: &str) -> LearningResult<()>;
    async fn update(&self, record: &LearningRecord, actor: &str) -> LearningResult<()>;
    async fn find(&self, id: Uuid) -> LearningResult<Option<LearningRecord>>;
    async fn list(&self, query: &LearningQuery) -> LearningResult<Vec<LearningRecord>>;
    async fn count(&self, query: &LearningQuery) -> LearningResult<u64>;
    async fn snapshot(&self, agent_id: Uuid) -> LearningResult<LearningSnapshot>;
}

pub trait LearningObserver: Send + Sync {
    fn on_learning(&self, record: &LearningRecord);
}