use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ModelError, ModelResult};

use super::{validate_metadata, ModelCapability};

/// A configured Provider. Secrets are deliberately absent from this structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDefinition {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub endpoint: Option<String>,
    pub enabled: bool,
    pub timeout_ms: u64,
    pub max_retries: u32,
    pub rate_limit_per_minute: Option<u32>,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProviderDefinition {
    pub fn new(key: impl Into<String>, name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            name: name.into(),
            endpoint: None,
            enabled: true,
            timeout_ms: 60_000,
            max_retries: 1,
            rate_limit_per_minute: None,
            metadata: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.key.trim().is_empty() || self.name.trim().is_empty() {
            return Err(ModelError::InvalidArgument(
                "provider key and name must not be empty".into(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(ModelError::InvalidArgument(
                "provider timeout_ms must be greater than zero".into(),
            ));
        }
        if let Some(endpoint) = &self.endpoint {
            let endpoint = reqwest::Url::parse(endpoint).map_err(|error| {
                ModelError::InvalidArgument(format!("invalid provider endpoint: {error}"))
            })?;
            if !matches!(endpoint.scheme(), "http" | "https")
                || !endpoint.username().is_empty()
                || endpoint.password().is_some()
                || endpoint.query().is_some()
                || endpoint.fragment().is_some()
            {
                return Err(ModelError::InvalidArgument(
                    "provider endpoint must be an http(s) base URL without credentials, query or fragment"
                        .into(),
                ));
            }
        }
        validate_metadata(&self.metadata, "provider metadata")?;
        Ok(())
    }
}

/// Hard model limits used before Provider invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLimits {
    pub context_tokens: u64,
    pub max_output_tokens: u64,
}

impl Default for ModelLimits {
    fn default() -> Self {
        Self {
            context_tokens: 128_000,
            max_output_tokens: 8_192,
        }
    }
}

/// Price per one million tokens. Values and currency may be unknown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelPricing {
    pub input_per_million: Option<f64>,
    pub output_per_million: Option<f64>,
    pub cache_per_million: Option<f64>,
    pub currency: Option<String>,
}

impl ModelPricing {
    pub fn estimate(
        &self,
        prompt_tokens: u64,
        completion_tokens: u64,
        cache_tokens: u64,
    ) -> Option<f64> {
        let input_price = self.input_per_million?;
        let uncached_tokens = prompt_tokens.saturating_sub(cache_tokens);
        let input = input_price * uncached_tokens as f64 / 1_000_000.0;
        let output = self.output_per_million? * completion_tokens as f64 / 1_000_000.0;
        let cache =
            self.cache_per_million.unwrap_or(input_price) * cache_tokens as f64 / 1_000_000.0;
        Some(input + output + cache)
    }

    pub fn sort_cost(&self) -> f64 {
        self.input_per_million.unwrap_or(f64::INFINITY)
            + self.output_per_million.unwrap_or(f64::INFINITY)
    }

    fn validate(&self) -> ModelResult<()> {
        for (name, value) in [
            ("input_per_million", self.input_per_million),
            ("output_per_million", self.output_per_million),
            ("cache_per_million", self.cache_per_million),
        ] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(ModelError::InvalidArgument(format!(
                    "pricing {name} must be finite and non-negative"
                )));
            }
        }
        Ok(())
    }
}

/// Operational hints used by deterministic Router strategies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelPerformance {
    pub expected_latency_ms: Option<u64>,
    pub quality_score: Option<f64>,
}

impl ModelPerformance {
    fn validate(&self) -> ModelResult<()> {
        if self.quality_score.is_some_and(|value| !value.is_finite()) {
            return Err(ModelError::InvalidArgument(
                "quality_score must be finite".into(),
            ));
        }
        Ok(())
    }
}

/// Reversible policy flags attached to a profile. Enterprise policy engines can
/// replace the Router later without changing Provider contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelPolicy {
    pub allowed: bool,
    pub allow_workspace: bool,
    pub allow_network: bool,
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self {
            allowed: true,
            allow_workspace: true,
            allow_network: false,
        }
    }
}

/// Stable model abstraction used by callers instead of a vendor model name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: Uuid,
    pub key: String,
    pub provider: String,
    pub model: String,
    pub capabilities: BTreeSet<ModelCapability>,
    pub limits: ModelLimits,
    pub pricing: ModelPricing,
    pub performance: ModelPerformance,
    pub policy: ModelPolicy,
    pub metadata: BTreeMap<String, String>,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ModelProfile {
    pub fn new(
        key: impl Into<String>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            provider: provider.into(),
            model: model.into(),
            capabilities: BTreeSet::new(),
            limits: ModelLimits::default(),
            pricing: ModelPricing::default(),
            performance: ModelPerformance::default(),
            policy: ModelPolicy::default(),
            metadata: BTreeMap::new(),
            priority: 0,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_capability(mut self, capability: ModelCapability) -> Self {
        self.capabilities.insert(capability);
        self
    }

    pub fn supports(&self, capability: ModelCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.key.trim().is_empty()
            || self.provider.trim().is_empty()
            || self.model.trim().is_empty()
        {
            return Err(ModelError::InvalidArgument(
                "profile key, provider and model must not be empty".into(),
            ));
        }
        if self.limits.context_tokens == 0 || self.limits.max_output_tokens == 0 {
            return Err(ModelError::InvalidArgument(
                "model token limits must be greater than zero".into(),
            ));
        }
        self.pricing.validate()?;
        self.performance.validate()?;
        validate_metadata(&self.metadata, "profile metadata")?;
        Ok(())
    }
}

/// Router selection strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingStrategy {
    #[default]
    Auto,
    Manual,
    LowestCost,
    LowestLatency,
}

/// Provider-neutral fields used by every operation when selecting a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingRequest {
    pub profile: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub required_capabilities: BTreeSet<ModelCapability>,
    pub max_output_tokens: Option<u64>,
    pub strategy: RoutingStrategy,
}

/// Ordered output from a Router. Fallbacks are attempted only before output starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub primary: ModelProfile,
    pub fallbacks: Vec<ModelProfile>,
    pub strategy: RoutingStrategy,
}

impl ModelRoute {
    pub fn candidates(&self) -> impl Iterator<Item = &ModelProfile> {
        std::iter::once(&self.primary).chain(self.fallbacks.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_supports_declared_capability() {
        let profile =
            ModelProfile::new("coding", "openai", "gpt").with_capability(ModelCapability::Chat);
        assert!(profile.supports(ModelCapability::Chat));
        assert!(!profile.supports(ModelCapability::Vision));
    }

    #[test]
    fn pricing_estimates_known_cost() {
        let pricing = ModelPricing {
            input_per_million: Some(2.0),
            output_per_million: Some(4.0),
            cache_per_million: Some(1.0),
            currency: Some("USD".into()),
        };
        assert_eq!(pricing.estimate(1_000_000, 500_000, 100_000), Some(3.9));
    }

    #[test]
    fn pricing_rejects_nan() {
        let mut profile = ModelProfile::new("bad", "p", "m");
        profile.pricing.input_per_million = Some(f64::NAN);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn provider_metadata_rejects_secrets() {
        let mut provider = ProviderDefinition::new("p", "Provider");
        provider
            .metadata
            .insert("api_token".into(), "secret".into());
        assert!(provider.validate().is_err());
    }

    #[test]
    fn provider_endpoint_rejects_query_parameters() {
        let mut provider = ProviderDefinition::new("p", "Provider");
        provider.endpoint = Some("https://example.com/v1?apikey=secret".into());
        assert!(provider.validate().is_err());
    }
}
