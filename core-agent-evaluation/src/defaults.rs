use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::{
    DimensionStats, Evaluation, EvaluationDimension, EvaluationQuery, EvaluationSnapshot, Score,
    validate_actor,
};
use crate::error::{EvaluationError, EvaluationResult};
use crate::infrastructure::EvaluationStore;

#[derive(Default)]
pub struct InMemoryEvaluationStore {
    evaluations: RwLock<Vec<Evaluation>>,
}

#[async_trait]
impl EvaluationStore for InMemoryEvaluationStore {
    async fn record(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()> {
        validate_actor("evaluation writer", actor)?;
        evaluation.validate()?;
        let mut evals = self
            .evaluations
            .write()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        if evals.iter().any(|e| e.id == evaluation.id) {
            return Err(EvaluationError::Conflict(
                "evaluation already exists".into(),
            ));
        }
        evals.push(evaluation.clone());
        Ok(())
    }

    async fn update(&self, evaluation: &Evaluation, actor: &str) -> EvaluationResult<()> {
        validate_actor("evaluation writer", actor)?;
        evaluation.validate()?;
        let mut evals = self
            .evaluations
            .write()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        if let Some(pos) = evals.iter().position(|e| e.id == evaluation.id) {
            evals[pos] = evaluation.clone();
            Ok(())
        } else {
            Err(EvaluationError::NotFound(evaluation.id.to_string()))
        }
    }

    async fn find(&self, id: Uuid) -> EvaluationResult<Option<Evaluation>> {
        let evals = self
            .evaluations
            .read()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        Ok(evals.iter().find(|e| e.id == id).cloned())
    }

    async fn list(&self, query: &EvaluationQuery) -> EvaluationResult<Vec<Evaluation>> {
        query.validate()?;
        let evals = self
            .evaluations
            .read()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        let filtered: Vec<Evaluation> = evals
            .iter()
            .filter(|e| {
                query.agent_id.map_or(true, |a| e.agent_id == a)
                    && query.task_id.map_or(true, |t| e.task_id == t)
                    && query.execution_id.map_or(true, |eid| e.execution_id == eid)
                    && query.passed.map_or(true, |p| e.passed == p)
                    && query
                        .evaluator
                        .as_ref()
                        .map_or(true, |ev| e.evaluator == *ev)
                    && query.from.map_or(true, |f| e.created_at >= f)
                    && query.to.map_or(true, |t| e.created_at <= t)
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect();
        Ok(filtered)
    }

    async fn count(&self, query: &EvaluationQuery) -> EvaluationResult<u64> {
        query.validate()?;
        let evals = self
            .evaluations
            .read()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        let count = evals
            .iter()
            .filter(|e| {
                query.agent_id.map_or(true, |a| e.agent_id == a)
                    && query.task_id.map_or(true, |t| e.task_id == t)
                    && query.passed.map_or(true, |p| e.passed == p)
                    && query
                        .evaluator
                        .as_ref()
                        .map_or(true, |ev| e.evaluator == *ev)
                    && query.from.map_or(true, |f| e.created_at >= f)
                    && query.to.map_or(true, |t| e.created_at <= t)
            })
            .count() as u64;
        Ok(count)
    }

    async fn snapshot(&self, agent_id: Uuid) -> EvaluationResult<EvaluationSnapshot> {
        let evals = self
            .evaluations
            .read()
            .map_err(|_| EvaluationError::Internal("evaluation store lock poisoned".into()))?;
        let agent_evals: Vec<&Evaluation> =
            evals.iter().filter(|e| e.agent_id == agent_id).collect();

        let total = agent_evals.len() as u64;
        let passed = agent_evals.iter().filter(|e| e.passed).count() as u64;
        let avg_score = if total > 0 {
            agent_evals
                .iter()
                .map(|e| f64::from(e.total_score.get()))
                .sum::<f64>()
                / total as f64
        } else {
            0.0
        };

        let mut by_dimension: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for eval in &agent_evals {
            for fb in &eval.feedback {
                by_dimension
                    .entry(fb.dimension.as_str().to_string())
                    .or_default()
                    .push(fb.score.get());
            }
        }
        let by_dimension: BTreeMap<String, DimensionStats> = by_dimension
            .into_iter()
            .map(|(dim, scores)| {
                let count = scores.len() as u64;
                let avg = if count > 0 {
                    scores.iter().map(|s| f64::from(*s)).sum::<f64>() / count as f64
                } else {
                    0.0
                };
                let min = *scores.iter().min().unwrap_or(&0);
                let max = *scores.iter().max().unwrap_or(&0);
                (dim, DimensionStats { average_score: avg, min_score: min, max_score: max, count })
            })
            .collect();

        let mut min_time: Option<DateTime<Utc>> = None;
        let mut max_time: Option<DateTime<Utc>> = None;
        for eval in &agent_evals {
            if min_time.map_or(true, |t| eval.created_at < t) {
                min_time = Some(eval.created_at);
            }
            if max_time.map_or(true, |t| eval.created_at > t) {
                max_time = Some(eval.created_at);
            }
        }

        Ok(EvaluationSnapshot {
            agent_id,
            total_evaluations: total,
            passed_count: passed,
            average_score: (avg_score * 100.0).round() / 100.0,
            by_dimension,
            from: min_time,
            to: max_time,
        })
    }
}

pub struct NoopEvaluationObserver;

impl crate::infrastructure::EvaluationObserver for NoopEvaluationObserver {
    fn on_evaluation(&self, _evaluation: &Evaluation) {}
}