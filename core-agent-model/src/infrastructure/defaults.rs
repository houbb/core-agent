use std::time::Duration;

use async_trait::async_trait;

use crate::domain::{ModelCapability, ModelOperation, ModelProfile, UsageRecord};
use crate::error::{ModelError, ModelResult};

use super::{CapabilityRegistry, RateLimiter, RetryPolicy, UsageCollector};

#[derive(Default)]
pub struct DefaultCapabilityRegistry;

impl CapabilityRegistry for DefaultCapabilityRegistry {
    fn supports(&self, profile: &ModelProfile, capability: ModelCapability) -> bool {
        profile.supports(capability)
    }
}

/// Small, deterministic exponential retry policy. Attempts include the first call.
#[derive(Debug, Clone)]
pub struct FixedRetryPolicy {
    pub default_retries: u32,
    pub max_retries: u32,
    pub base_delay_ms: u64,
}

impl Default for FixedRetryPolicy {
    fn default() -> Self {
        Self {
            default_retries: 1,
            max_retries: 3,
            base_delay_ms: 50,
        }
    }
}

impl RetryPolicy for FixedRetryPolicy {
    fn max_attempts(&self, _operation: ModelOperation, requested_retries: Option<u32>) -> u32 {
        requested_retries
            .unwrap_or(self.default_retries)
            .min(self.max_retries)
            .saturating_add(1)
    }

    fn should_retry(&self, error: &ModelError, _attempt: u32) -> bool {
        error.is_retryable()
    }

    fn delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(16);
        Duration::from_millis(self.base_delay_ms.saturating_mul(1_u64 << exponent))
    }
}

#[derive(Default)]
pub struct NoopRateLimiter;

#[async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn acquire(&self, _provider: &str) -> ModelResult<()> {
        Ok(())
    }
}

/// Explicit opt-out from durable usage collection.
#[derive(Default)]
pub struct NoopUsageCollector;

#[async_trait]
impl UsageCollector for NoopUsageCollector {
    async fn record(&self, _record: &UsageRecord) -> ModelResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_attempts_are_bounded() {
        let policy = FixedRetryPolicy::default();
        assert_eq!(policy.max_attempts(ModelOperation::Generate, Some(99)), 4);
    }

    #[test]
    fn delay_is_exponential() {
        let policy = FixedRetryPolicy::default();
        assert_eq!(policy.delay(1), Duration::from_millis(50));
        assert_eq!(policy.delay(3), Duration::from_millis(200));
    }
}
