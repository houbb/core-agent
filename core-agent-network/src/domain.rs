use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{NetworkError, NetworkResult};

// ── Agent Status ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    Online,
    Busy,
    Offline,
}

impl AgentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "ONLINE",
            Self::Busy => "BUSY",
            Self::Offline => "OFFLINE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ONLINE" => Some(Self::Online),
            "BUSY" => Some(Self::Busy),
            "OFFLINE" => Some(Self::Offline),
            _ => None,
        }
    }
}

// ── Trust Level ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrustLevel {
    Untrusted,
    Low,
    Medium,
    High,
}

impl TrustLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Untrusted => "UNTRUSTED",
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "UNTRUSTED" => Some(Self::Untrusted),
            "LOW" => Some(Self::Low),
            "MEDIUM" => Some(Self::Medium),
            "HIGH" => Some(Self::High),
            _ => None,
        }
    }
}

// ── Agent Registration ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub name: String,
    pub capabilities: BTreeSet<String>,
    pub status: AgentStatus,
    pub trust_level: TrustLevel,
    pub endpoint: Option<String>,
    pub reputation: f64,
    pub metadata: BTreeMap<String, String>,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AgentRegistration {
    pub fn new(
        agent_id: Uuid,
        name: impl Into<String>,
        actor: impl Into<String>,
    ) -> NetworkResult<Self> {
        let name = name.into();
        let actor = actor.into();
        if name.trim().is_empty() || name.len() > 256 {
            return Err(NetworkError::Validation(
                "name must contain 1..=256 characters".into(),
            ));
        }
        validate_actor(&actor)?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            agent_id,
            name,
            capabilities: BTreeSet::new(),
            status: AgentStatus::Offline,
            trust_level: TrustLevel::Medium,
            endpoint: None,
            reputation: 0.0,
            metadata: BTreeMap::new(),
            version: 1,
            actor,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> NetworkResult<()> {
        if self.version == 0 || self.updated_at < self.created_at {
            return Err(NetworkError::Validation(
                "invalid version or timestamps".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.reputation) {
            return Err(NetworkError::Validation(
                "reputation must be 0.0..=1.0".into(),
            ));
        }
        Ok(())
    }
}

// ── Discovery Request ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryRequest {
    pub capability: String,
    pub min_reputation: Option<f64>,
    pub max_results: Option<usize>,
}

// ── Network Query ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetworkQuery {
    pub capability: Option<String>,
    pub status: Option<AgentStatus>,
    pub trust_level: Option<TrustLevel>,
    pub reputation_min: Option<f64>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for NetworkQuery {
    fn default() -> Self {
        Self {
            capability: None,
            status: None,
            trust_level: None,
            reputation_min: None,
            limit: 100,
            offset: 0,
        }
    }
}

// ── Network Snapshot ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSnapshot {
    pub total_agents: u64,
    pub online_count: u64,
    pub by_capability: BTreeMap<String, u64>,
    pub avg_reputation: f64,
}

// ── Validation ────────────────────────────────────────────────────────

pub(crate) fn validate_actor(value: &str) -> NetworkResult<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(NetworkError::Validation(
            "actor must contain 1..=256 safe characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_registration() {
        let reg = AgentRegistration::new(Uuid::new_v4(), "db-agent", "system").unwrap();
        assert!(reg.validate().is_ok());
    }

    #[test]
    fn empty_name_rejected() {
        assert!(AgentRegistration::new(Uuid::new_v4(), "", "system").is_err());
    }

    #[test]
    fn status_roundtrip() {
        for s in &[AgentStatus::Online, AgentStatus::Busy, AgentStatus::Offline] {
            assert_eq!(AgentStatus::parse(s.as_str()), Some(*s));
        }
    }

    #[test]
    fn trust_level_roundtrip() {
        for t in &[TrustLevel::Untrusted, TrustLevel::Medium, TrustLevel::High] {
            assert_eq!(TrustLevel::parse(t.as_str()), Some(*t));
        }
    }
}