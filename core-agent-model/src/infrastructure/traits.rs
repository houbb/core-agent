use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::Stream;

use crate::domain::{
    EmbeddingRequest, EmbeddingResponse, ModelCapability, ModelOperation, ModelProfile,
    ModelRequest, ModelResponse, ModelRoute, ModelStreamEvent, ProviderDefinition, RoutingRequest,
    UsageRecord,
};
use crate::error::{ModelError, ModelResult};

pub type ModelStream = Pin<Box<dyn Stream<Item = ModelResult<ModelStreamEvent>> + Send + 'static>>;

/// Vendor adapter. Implementations perform one network/local inference attempt;
/// retry and fallback belong to the central Engines.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn key(&self) -> &str;

    async fn invoke(
        &self,
        request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelResponse>;

    async fn stream(
        &self,
        _request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelStream> {
        Err(ModelError::UnsupportedCapability {
            profile: target.key.clone(),
            capability: ModelCapability::Streaming,
        })
    }

    async fn embedding(
        &self,
        _request: &EmbeddingRequest,
        target: &ModelProfile,
    ) -> ModelResult<EmbeddingResponse> {
        Err(ModelError::UnsupportedCapability {
            profile: target.key.clone(),
            capability: ModelCapability::Embedding,
        })
    }

    async fn vision(
        &self,
        _request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelResponse> {
        Err(ModelError::UnsupportedCapability {
            profile: target.key.clone(),
            capability: ModelCapability::Vision,
        })
    }
}

#[async_trait]
pub trait ModelRouter: Send + Sync {
    async fn select(
        &self,
        request: &RoutingRequest,
        profiles: &[ModelProfile],
        capabilities: &dyn CapabilityRegistry,
    ) -> ModelResult<ModelRoute>;
}

#[async_trait]
pub trait ModelCatalog: Send + Sync {
    async fn upsert_provider(&self, provider: &ProviderDefinition) -> ModelResult<()>;
    async fn find_provider(&self, key: &str) -> ModelResult<Option<ProviderDefinition>>;
    async fn list_providers(&self) -> ModelResult<Vec<ProviderDefinition>>;

    async fn upsert_profile(&self, profile: &ModelProfile) -> ModelResult<()>;
    async fn find_profile(&self, key: &str) -> ModelResult<Option<ModelProfile>>;
    async fn list_profiles(&self) -> ModelResult<Vec<ModelProfile>>;
}

pub trait CapabilityRegistry: Send + Sync {
    fn supports(&self, profile: &ModelProfile, capability: ModelCapability) -> bool;

    fn validate(
        &self,
        profile: &ModelProfile,
        capabilities: impl IntoIterator<Item = ModelCapability>,
    ) -> ModelResult<()>
    where
        Self: Sized,
    {
        for capability in capabilities {
            if !self.supports(profile, capability) {
                return Err(ModelError::UnsupportedCapability {
                    profile: profile.key.clone(),
                    capability,
                });
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait RequestInterceptor: Send + Sync {
    async fn intercept(&self, request: &mut ModelRequest) -> ModelResult<()>;

    async fn intercept_embedding(&self, _request: &mut EmbeddingRequest) -> ModelResult<()> {
        Ok(())
    }
}

#[async_trait]
pub trait ResponseInterceptor: Send + Sync {
    async fn intercept(&self, response: &mut ModelResponse) -> ModelResult<()>;
}

#[async_trait]
pub trait UsageCollector: Send + Sync {
    async fn record(&self, record: &UsageRecord) -> ModelResult<()>;
}

pub trait RetryPolicy: Send + Sync {
    fn max_attempts(&self, operation: ModelOperation, requested_retries: Option<u32>) -> u32;
    fn should_retry(&self, error: &ModelError, attempt: u32) -> bool;
    fn delay(&self, attempt: u32) -> Duration;
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, provider: &str) -> ModelResult<()>;
}
