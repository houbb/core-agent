use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::defaults::InMemoryEvaluationStore;
use crate::domain::{
    Evaluation, EvaluationCriteria, EvaluationFeedback, EvaluationQuery, EvaluationSnapshot, Score,
    validate_actor,
};
use crate::error::{EvaluationError, EvaluationResult};
use crate::infrastructure::{EvaluationObserver, EvaluationStore};

pub struct EvaluationManagerBuilder {
    store: Arc<dyn EvaluationStore>,
    observers: Vec<Arc<dyn EvaluationObserver>>,
}

impl Default for EvaluationManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryEvaluationStore::default()),
            observers: Vec::new(),
        }
    }
}

impl EvaluationManagerBuilder {
    pub fn store(mut self, value: Arc<dyn EvaluationStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn EvaluationObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> EvaluationManager {
        EvaluationManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct EvaluationManager {
    store: Arc<dyn EvaluationStore>,
    observers: Vec<Arc<dyn EvaluationObserver>>,
}

impl EvaluationManager {
    pub fn builder() -> EvaluationManagerBuilder {
        EvaluationManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn EvaluationStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn create_evaluation(
        &self,
        agent_id: Uuid,
        task_id: Uuid,
        execution_id: Uuid,
        criteria: Vec<EvaluationCriteria>,
        evaluator: &str,
    ) -> EvaluationResult<Evaluation> {
        let eval = Evaluation::new(agent_id, task_id, execution_id, criteria, evaluator)?;
        self.store.record(&eval, evaluator).await?;
        for observer in &self.observers {
            observer.on_evaluation(&eval);
        }
        Ok(eval)
    }

    pub async fn record_feedback(
        &self,
        evaluation_id: Uuid,
        feedback: EvaluationFeedback,
        actor: &str,
    ) -> EvaluationResult<Evaluation> {
        validate_actor("feedback actor", actor)?;
        let mut eval = self
            .store
            .find(evaluation_id)
            .await?
            .ok_or_else(|| EvaluationError::NotFound(evaluation_id.to_string()))?;
        eval.record_feedback(feedback)?;
        eval.validate()?;
        self.store.update(&eval, actor).await?;
        for observer in &self.observers {
            observer.on_evaluation(&eval);
        }
        Ok(eval)
    }

    pub async fn find(&self, id: Uuid) -> EvaluationResult<Option<Evaluation>> {
        self.store.find(id).await
    }

    pub async fn list(&self, query: &EvaluationQuery) -> EvaluationResult<Vec<Evaluation>> {
        self.store.list(query).await
    }

    pub async fn count(&self, query: &EvaluationQuery) -> EvaluationResult<u64> {
        self.store.count(query).await
    }

    pub async fn snapshot(&self, agent_id: Uuid) -> EvaluationResult<EvaluationSnapshot> {
        self.store.snapshot(agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_and_list_evaluations() {
        let manager = EvaluationManager::builder().build();
        let agent_id = Uuid::new_v4();
        let criteria = vec![
            EvaluationCriteria::new(
                crate::EvaluationDimension::Correctness,
                "correctness",
                "Is the result correct?",
                1.0,
            )
            .unwrap(),
        ];
        let eval = manager
            .create_evaluation(agent_id, Uuid::new_v4(), Uuid::new_v4(), criteria, "judge-agent")
            .await
            .unwrap();
        assert_eq!(eval.agent_id, agent_id);
        assert_eq!(eval.total_score.get(), 0);

        let evals = manager
            .list(&EvaluationQuery {
                agent_id: Some(agent_id),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(evals.len(), 1);
    }

    #[tokio::test]
    async fn snapshot_aggregates_correctly() {
        let manager = EvaluationManager::builder().build();
        let agent_id = Uuid::new_v4();
        for i in 0..3 {
            let criteria = vec![
                EvaluationCriteria::new(
                    crate::EvaluationDimension::Correctness,
                    "correctness",
                    "test",
                    1.0,
                )
                .unwrap(),
            ];
            manager
                .create_evaluation(agent_id, Uuid::new_v4(), Uuid::new_v4(), criteria, "judge")
                .await
                .unwrap();
        }
        let snap = manager.snapshot(agent_id).await.unwrap();
        assert_eq!(snap.total_evaluations, 3);
    }

    #[tokio::test]
    async fn record_feedback_updates_score() {
        let manager = EvaluationManager::builder().build();
        let criteria = vec![
            EvaluationCriteria::new(
                crate::EvaluationDimension::Correctness,
                "correctness",
                "test",
                1.0,
            )
            .unwrap(),
        ];
        let eval = manager
            .create_evaluation(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), criteria, "judge")
            .await
            .unwrap();

        let feedback = EvaluationFeedback::new(
            crate::EvaluationDimension::Correctness,
            Score::new(85).unwrap(),
            "Good result",
        );
        let updated = manager
            .record_feedback(eval.id, feedback, "judge")
            .await
            .unwrap();
        assert_eq!(updated.total_score.get(), 85);
    }
}