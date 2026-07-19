use std::cmp::Ordering;

use async_trait::async_trait;

use crate::domain::{ModelProfile, ModelRoute, RoutingRequest, RoutingStrategy};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{CapabilityRegistry, ModelRouter};

/// Deterministic, explainable default Router.
#[derive(Default)]
pub struct DefaultModelRouter;

#[async_trait]
impl ModelRouter for DefaultModelRouter {
    async fn select(
        &self,
        request: &RoutingRequest,
        profiles: &[ModelProfile],
        capabilities: &dyn CapabilityRegistry,
    ) -> ModelResult<ModelRoute> {
        if let Some(profile_key) = request.profile.as_deref() {
            let profile = profiles
                .iter()
                .find(|profile| profile.key == profile_key)
                .cloned()
                .ok_or_else(|| {
                    ModelError::RouteNotFound(format!("profile {profile_key} does not exist"))
                })?;
            validate_candidate(&profile, request, capabilities)?;
            return Ok(ModelRoute {
                primary: profile,
                fallbacks: Vec::new(),
                strategy: request.strategy,
            });
        }

        if request.strategy == RoutingStrategy::Manual
            && request.provider.is_none()
            && request.model.is_none()
        {
            return Err(ModelError::InvalidArgument(
                "manual routing requires profile, provider or model".into(),
            ));
        }

        let mut candidates = profiles
            .iter()
            .filter(|profile| candidate_matches(profile, request, capabilities))
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(ModelError::RouteNotFound(
                "no enabled, allowed profile satisfies all hints and capabilities".into(),
            ));
        }

        if request.strategy == RoutingStrategy::LowestCost {
            validate_comparable_currency(&candidates)?;
        }
        candidates.sort_by(|left, right| compare_profiles(left, right, request.strategy));
        let primary = candidates.remove(0);
        Ok(ModelRoute {
            primary,
            fallbacks: candidates,
            strategy: request.strategy,
        })
    }
}

fn candidate_matches(
    profile: &ModelProfile,
    request: &RoutingRequest,
    capabilities: &dyn CapabilityRegistry,
) -> bool {
    profile.enabled
        && profile.policy.allowed
        && request
            .provider
            .as_ref()
            .is_none_or(|provider| provider == &profile.provider)
        && request
            .model
            .as_ref()
            .is_none_or(|model| model == &profile.model)
        && request
            .required_capabilities
            .iter()
            .all(|capability| capabilities.supports(profile, *capability))
        && request
            .max_output_tokens
            .is_none_or(|tokens| tokens <= profile.limits.max_output_tokens)
}

fn validate_candidate(
    profile: &ModelProfile,
    request: &RoutingRequest,
    capabilities: &dyn CapabilityRegistry,
) -> ModelResult<()> {
    if !profile.enabled || !profile.policy.allowed {
        return Err(ModelError::RouteNotFound(format!(
            "profile {} is disabled or denied by policy",
            profile.key
        )));
    }
    if request
        .provider
        .as_ref()
        .is_some_and(|provider| provider != &profile.provider)
        || request
            .model
            .as_ref()
            .is_some_and(|model| model != &profile.model)
    {
        return Err(ModelError::InvalidArgument(format!(
            "profile {} conflicts with provider/model hint",
            profile.key
        )));
    }
    if let Some(capability) = request
        .required_capabilities
        .iter()
        .find(|capability| !capabilities.supports(profile, **capability))
    {
        return Err(ModelError::UnsupportedCapability {
            profile: profile.key.clone(),
            capability: *capability,
        });
    }
    if request
        .max_output_tokens
        .is_some_and(|tokens| tokens > profile.limits.max_output_tokens)
    {
        return Err(ModelError::InvalidArgument(format!(
            "profile {} max output is {} tokens",
            profile.key, profile.limits.max_output_tokens
        )));
    }
    Ok(())
}

fn validate_comparable_currency(profiles: &[ModelProfile]) -> ModelResult<()> {
    let mut currencies = profiles
        .iter()
        .filter(|profile| {
            profile.pricing.input_per_million.is_some()
                || profile.pricing.output_per_million.is_some()
        })
        .map(|profile| {
            profile
                .pricing
                .currency
                .as_deref()
                .unwrap_or("UNSPECIFIED")
                .to_ascii_uppercase()
        })
        .collect::<Vec<_>>();
    currencies.sort();
    currencies.dedup();
    if currencies.len() > 1 {
        return Err(ModelError::InvalidArgument(format!(
            "lowest-cost routing cannot compare currencies: {}",
            currencies.join(", ")
        )));
    }
    Ok(())
}

