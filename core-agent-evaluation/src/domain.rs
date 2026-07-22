use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{EvaluationError, EvaluationResult};

const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

// ── Evaluation Dimension ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvaluationDimension {
    Correctness,
    Quality,
    Safety,
    Cost,
}

impl EvaluationDimension {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Correctness => "CORRECTNESS",
            Self::Quality => "QUALITY",
            Self::Safety => "SAFETY",
            Self::Cost => "COST",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CORRECTNESS" => Some(Self::Correctness),
            "QUALITY" => Some(Self::Quality),
            "SAFETY" => Some(Self::Safety),
            "COST" => Some(Self::Cost),
            _ => None,
        }
    }

    pub fn all() -> &'static [EvaluationDimension] {
        &[Self::Correctness, Self::Quality, Self::Safety, Self::Cost]
    }
}

// ── Evaluation Score (0-100) ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score(u8);

impl Score {
    pub fn new(value: u8) -> EvaluationResult<Self> {
        if value > 100 {
            return Err(EvaluationError::Validation(
                "score must be 0..=100".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn get(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Score {
    type Error = EvaluationError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

// ── Evaluation Criteria ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationCriteria {
    pub dimension: EvaluationDimension,
    pub name: String,
    pub description: String,
    pub weight: f64,
    pub config: Value,
}

impl EvaluationCriteria {
    pub fn new(
        dimension: EvaluationDimension,
        name: impl Into<String>,
        description: impl Into<String>,
        weight: f64,
    ) -> EvaluationResult<Self> {
        let name = name.into();
        let description = description.into();
        if name.trim().is_empty() || name.len() > 256 {
            return Err(EvaluationError::Validation(
                "criteria name must contain 1..=256 characters".into(),
            ));
        }
        if description.trim().is_empty() || description.len() > 4096 {
            return Err(EvaluationError::Validation(
                "criteria description must contain 1..=4096 characters".into(),
            ));
        }
        if !(0.0..=1.0).contains(&weight) {
            return Err(EvaluationError::Validation(
                "criteria weight must be 0.0..=1.0".into(),
            ));
        }
        Ok(Self {
            dimension,
            name,
            description,
            weight,
            config: Value::Object(Default::default()),
        })
    }
}

// ── Evaluation Feedback ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationFeedback {
    pub dimension: EvaluationDimension,
    pub score: Score,
    pub summary: String,
    pub details: Value,
    pub suggestions: Vec<String>,
}

impl EvaluationFeedback {
    pub fn new(
        dimension: EvaluationDimension,
        score: Score,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            dimension,
            score,
            summary: summary.into(),
            details: Value::Object(Default::default()),
            suggestions: Vec::new(),
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }
}

// ── Evaluation ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub task_id: Uuid,
    pub execution_id: Uuid,
    pub criteria: Vec<EvaluationCriteria>,
    pub feedback: Vec<EvaluationFeedback>,
    pub total_score: Score,
    pub passed: bool,
    pub metadata: BTreeMap<String, String>,
    pub evaluator: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

impl Evaluation {
    pub fn new(
        agent_id: Uuid,
        task_id: Uuid,
        execution_id: Uuid,
        criteria: Vec<EvaluationCriteria>,
        evaluator: impl Into<String>,
    ) -> EvaluationResult<Self> {
        let evaluator = evaluator.into();
        if criteria.is_empty() || criteria.len() > 16 {
            return Err(EvaluationError::Validation(
                "criteria must contain 1..=16 items".into(),
            ));
        }
        validate_actor("evaluator", &evaluator)?;
        let total_weight: f64 = criteria.iter().map(|c| c.weight).sum();
        if (total_weight - 1.0).abs() > 0.001 {
            return Err(EvaluationError::Validation(
                "criteria weights must sum to 1.0".into(),
            ));
        }
        let mut seen = BTreeMap::new();
        for c in &criteria {
            if seen.contains_key(&(c.dimension, c.name.clone())) {
                return Err(EvaluationError::Validation(
                    "duplicate criteria (dimension + name)".into(),
                ));
            }
            seen.insert((c.dimension, c.name.clone()), true);
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            agent_id,
            task_id,
            execution_id,
            criteria,
            feedback: Vec::new(),
            total_score: Score::new(0)?,
            passed: false,
            metadata: BTreeMap::new(),
            evaluator: evaluator.clone(),
            version: 1,
            created_at: now,
        })
    }

    pub fn record_feedback(&mut self, feedback: EvaluationFeedback) -> EvaluationResult<()> {
        // Verify the dimension is in criteria
        if !self.criteria.iter().any(|c| c.dimension == feedback.dimension) {
            return Err(EvaluationError::Validation(format!(
                "dimension {:?} not in criteria",
                feedback.dimension
            )));
        }
        // Replace existing feedback for same dimension
        if let Some(pos) = self.feedback.iter().position(|f| f.dimension == feedback.dimension) {
            self.feedback[pos] = feedback;
        } else {
            self.feedback.push(feedback);
        }
        self.recalc_total();
        self.version += 1;
        Ok(())
    }

    pub fn record_feedback_batch(&mut self, batch: Vec<EvaluationFeedback>) -> EvaluationResult<()> {
        for fb in batch {
            self.record_feedback(fb)?;
        }
        Ok(())
    }

    pub fn validate(&self) -> EvaluationResult<()> {
        if self.version == 0 {
            return Err(EvaluationError::Validation(
                "evaluation version must be >= 1".into(),
            ));
        }
        for fb in &self.feedback {
            if !self.criteria.iter().any(|c| c.dimension == fb.dimension) {
                return Err(EvaluationError::Validation(format!(
                    "feedback dimension {:?} not in criteria",
                    fb.dimension
                )));
            }
        }
        if serde_json::to_vec(self)?.len() > MAX_DOCUMENT_BYTES {
            return Err(EvaluationError::Validation(
                "serialized evaluation exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }

    pub fn set_passed(&mut self, threshold: u8) {
        self.passed = self.total_score.get() >= threshold;
    }

    fn recalc_total(&mut self) {
        if self.feedback.is_empty() {
            self.total_score = Score::new(0).unwrap();
            self.passed = false;
            return;
        }
        let weighted: f64 = self
            .criteria
            .iter()
            .map(|c| {
                let score = self
                    .feedback
                    .iter()
                    .find(|f| f.dimension == c.dimension)
                    .map(|f| f64::from(f.score.get()))
                    .unwrap_or(0.0);
                score * c.weight
            })
            .sum();
        let total = (weighted.round() as u8).min(100);
        self.total_score = Score::new(total).unwrap();
    }
}

// ── Evaluation Query ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EvaluationQuery {
    pub agent_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub execution_id: Option<Uuid>,
    pub passed: Option<bool>,
    pub evaluator: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for EvaluationQuery {
    fn default() -> Self {
        Self {
            agent_id: None,
            task_id: None,
            execution_id: None,
            passed: None,
            evaluator: None,
            from: None,
            to: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl EvaluationQuery {
    pub fn validate(&self) -> EvaluationResult<()> {
        if self.limit == 0 || self.limit > 10000 {
            return Err(EvaluationError::Validation(
                "query limit must be within 1..=10000".into(),
            ));
        }
        Ok(())
    }
}

// ── Evaluation Snapshot (aggregated stats) ────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationSnapshot {
    pub agent_id: Uuid,
    pub total_evaluations: u64,
    pub passed_count: u64,
    pub average_score: f64,
    pub by_dimension: BTreeMap<String, DimensionStats>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionStats {
    pub average_score: f64,
    pub min_score: u8,
    pub max_score: u8,
    pub count: u64,
}

// ── Validation Helpers ────────────────────────────────────────────────

pub(crate) fn validate_actor(label: &str, value: &str) -> EvaluationResult<()> {
    validate_text(label, value, 256)
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> EvaluationResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(EvaluationError::Validation(format!(
            "{label} must contain 1..={max} safe characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_evaluation() -> Evaluation {
        let criteria = vec![
            EvaluationCriteria::new(
                EvaluationDimension::Correctness,
                "result-correctness",
                "Whether the result is factually correct",
                0.4,
            )
            .unwrap(),
            EvaluationCriteria::new(
                EvaluationDimension::Quality,
                "code-quality",
                "Code quality and maintainability",
                0.3,
            )
            .unwrap(),
            EvaluationCriteria::new(
                EvaluationDimension::Safety,
                "safety-check",
                "Whether sensitive info is leaked",
                0.2,
            )
            .unwrap(),
            EvaluationCriteria::new(
                EvaluationDimension::Cost,
                "cost-efficiency",
                "Token cost efficiency",
                0.1,
            )
            .unwrap(),
        ];
        Evaluation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            criteria,
            "judge-agent",
        )
        .unwrap()
    }

    #[test]
    fn valid_evaluation_passes_validate() {
        let eval = sample_evaluation();
        assert!(eval.validate().is_ok());
    }

    #[test]
    fn empty_criteria_is_rejected() {
        let result = Evaluation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec![],
            "judge",
        );
        assert!(matches!(result, Err(EvaluationError::Validation(_))));
    }

    #[test]
    fn weights_must_sum_to_one() {
        let criteria = vec![
            EvaluationCriteria::new(
                EvaluationDimension::Correctness,
                "correctness",
                "test",
                0.5,
            )
            .unwrap(),
            EvaluationCriteria::new(
                EvaluationDimension::Quality,
                "quality",
                "test",
                0.3,
            )
            .unwrap(),
        ];
        let result = Evaluation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            criteria,
            "judge",
        );
        assert!(matches!(result, Err(EvaluationError::Validation(_))));
    }

    #[test]
    fn score_bounds_are_validated() {
        assert!(Score::new(0).is_ok());
        assert!(Score::new(100).is_ok());
        assert!(Score::new(101).is_err());
    }

    #[test]
    fn feedback_updates_total_score() {
        let mut eval = sample_evaluation();
        let fb = EvaluationFeedback::new(
            EvaluationDimension::Correctness,
            Score::new(80).unwrap(),
            "Mostly correct",
        );
        eval.record_feedback(fb).unwrap();
        assert_eq!(eval.total_score.get(), 32); // 80 * 0.4 = 32
    }

    #[test]
    fn full_feedback_calculates_weighted_average() {
        let mut eval = sample_evaluation();
        eval.record_feedback(EvaluationFeedback::new(
            EvaluationDimension::Correctness,
            Score::new(100).unwrap(),
            "Perfect",
        ))
        .unwrap();
        eval.record_feedback(EvaluationFeedback::new(
            EvaluationDimension::Quality,
            Score::new(80).unwrap(),
            "Good quality",
        ))
        .unwrap();
        eval.record_feedback(EvaluationFeedback::new(
            EvaluationDimension::Safety,
            Score::new(90).unwrap(),
            "Safe",
        ))
        .unwrap();
        eval.record_feedback(EvaluationFeedback::new(
            EvaluationDimension::Cost,
            Score::new(70).unwrap(),
            "Reasonable cost",
        ))
        .unwrap();
        // 100*0.4 + 80*0.3 + 90*0.2 + 70*0.1 = 40 + 24 + 18 + 7 = 89
        assert_eq!(eval.total_score.get(), 89);
    }

    #[test]
    fn dimension_roundtrip() {
        for d in EvaluationDimension::all() {
            let s = d.as_str();
            let parsed = EvaluationDimension::parse(s).unwrap();
            assert_eq!(*d, parsed);
        }
    }

    #[test]
    fn criteria_weight_bounds() {
        assert!(EvaluationCriteria::new(EvaluationDimension::Correctness, "a", "desc", 0.0).is_ok());
        assert!(EvaluationCriteria::new(EvaluationDimension::Correctness, "a", "desc", 1.0).is_ok());
        assert!(EvaluationCriteria::new(EvaluationDimension::Correctness, "a", "desc", 1.5).is_err());
    }
}