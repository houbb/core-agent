use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{CostError, CostResult};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BudgetScope {
    User,
    Agent,
    Project,
    Organization,
}

impl BudgetScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "USER",
            Self::Agent => "AGENT",
            Self::Project => "PROJECT",
            Self::Organization => "ORGANIZATION",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "USER" => Some(Self::User),
            "AGENT" => Some(Self::Agent),
            "PROJECT" => Some(Self::Project),
            "ORGANIZATION" => Some(Self::Organization),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BudgetState {
    Active,
    Exceeded,
    Suspended,
}

impl BudgetState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Exceeded => "EXCEEDED",
            Self::Suspended => "SUSPENDED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "ACTIVE" => Some(Self::Active),
            "EXCEEDED" => Some(Self::Exceeded),
            "SUSPENDED" => Some(Self::Suspended),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub organization_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub model_key: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub price_per_token_micros: u64,
    pub amount_micros: u64,
    pub currency: String,
    pub event_key: String,
    pub actor: String,
    pub occurred_at: DateTime<Utc>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
}

impl CostRecord {
    pub fn new(
        tenant_id: Uuid,
        event_key: impl Into<String>,
        currency: impl Into<String>,
        amount_micros: u64,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let currency = currency.into();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            organization_id: None,
            project_id: None,
            agent_id: None,
            session_id: None,
            model_key: None,
            input_tokens: 0,
            output_tokens: 0,
            price_per_token_micros: 0,
            amount_micros,
            currency: currency.clone(),
            event_key: event_key.into(),
            actor: actor.into(),
            occurred_at: now,
            version: 1,
            created_at: now,
        }
    }

    pub fn validate(&self) -> CostResult<()> {
        validate_actor("cost actor", &self.actor)?;
        validate_key("cost event key", &self.event_key)?;
        if self.currency.len() != 3
            || !self.currency.bytes().all(|b| b.is_ascii_uppercase())
            || (self.amount_micros == 0 && self.input_tokens == 0 && self.output_tokens == 0)
        {
            return Err(CostError::Validation(
                "cost currency or usage is invalid".into(),
            ));
        }
        if self.version == 0 || self.created_at < self.occurred_at {
            return Err(CostError::Validation(
                "cost record version or timestamps are invalid".into(),
            ));
        }
        if let Some(model) = &self.model_key {
            validate_key("cost model key", model)?;
        }
        validate_size(self, "cost record")
    }

    pub fn with_agent(mut self, agent_id: Uuid) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    pub fn with_model(mut self, model_key: impl Into<String>) -> Self {
        self.model_key = Some(model_key.into());
        self
    }

    pub fn with_tokens(mut self, input: u64, output: u64, price_per_token_micros: u64) -> Self {
        self.input_tokens = input;
        self.output_tokens = output;
        self.price_per_token_micros = price_per_token_micros;
        self.amount_micros = (input + output) * price_per_token_micros;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Budget {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub scope: BudgetScope,
    pub scope_id: String,
    pub monthly_limit_micros: u64,
    pub monthly_used_micros: u64,
    pub currency: String,
    pub alert_threshold: u8,
    pub state: BudgetState,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Budget {
    pub fn new(
        tenant_id: Uuid,
        scope: BudgetScope,
        scope_id: impl Into<String>,
        monthly_limit_micros: u64,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            scope,
            scope_id: scope_id.into(),
            monthly_limit_micros,
            monthly_used_micros: 0,
            currency: "USD".into(),
            alert_threshold: 80,
            state: BudgetState::Active,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> CostResult<()> {
        validate_actor("budget actor", &self.actor)?;
        validate_key("budget scope id", &self.scope_id)?;
        if self.monthly_limit_micros == 0
            || self.alert_threshold > 100
            || self.version == 0
            || self.updated_at < self.created_at
        {
            return Err(CostError::Validation(
                "budget bounds are invalid".into(),
            ));
        }
        if self.currency.len() != 3 || !self.currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(CostError::Validation(
                "budget currency must be a 3-letter ISO code".into(),
            ));
        }
        validate_size(self, "budget")
    }

    pub fn remaining_micros(&self) -> u64 {
        self.monthly_limit_micros.saturating_sub(self.monthly_used_micros)
    }

    pub fn usage_percent(&self) -> u8 {
        if self.monthly_limit_micros == 0 {
            return 0;
        }
        let pct = (self.monthly_used_micros as f64 / self.monthly_limit_micros as f64 * 100.0) as u8;
        pct.min(100)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CostSummary {
    pub tenant_id: Uuid,
    pub total_amount_micros: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub by_currency: BTreeMap<String, u64>,
    pub by_agent: BTreeMap<String, u64>,
    pub by_model: BTreeMap<String, u64>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub record_count: u64,
}

pub(crate) fn validate_actor(label: &str, value: &str) -> CostResult<()> {
    validate_text(label, value, 256)
}

pub(crate) fn validate_key(label: &str, value: &str) -> CostResult<()> {
    if value.is_empty() || value.len() > 386 || value.chars().any(char::is_whitespace) {
        return Err(CostError::Validation(format!(
            "{label} must be a safe identifier"
        )));
    }
    Ok(())
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> CostResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(CostError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_size<T: Serialize>(value: &T, label: &str) -> CostResult<()> {
    if serde_json::to_vec(value)?.len() > MAX_DOCUMENT_BYTES {
        return Err(CostError::Validation(format!(
            "{label} exceeds {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_cost_record_passes_validate() {
        let record = CostRecord::new(Uuid::new_v4(), "evt-001", "USD", 1000, "agent");
        assert!(record.validate().is_ok());
    }

    #[test]
    fn zero_amount_and_tokens_is_rejected() {
        let record = CostRecord {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            organization_id: None,
            project_id: None,
            agent_id: None,
            session_id: None,
            model_key: None,
            input_tokens: 0,
            output_tokens: 0,
            price_per_token_micros: 0,
            amount_micros: 0,
            currency: "USD".into(),
            event_key: "test".into(),
            actor: "agent".into(),
            occurred_at: Utc::now(),
            version: 1,
            created_at: Utc::now(),
        };
        assert!(matches!(record.validate(), Err(CostError::Validation(_))));
    }

    #[test]
    fn invalid_currency_is_rejected() {
        let record = CostRecord::new(Uuid::new_v4(), "evt-001", "US", 1000, "agent");
        assert!(matches!(record.validate(), Err(CostError::Validation(_))));
    }

    #[test]
    fn valid_budget_passes_validate() {
        let budget = Budget::new(Uuid::new_v4(), BudgetScope::Project, "proj-1", 1_000_000, "admin");
        assert!(budget.validate().is_ok());
    }

    #[test]
    fn budget_usage_percent_calculation() {
        let mut budget = Budget::new(Uuid::new_v4(), BudgetScope::User, "user-1", 1000, "admin");
        assert_eq!(budget.usage_percent(), 0);
        budget.monthly_used_micros = 500;
        assert_eq!(budget.usage_percent(), 50);
        budget.monthly_used_micros = 1000;
        assert_eq!(budget.usage_percent(), 100);
    }

    #[test]
    fn budget_remaining_micros() {
        let mut budget = Budget::new(Uuid::new_v4(), BudgetScope::Organization, "org-1", 5000, "admin");
        assert_eq!(budget.remaining_micros(), 5000);
        budget.monthly_used_micros = 3000;
        assert_eq!(budget.remaining_micros(), 2000);
        budget.monthly_used_micros = 6000;
        assert_eq!(budget.remaining_micros(), 0);
    }

    #[test]
    fn builder_methods_work() {
        let record = CostRecord::new(Uuid::new_v4(), "evt-001", "USD", 1000, "agent")
            .with_agent(Uuid::new_v4())
            .with_session(Uuid::new_v4())
            .with_model("gpt-5")
            .with_tokens(100, 20, 5);
        assert!(record.agent_id.is_some());
        assert!(record.session_id.is_some());
        assert_eq!(record.model_key.unwrap(), "gpt-5");
        assert_eq!(record.amount_micros, (100 + 20) * 5);
    }

    #[test]
    fn budget_scope_roundtrip() {
        for variant in &[BudgetScope::User, BudgetScope::Agent, BudgetScope::Project, BudgetScope::Organization] {
            let s = variant.as_str();
            let parsed = BudgetScope::parse(s).unwrap();
            assert_eq!(*variant, parsed);
        }
    }
}