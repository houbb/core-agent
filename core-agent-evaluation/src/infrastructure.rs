use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{Evaluation, EvaluationQuery, EvaluationSnapshot};
use crate::error::EvaluationResult;

#[async_trait]
pub trait EvaluationStore: Send + Sync {
    async fn record(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()>;
    async fn update(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()>;
    async fn find(&self, id: Uuid) -> EvaluationResult<Option<Evaluation>>;
    async fn list(&self, query: &EvaluationQuery) -> EvaluationResult<Vec<Evaluation>>;
    async fn count(&self, query: &EvaluationQuery) -> EvaluationResult<u64>;
    async fn snapshot(&self, agent_id: Uuid) -> EvaluationResult<EvaluationSnapshot>;
}

pub trait EvaluationObserver: Send + Sync {
    fn on_evaluation(&self, evaluation: &Evaluation);
}