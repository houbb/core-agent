use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{LearningError, LearningResult};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

// ── Learning Source ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LearningSource {
    Evaluation,
    UserFeedback,
    ExecutionTrace,
    ManualReview,
    SystemAnalysis,
}

impl LearningSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Evaluation => "EVALUATION",
            Self::UserFeedback => "USER_FEEDBACK",
            Self::ExecutionTrace => "EXECUTION_TRACE",
            Self::ManualReview => "MANUAL_REVIEW",
            Self::SystemAnalysis => "SYSTEM_ANALYSIS",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "EVALUATION" => Some(Self::Evaluation),
            "USER_FEEDBACK" => Some(Self::UserFeedback),
            "EXECUTION_TRACE" => Some(Self::ExecutionTrace),
            "MANUAL_REVIEW" => Some(Self::ManualReview),
            "SYSTEM_ANALYSIS" => Some(Self::SystemAnalysis),
            _ => None,
        }
    }
}

// ── Learning Type ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LearningType {
    Skill,
    Workflow,
    Prompt,
    Policy,
    Pattern,
}

impl LearningType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "SKILL",
            Self::Workflow => "WORKFLOW",
            Self::Prompt => "PROMPT",
            Self::Policy => "POLICY",
            Self::Pattern => "PATTERN",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "SKILL" => Some(Self::Skill),
            "WORKFLOW" => Some(Self::Workflow),
            "PROMPT" => Some(Self::Prompt),
            "POLICY" => Some(Self::Policy),
            "PATTERN" => Some(Self::Pattern),
            _ => None,
        }
    }
}

// ── Learning Status ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LearningStatus {
    Candidate,
    Reviewing,
    Approved,
    Applied,
    Rejected,
    Archived,
}

impl LearningStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "CANDIDATE",
            Self::Reviewing => "REVIEWING",
            Self::Approved => "APPROVED",
            Self::Applied => "APPLIED",
            Self::Rejected => "REJECTED",
            Self::Archived => "ARCHIVED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "CANDIDATE" => Some(Self::Candidate),
            "REVIEWING" => Some(Self::Reviewing),
            "APPROVED" => Some(Self::Approved),
            "APPLIED" => Some(Self::Applied),
            "REJECTED" => Some(Self::Rejected),
            "ARCHIVED" => Some(Self::Archived),
            _ => None,
        }
    }
}

// ── Learning Record ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningRecord {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub source: LearningSource,
    pub learning_type: LearningType,
    pub status: LearningStatus,
    pub title: String,
    pub description: String,
    pub experience: Value,
    pub improvement: Value,
    pub confidence: f64,
    pub source_id: Option<Uuid>,
    pub metadata: BTreeMap<String, String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LearningRecord {
    pub fn new(
        agent_id: Uuid,
        source: LearningSource,
        learning_type: LearningType,
        title: impl Into<String>,
        description: impl Into<String>,
        experience: Value,
        improvement: Value,
        actor: impl Into<String>,
    ) -> LearningResult<Self> {
        let title = title.into();
        let description = description.into();
        let actor = actor.into();
        if title.trim().is_empty() || title.len() > 256 {
            return Err(LearningError::Validation(
                "title must contain 1..=256 characters".into(),
            ));
        }
        if description.trim().is_empty() || description.len() > 4096 {
            return Err(LearningError::Validation(
                "description must contain 1..=4096 characters".into(),
            ));
        }
        validate_actor(&actor)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            agent_id,
            source,
            learning_type,
            status: LearningStatus::Candidate,
            title,
            description,
            experience,
            improvement,
            confidence: 0.0,
            source_id: None,
            metadata: BTreeMap::new(),
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> LearningResult<()> {
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(LearningError::Validation(
                "version or timestamps are invalid".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(LearningError::Validation(
                "confidence must be 0.0..=1.0".into(),
            ));
        }
        if serde_json::to_vec(self)?.len() > MAX_DOCUMENT_BYTES {
            return Err(LearningError::Validation(
                "serialized record exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }
}

// ── Learning Query ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LearningQuery {
    pub agent_id: Option<Uuid>,
    pub learning_type: Option<LearningType>,
    pub status: Option<LearningStatus>,
    pub source: Option<LearningSource>,
    pub confidence_min: Option<f64>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for LearningQuery {
    fn default() -> Self {
        Self {
            agent_id: None,
            learning_type: None,
            status: None,
            source: None,
            confidence_min: None,
            from: None,
            to: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl LearningQuery {
    pub fn validate(&self) -> LearningResult<()> {
        if self.limit == 0 || self.limit > 10000 {
            return Err(LearningError::Validation(
                "query limit must be within 1..=10000".into(),
            ));
        }
        Ok(())
    }
}

// ── Learning Snapshot ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSnapshot {
    pub agent_id: Uuid,
    pub total_records: u64,
    pub by_type: BTreeMap<String, u64>,
    pub by_status: BTreeMap<String, u64>,
    pub avg_confidence: f64,
    pub applied_count: u64,
}

// ── Validation Helpers ────────────────────────────────────────────────

pub(crate) fn validate_actor(value: &str) -> LearningResult<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(LearningError::Validation(
            "actor must contain 1..=256 safe characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_record_passes_validate() {
        let record = LearningRecord::new(
            Uuid::new_v4(),
            LearningSource::Evaluation,
            LearningType::Skill,
            "redis-slowlog",
            "Always check slowlog first for Redis issues",
            serde_json::json!({"observation": "redis slow query"}),
            serde_json::json!({"skill": "redis-diagnosis"}),
            "system",
        )
        .unwrap();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn empty_title_is_rejected() {
        let result = LearningRecord::new(
            Uuid::new_v4(),
            LearningSource::Evaluation,
            LearningType::Skill,
            "",
            "desc",
            Value::Null,
            Value::Null,
            "system",
        );
        assert!(result.is_err());
    }

    #[test]
    fn status_roundtrip() {
        for status in &[
            LearningStatus::Candidate,
            LearningStatus::Approved,
            LearningStatus::Applied,
        ] {
            assert_eq!(LearningStatus::parse(status.as_str()), Some(*status));
        }
    }

    #[test]
    fn source_roundtrip() {
        for source in &[
            LearningSource::Evaluation,
            LearningSource::UserFeedback,
            LearningSource::ExecutionTrace,
        ] {
            assert_eq!(LearningSource::parse(source.as_str()), Some(*source));
        }
    }

    #[test]
    fn confidence_bounds() {
        let mut record = LearningRecord::new(
            Uuid::new_v4(),
            LearningSource::Evaluation,
            LearningType::Skill,
            "test",
            "desc",
            Value::Null,
            Value::Null,
            "system",
        )
        .unwrap();
        record.confidence = 1.5;
        assert!(record.validate().is_err());
        record.confidence = 0.5;
        assert!(record.validate().is_ok());
    }
}