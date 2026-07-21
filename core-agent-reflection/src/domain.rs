/// Reflection — Agent self-evaluation result after execution.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Reflection {
    pub id: uuid::Uuid,
    /// ID of the execution being reflected on
    pub execution_id: uuid::Uuid,
    /// Overall score 0-100
    pub score: u32,
    /// List of issues found
    pub issues: Vec<String>,
    /// Improvement suggestions
    pub suggestions: Vec<String>,
    /// Reflection criteria (code_quality, test_coverage, correctness, completeness)
    pub criteria: Vec<String>,
    /// When the reflection was created
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Reflection {
    pub fn new(
        execution_id: uuid::Uuid,
        score: u32,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            execution_id,
            score,
            issues: Vec::new(),
            suggestions: Vec::new(),
            criteria: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn validate(&self) -> Result<(), ReflectionError> {
        if self.score > 100 {
            return Err(ReflectionError::Validation("score must be 0-100".into()));
        }
        if self.issues.len() > 64 {
            return Err(ReflectionError::Validation(
                "too many issues (max 64)".into(),
            ));
        }
        if self.suggestions.len() > 64 {
            return Err(ReflectionError::Validation(
                "too many suggestions (max 64)".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReflectionRequest {
    pub execution_id: uuid::Uuid,
    /// Execution result text to reflect on
    pub result_summary: String,
    /// Expected goal / plan description
    pub goal_description: String,
    /// Specific criteria to evaluate
    pub criteria: Vec<String>,
    /// Max retry count for reflection loop
    pub max_retries: u32,
    /// Minimum score threshold (0-100)
    pub min_score_threshold: u32,
}

impl Default for ReflectionRequest {
    fn default() -> Self {
        Self {
            execution_id: uuid::Uuid::nil(),
            result_summary: String::new(),
            goal_description: String::new(),
            criteria: vec!["correctness".into(), "completeness".into()],
            max_retries: 3,
            min_score_threshold: 70,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ReflectionError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

pub type ReflectionResult<T> = Result<T, ReflectionError>;
