use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::domain::*;

/// Manages Reflection lifecycle — create, evaluate, and store results.
pub struct ReflectionManager {
    reflections: RwLock<HashMap<uuid::Uuid, Reflection>>,
}

impl ReflectionManager {
    pub fn new() -> Self {
        Self {
            reflections: RwLock::default(),
        }
    }

    /// Store a reflection result.
    pub async fn store(&self, reflection: Reflection) -> ReflectionResult<()> {
        reflection.validate()?;
        self.reflections
            .write()
            .await
            .insert(reflection.id, reflection);
        Ok(())
    }

    /// Get a reflection by ID.
    pub async fn get(&self, id: uuid::Uuid) -> ReflectionResult<Reflection> {
        self.reflections
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| ReflectionError::Runtime(format!("reflection {id} not found")))
    }

    /// List all reflections for an execution.
    pub async fn list_for_execution(
        &self,
        execution_id: uuid::Uuid,
    ) -> Vec<Reflection> {
        self.reflections
            .read()
            .await
            .values()
            .filter(|r| r.execution_id == execution_id)
            .cloned()
            .collect()
    }

    /// Evaluate an execution result — produces a Reflection.
    /// This is a rule-based evaluator for MVP.
    /// In production, this would use an LLM for deeper analysis.
    pub async fn evaluate(
        &self,
        request: &ReflectionRequest,
    ) -> ReflectionResult<Reflection> {
        let mut reflection = Reflection::new(request.execution_id, 0);

        // Simple rule-based evaluation for MVP
        let result_len = request.result_summary.len();
        let goal_len = request.goal_description.len();

        // Score based on result completeness
        let mut score: u32 = 50; // baseline

        // Length ratio heuristic
        if result_len > 0 && goal_len > 0 {
            let ratio = result_len as f64 / goal_len as f64;
            if ratio >= 0.5 {
                score += 20;
            }
            if ratio >= 1.0 {
                score += 10;
            }
            if ratio >= 2.0 {
                score += 10;
            }
        }

        // Cap at 100
        score = score.min(100);

        reflection.score = score;
        reflection.criteria = request.criteria.clone();

        // Generate issues based on score
        if score < 60 {
            reflection.issues.push("Execution result is incomplete".into());
            reflection.suggestions.push("Consider adding more detail to the execution".into());
        } else if score < 80 {
            reflection.issues.push("Execution result could be improved".into());
            reflection.suggestions.push("Review the result for completeness".into());
        }

        if score < 100 {
            reflection.suggestions.push("Consider verifying the result with additional checks".into());
        }

        reflection.validate()?;
        self.store(reflection.clone()).await?;
        Ok(reflection)
    }

    /// Check if a reflection passes the score threshold.
    pub fn passes_threshold(reflection: &Reflection, threshold: u32) -> bool {
        reflection.score >= threshold
    }

    /// Check if retry is allowed.
    pub fn can_retry(request: &ReflectionRequest, attempts: u32) -> bool {
        attempts < request.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_evaluate() {
        let manager = ReflectionManager::new();
        let request = ReflectionRequest {
            execution_id: uuid::Uuid::new_v4(),
            result_summary: "Implemented OAuth login with Google and GitHub providers. \
                Added tests for all endpoints. Updated documentation.".into(),
            goal_description: "Implement OAuth login".into(),
            criteria: vec!["correctness".into(), "completeness".into()],
            max_retries: 3,
            min_score_threshold: 70,
        };

        let reflection = manager.evaluate(&request).await.unwrap();
        assert!(reflection.score >= 50);
        assert!(reflection.score <= 100);
        assert!(!reflection.criteria.is_empty());
    }

    #[tokio::test]
    async fn test_empty_result_scores_low() {
        let manager = ReflectionManager::new();
        let request = ReflectionRequest {
            execution_id: uuid::Uuid::new_v4(),
            result_summary: "".into(),
            goal_description: "A complex goal that requires significant work".into(),
            criteria: vec!["completeness".into()],
            max_retries: 3,
            min_score_threshold: 70,
        };

        let reflection = manager.evaluate(&request).await.unwrap();
        assert!(reflection.score <= 70);
        assert!(!reflection.issues.is_empty());
    }

    #[test]
    fn test_threshold() {
        let reflection = Reflection::new(uuid::Uuid::new_v4(), 85);
        assert!(ReflectionManager::passes_threshold(&reflection, 70));
        assert!(!ReflectionManager::passes_threshold(&reflection, 90));
    }

    #[test]
    fn test_can_retry() {
        let request = ReflectionRequest {
            execution_id: uuid::Uuid::new_v4(),
            result_summary: "test".into(),
            goal_description: "goal".into(),
            criteria: vec![],
            max_retries: 3,
            min_score_threshold: 70,
        };
        assert!(ReflectionManager::can_retry(&request, 0));
        assert!(ReflectionManager::can_retry(&request, 2));
        assert!(!ReflectionManager::can_retry(&request, 3));
    }
}