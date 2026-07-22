use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{AutonomousError, AutonomousResult};

// ── Autonomy Level (L0-L4) ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyLevel {
    L0Suggest,
    L1AutoAnalyze,
    L2AutoExecuteLowRisk,
    L3AutoExecuteProduction,
    L4FullAutonomous,
}

impl AutonomyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::L0Suggest => "L0_SUGGEST",
            Self::L1AutoAnalyze => "L1_AUTO_ANALYZE",
            Self::L2AutoExecuteLowRisk => "L2_AUTO_EXECUTE_LOW_RISK",
            Self::L3AutoExecuteProduction => "L3_AUTO_EXECUTE_PRODUCTION",
            Self::L4FullAutonomous => "L4_FULL_AUTONOMOUS",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "L0_SUGGEST" => Some(Self::L0Suggest),
            "L1_AUTO_ANALYZE" => Some(Self::L1AutoAnalyze),
            "L2_AUTO_EXECUTE_LOW_RISK" => Some(Self::L2AutoExecuteLowRisk),
            "L3_AUTO_EXECUTE_PRODUCTION" => Some(Self::L3AutoExecuteProduction),
            "L4_FULL_AUTONOMOUS" => Some(Self::L4FullAutonomous),
            _ => None,
        }
    }
}

// ── Trigger Type ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerType {
    Event,
    Schedule,
    Goal,
}

impl TriggerType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Event => "EVENT",
            Self::Schedule => "SCHEDULE",
            Self::Goal => "GOAL",
        }
    }
}

// ── Loop Status ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomousLoopStatus {
    Idle,
    Observing,
    Analyzing,
    Planning,
    Acting,
    Evaluating,
    Learning,
}

impl AutonomousLoopStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "IDLE",
            Self::Observing => "OBSERVING",
            Self::Analyzing => "ANALYZING",
            Self::Planning => "PLANNING",
            Self::Acting => "ACTING",
            Self::Evaluating => "EVALUATING",
            Self::Learning => "LEARNING",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "IDLE" => Some(Self::Idle),
            "OBSERVING" => Some(Self::Observing),
            "ANALYZING" => Some(Self::Analyzing),
            "PLANNING" => Some(Self::Planning),
            "ACTING" => Some(Self::Acting),
            "EVALUATING" => Some(Self::Evaluating),
            "LEARNING" => Some(Self::Learning),
            _ => None,
        }
    }
}

// ── Autonomous Goal ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousGoal {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub description: String,
    pub priority: u8,
    pub constraints: Value,
    pub deadline: Option<DateTime<Utc>>,
    pub autonomy_level: AutonomyLevel,
    pub active: bool,
    pub metadata: BTreeMap<String, String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AutonomousGoal {
    pub fn new(
        agent_id: Uuid,
        description: impl Into<String>,
        priority: u8,
        autonomy_level: AutonomyLevel,
        actor: impl Into<String>,
    ) -> AutonomousResult<Self> {
        let description = description.into();
        let actor = actor.into();
        if description.trim().is_empty() || description.len() > 4096 {
            return Err(AutonomousError::Validation(
                "description must contain 1..=4096 characters".into(),
            ));
        }
        if priority > 10 {
            return Err(AutonomousError::Validation(
                "priority must be 0..=10".into(),
            ));
        }
        validate_actor(&actor)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            agent_id,
            description,
            priority,
            constraints: Value::Object(Default::default()),
            deadline: None,
            autonomy_level,
            active: true,
            metadata: BTreeMap::new(),
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> AutonomousResult<()> {
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(AutonomousError::Validation(
                "invalid version or timestamps".into(),
            ));
        }
        Ok(())
    }
}

// ── Autonomous Trigger ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousTrigger {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub trigger_type: TriggerType,
    pub name: String,
    pub config: Value,
    pub active: bool,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AutonomousTrigger {
    pub fn new(
        agent_id: Uuid,
        trigger_type: TriggerType,
        name: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            agent_id,
            trigger_type,
            name: name.into(),
            config: Value::Object(Default::default()),
            active: true,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── Autonomous Loop State ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousLoopState {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub status: AutonomousLoopStatus,
    pub current_cycle: u64,
    pub last_trigger: Option<TriggerType>,
    pub last_trigger_at: Option<DateTime<Utc>>,
    pub autonomy_level: AutonomyLevel,
    pub metadata: BTreeMap<String, String>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AutonomousLoopState {
    pub fn new(agent_id: Uuid, autonomy_level: AutonomyLevel) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            agent_id,
            status: AutonomousLoopStatus::Idle,
            current_cycle: 0,
            last_trigger: None,
            last_trigger_at: None,
            autonomy_level,
            metadata: BTreeMap::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }
}

// ── Query ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AutonomousQuery {
    pub agent_id: Option<Uuid>,
    pub status: Option<AutonomousLoopStatus>,
    pub autonomy_level: Option<AutonomyLevel>,
    pub active: Option<bool>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for AutonomousQuery {
    fn default() -> Self {
        Self {
            agent_id: None,
            status: None,
            autonomy_level: None,
            active: None,
            limit: 100,
            offset: 0,
        }
    }
}

// ── Snapshot ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousSnapshot {
    pub agent_id: Uuid,
    pub total_cycles: u64,
    pub current_status: String,
    pub autonomy_level: String,
    pub active_goals: u64,
}

// ── Validation ────────────────────────────────────────────────────────

pub(crate) fn validate_actor(value: &str) -> AutonomousResult<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(AutonomousError::Validation(
            "actor must contain 1..=256 safe characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_goal() {
        let goal = AutonomousGoal::new(
            Uuid::new_v4(),
            "Keep system stable",
            5,
            AutonomyLevel::L2AutoExecuteLowRisk,
            "system",
        )
        .unwrap();
        assert!(goal.validate().is_ok());
    }

    #[test]
    fn priority_bounds() {
        assert!(
            AutonomousGoal::new(Uuid::new_v4(), "test", 11, AutonomyLevel::L0Suggest, "system")
                .is_err()
        );
    }

    #[test]
    fn level_roundtrip() {
        for l in &[
            AutonomyLevel::L0Suggest,
            AutonomyLevel::L2AutoExecuteLowRisk,
            AutonomyLevel::L4FullAutonomous,
        ] {
            assert_eq!(AutonomyLevel::parse(l.as_str()), Some(*l));
        }
    }

    #[test]
    fn loop_state_creation() {
        let state = AutonomousLoopState::new(Uuid::new_v4(), AutonomyLevel::L1AutoAnalyze);
        assert_eq!(state.status, AutonomousLoopStatus::Idle);
    }
}