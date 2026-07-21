use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use core_agent_subagent::{AgentRole, SubAgentStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{OrchestratorError, OrchestratorResult};

const MAX_ITEMS: usize = 256;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_TEXT: usize = 4096;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

// ── OrchestrationStrategy ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationStrategy {
    Sequential,
    Parallel,
    Supervisor,
    Debate,
}
string_enum!(OrchestrationStrategy {
    Sequential => "sequential",
    Parallel => "parallel",
    Supervisor => "supervisor",
    Debate => "debate",
});

// ── OrchestrationStatus ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrchestrationStatus {
    Created,
    Running,
    Completed,
    Failed,
}
string_enum!(OrchestrationStatus {
    Created => "CREATED",
    Running => "RUNNING",
    Completed => "COMPLETED",
    Failed => "FAILED",
});

impl OrchestrationStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

// ── AgentInstanceRef ──

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInstanceRef {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub role: AgentRole,
}

// ── WorkerResult ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerResult {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub role: AgentRole,
    pub status: SubAgentStatus,
    pub finding: String,
    pub confidence: f64,
}

// ── AggregatedResult ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub summary: String,
    pub confidence: f64,
    pub details: Vec<WorkerResult>,
    pub metadata: BTreeMap<String, Value>,
}

// ── Orchestration ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Orchestration {
    pub id: Uuid,
    pub goal: String,
    pub supervisor_agent_id: Uuid,
    pub strategy: OrchestrationStrategy,
    pub status: OrchestrationStatus,
    pub worker_agents: Vec<AgentInstanceRef>,
    pub result: Option<AggregatedResult>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Orchestration {
    pub fn new(
        goal: String,
        strategy: OrchestrationStrategy,
        supervisor_agent_id: Uuid,
        actor: String,
    ) -> OrchestratorResult<Self> {
        let now = Utc::now();
        let value = Self {
            id: Uuid::new_v4(),
            goal,
            supervisor_agent_id,
            strategy,
            status: OrchestrationStatus::Created,
            worker_agents: Vec::new(),
            result: None,
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> OrchestratorResult<()> {
        if self.goal.trim().is_empty() || self.goal.len() > MAX_TEXT {
            return Err(OrchestratorError::Validation(
                "orchestration goal must contain 1..=4096 safe characters".into(),
            ));
        }
        if self.worker_agents.len() > MAX_ITEMS {
            return Err(OrchestratorError::Validation(
                "orchestration worker count exceeds 256".into(),
            ));
        }
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(OrchestratorError::Validation(
                "orchestration version or timestamps are invalid".into(),
            ));
        }
        if self.status == OrchestrationStatus::Completed && self.result.is_none() {
            return Err(OrchestratorError::Validation(
                "completed orchestration must have a result".into(),
            ));
        }
        if let Some(result) = &self.result {
            if result.confidence < 0.0 || result.confidence > 1.0 {
                return Err(OrchestratorError::Validation(
                    "confidence must be within 0.0..=1.0".into(),
                ));
            }
        }
        if serde_json::to_vec(self)?.len() > MAX_DOCUMENT_BYTES {
            return Err(OrchestratorError::Validation(
                "serialized Orchestration exceeds 1 MiB".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_orchestration() {
        let result = Orchestration::new(
            "analyze outage".into(),
            OrchestrationStrategy::Supervisor,
            Uuid::new_v4(),
            "system".into(),
        ).unwrap();
        assert_eq!(result.status, OrchestrationStatus::Created);
        assert!(result.worker_agents.is_empty());
    }

    #[test]
    fn strategy_roundtrip() {
        assert_eq!(OrchestrationStrategy::parse("sequential"), Some(OrchestrationStrategy::Sequential));
        assert_eq!(OrchestrationStrategy::parse("parallel"), Some(OrchestrationStrategy::Parallel));
        assert_eq!(OrchestrationStrategy::parse("supervisor"), Some(OrchestrationStrategy::Supervisor));
        assert_eq!(OrchestrationStrategy::parse("debate"), Some(OrchestrationStrategy::Debate));
    }

    #[test]
    fn empty_goal_rejected() {
        let result = Orchestration::new(
            "".into(),
            OrchestrationStrategy::Sequential,
            Uuid::new_v4(),
            "system".into(),
        );
        assert!(result.is_err());
    }
}