fn compare_profiles(
    left: &ModelProfile,
    right: &ModelProfile,
    strategy: RoutingStrategy,
) -> Ordering {
    let specific = match strategy {
        RoutingStrategy::LowestCost => left
            .pricing
            .sort_cost()
            .partial_cmp(&right.pricing.sort_cost())
            .unwrap_or(Ordering::Equal),
        RoutingStrategy::LowestLatency => left
            .performance
            .expected_latency_ms
            .unwrap_or(u64::MAX)
            .cmp(&right.performance.expected_latency_ms.unwrap_or(u64::MAX)),
        RoutingStrategy::Auto | RoutingStrategy::Manual => Ordering::Equal,
    };
    specific
        .then_with(|| right.priority.cmp(&left.priority))
        .then_with(|| {
            left.performance
                .expected_latency_ms
                .unwrap_or(u64::MAX)
                .cmp(&right.performance.expected_latency_ms.unwrap_or(u64::MAX))
        })
        .then_with(|| left.key.cmp(&right.key))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::domain::{ModelCapability, ModelPricing};
    use crate::infrastructure::DefaultCapabilityRegistry;

    fn request(strategy: RoutingStrategy) -> RoutingRequest {
        RoutingRequest {
            profile: None,
            provider: None,
            model: None,
            required_capabilities: BTreeSet::from([ModelCapability::Chat]),
            max_output_tokens: None,
            strategy,
        }
    }

    fn profile(key: &str, priority: i32, cost: f64) -> ModelProfile {
        let mut profile =
            ModelProfile::new(key, key, "model").with_capability(ModelCapability::Chat);
        profile.priority = priority;
        profile.pricing = ModelPricing {
            input_per_million: Some(cost),
            output_per_million: Some(cost),
            ..Default::default()
        };
        profile
    }

    #[tokio::test]
    async fn auto_prefers_priority() {
        let profiles = vec![profile("slow", 1, 1.0), profile("fast", 10, 5.0)];
        let route = DefaultModelRouter
            .select(
                &request(RoutingStrategy::Auto),
                &profiles,
                &DefaultCapabilityRegistry,
            )
            .await
            .unwrap();
        assert_eq!(route.primary.key, "fast");
        assert_eq!(route.fallbacks[0].key, "slow");
    }

    #[tokio::test]
    async fn cost_strategy_prefers_lowest_cost() {
        let profiles = vec![profile("premium", 10, 5.0), profile("cheap", 1, 1.0)];
        let route = DefaultModelRouter
            .select(
                &request(RoutingStrategy::LowestCost),
                &profiles,
                &DefaultCapabilityRegistry,
            )
            .await
            .unwrap();
        assert_eq!(route.primary.key, "cheap");
    }

    #[tokio::test]
    async fn explicit_profile_rejects_conflicting_hint() {
        let profiles = vec![profile("coding", 1, 1.0)];
        let mut request = request(RoutingStrategy::Manual);
        request.profile = Some("coding".into());
        request.provider = Some("other".into());
        assert!(matches!(
            DefaultModelRouter
                .select(&request, &profiles, &DefaultCapabilityRegistry)
                .await,
            Err(ModelError::InvalidArgument(_))
        ));
    }

    #[tokio::test]
    async fn missing_capability_does_not_route() {
        let profiles = vec![profile("text", 1, 1.0)];
        let mut request = request(RoutingStrategy::Auto);
        request
            .required_capabilities
            .insert(ModelCapability::Vision);
        assert!(matches!(
            DefaultModelRouter
                .select(&request, &profiles, &DefaultCapabilityRegistry)
                .await,
            Err(ModelError::RouteNotFound(_))
        ));
    }

    #[tokio::test]
    async fn requested_output_limit_filters_profiles() {
        let mut small = profile("small", 10, 1.0);
        small.limits.max_output_tokens = 100;
        let mut large = profile("large", 1, 2.0);
        large.limits.max_output_tokens = 1_000;
        let mut request = request(RoutingStrategy::Auto);
        request.max_output_tokens = Some(500);
        let route = DefaultModelRouter
            .select(&request, &[small, large], &DefaultCapabilityRegistry)
            .await
            .unwrap();
        assert_eq!(route.primary.key, "large");
    }

    #[tokio::test]
    async fn lowest_cost_rejects_mixed_currencies() {
        let mut usd = profile("usd", 1, 1.0);
        usd.pricing.currency = Some("USD".into());
        let mut eur = profile("eur", 1, 1.0);
        eur.pricing.currency = Some("EUR".into());
        assert!(matches!(
            DefaultModelRouter
                .select(
                    &request(RoutingStrategy::LowestCost),
                    &[usd, eur],
                    &DefaultCapabilityRegistry,
                )
                .await,
            Err(ModelError::InvalidArgument(_))
        ));
    }
}
