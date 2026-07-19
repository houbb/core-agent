use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::{
    EmbeddingRequest, EmbeddingResponse, ModelOperation, ModelProfile, ModelRequest, ModelResponse,
    ModelRoute,
};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{
    ModelCatalog, ModelObservation, ModelObserver, ModelStage, ProviderRegistry, RateLimiter,
    RetryPolicy,
};

/// Owns non-stream inference, timeout, retry and pre-output fallback.
pub struct InferenceEngine {
    catalog: Arc<dyn ModelCatalog>,
    providers: Arc<ProviderRegistry>,
    retry_policy: Arc<dyn RetryPolicy>,
    rate_limiter: Arc<dyn RateLimiter>,
    observers: Vec<Arc<dyn ModelObserver>>,
}

impl InferenceEngine {
    pub fn new(
        catalog: Arc<dyn ModelCatalog>,
        providers: Arc<ProviderRegistry>,
        retry_policy: Arc<dyn RetryPolicy>,
        rate_limiter: Arc<dyn RateLimiter>,
        observers: Vec<Arc<dyn ModelObserver>>,
    ) -> Self {
        Self {
            catalog,
            providers,
            retry_policy,
            rate_limiter,
            observers,
        }
    }

    pub async fn generate(
        &self,
        request: &ModelRequest,
        route: &ModelRoute,
    ) -> ModelResult<ModelResponse> {
        self.execute_model(request, route, ModelOperation::Generate, false)
            .await
    }

    pub async fn vision(
        &self,
        request: &ModelRequest,
        route: &ModelRoute,
    ) -> ModelResult<ModelResponse> {
        self.execute_model(request, route, ModelOperation::Vision, true)
            .await
    }

    async fn execute_model(
        &self,
        request: &ModelRequest,
        route: &ModelRoute,
        operation: ModelOperation,
        vision: bool,
    ) -> ModelResult<ModelResponse> {
        let started = Instant::now();
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(request.config.timeout_ms);
        let mut last_error = None;
        for (candidate_index, profile) in route.candidates().enumerate() {
            if candidate_index > 0 {
                self.observe(
                    request.id,
                    operation,
                    ModelStage::Fallback,
                    profile,
                    0,
                    started,
                    None,
                );
            }
            let (timeout_ms, retries) = match self
                .provider_execution_config(
                    profile,
                    request.config.timeout_ms,
                    request.config.max_retries,
                )
                .await
            {
                Ok(config) => config,
                Err(error) if error.is_fallback_eligible() => {
                    last_error = Some(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let max_attempts = self.retry_policy.max_attempts(operation, retries).max(1);

            for attempt in 1..=max_attempts {
                self.observe(
                    request.id,
                    operation,
                    ModelStage::AttemptStarted,
                    profile,
                    attempt,
                    started,
                    None,
                );
                let provider = match self.providers.get(&profile.provider) {
                    Ok(provider) => provider,
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                };
                let future = async {
                    self.rate_limiter.acquire(&profile.provider).await?;
                    if vision {
                        provider.vision(request, profile).await
                    } else {
                        provider.invoke(request, profile).await
                    }
                };
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                let result =
                    tokio::time::timeout(Duration::from_millis(timeout_ms).min(remaining), future)
                        .await
                        .map_err(|_| ModelError::Timeout {
                            provider: profile.provider.clone(),
                            timeout_ms: request.config.timeout_ms,
                        })
                        .and_then(|result| result);

                match result {
                    Ok(mut response) => {
                        normalize_response(&mut response, request, profile, started)?;
                        self.observe(
                            request.id,
                            operation,
                            ModelStage::Completed,
                            profile,
                            attempt,
                            started,
                            None,
                        );
                        return Ok(response);
                    }
                    Err(error) => {
                        let retry = attempt < max_attempts
                            && self.retry_policy.should_retry(&error, attempt);
                        let fallback = error.is_fallback_eligible();
                        self.observe(
                            request.id,
                            operation,
                            if retry {
                                ModelStage::RetryScheduled
                            } else {
                                ModelStage::Failed
                            },
                            profile,
                            attempt,
                            started,
                            Some(error.kind()),
                        );
                        if retry {
                            let delay = self.retry_policy.delay(attempt);
                            let remaining =
                                deadline.saturating_duration_since(tokio::time::Instant::now());
                            if delay >= remaining {
                                return Err(ModelError::Timeout {
                                    provider: profile.provider.clone(),
                                    timeout_ms: request.config.timeout_ms,
                                });
                            }
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        last_error = Some(error);
                        if !fallback {
                            return Err(last_error.expect("error was just assigned"));
                        }
                        break;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ModelError::RouteNotFound("route contained no executable profiles".into())
        }))
    }

    pub async fn embedding(
        &self,
        request: &EmbeddingRequest,
        route: &ModelRoute,
    ) -> ModelResult<EmbeddingResponse> {
        let operation = ModelOperation::Embedding;
        let started = Instant::now();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(request.timeout_ms);
        let mut last_error = None;
        for (candidate_index, profile) in route.candidates().enumerate() {
            if candidate_index > 0 {
                self.observe(
                    request.id,
                    operation,
                    ModelStage::Fallback,
                    profile,
                    0,
                    started,
                    None,
                );
            }
            let (timeout_ms, retries) = match self
                .provider_execution_config(profile, request.timeout_ms, request.max_retries)
                .await
            {
                Ok(config) => config,
                Err(error) if error.is_fallback_eligible() => {
                    last_error = Some(error);
                    continue;
                }
                Err(error) => return Err(error),
            };
            let max_attempts = self.retry_policy.max_attempts(operation, retries).max(1);
            for attempt in 1..=max_attempts {
                self.observe(
                    request.id,
                    operation,
                    ModelStage::AttemptStarted,
                    profile,
                    attempt,
                    started,
                    None,
                );
                let provider = match self.providers.get(&profile.provider) {
                    Ok(provider) => provider,
                    Err(error) => {
                        last_error = Some(error);
                        break;
                    }
                };
                let future = async {
                    self.rate_limiter.acquire(&profile.provider).await?;
                    provider.embedding(request, profile).await
                };
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                let result =
                    tokio::time::timeout(Duration::from_millis(timeout_ms).min(remaining), future)
                        .await
                        .map_err(|_| ModelError::Timeout {
                            provider: profile.provider.clone(),
                            timeout_ms: request.timeout_ms,
                        })
                        .and_then(|result| result);
                match result {
                    Ok(mut response) => {
                        normalize_embedding(&mut response, request, profile, started)?;
                        self.observe(
                            request.id,
                            operation,
                            ModelStage::Completed,
                            profile,
                            attempt,
                            started,
                            None,
                        );
                        return Ok(response);
                    }
                    Err(error) => {
                        let retry = attempt < max_attempts
                            && self.retry_policy.should_retry(&error, attempt);
                        let fallback = error.is_fallback_eligible();
                        self.observe(
                            request.id,
                            operation,
                            if retry {
                                ModelStage::RetryScheduled
                            } else {
                                ModelStage::Failed
                            },
                            profile,
                            attempt,
                            started,
                            Some(error.kind()),
                        );
                        if retry {
                            let delay = self.retry_policy.delay(attempt);
                            let remaining =
                                deadline.saturating_duration_since(tokio::time::Instant::now());
                            if delay >= remaining {
                                return Err(ModelError::Timeout {
                                    provider: profile.provider.clone(),
                                    timeout_ms: request.timeout_ms,
                                });
                            }
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        last_error = Some(error);
                        if !fallback {
                            return Err(last_error.expect("error was just assigned"));
                        }
                        break;
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            ModelError::RouteNotFound("route contained no executable profiles".into())
        }))
    }

    async fn provider_execution_config(
        &self,
        profile: &ModelProfile,
        requested_timeout_ms: u64,
        requested_retries: Option<u32>,
    ) -> ModelResult<(u64, Option<u32>)> {
        let Some(provider) = self.catalog.find_provider(&profile.provider).await? else {
            return Ok((requested_timeout_ms, requested_retries));
        };
        if !provider.enabled {
            return Err(ModelError::ProviderNotFound(format!(
                "{} is disabled",
                provider.key
            )));
        }
        Ok((
            requested_timeout_ms.min(provider.timeout_ms),
            requested_retries.or(Some(provider.max_retries)),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn observe(
        &self,
        request_id: uuid::Uuid,
        operation: ModelOperation,
        stage: ModelStage,
        profile: &ModelProfile,
        attempt: u32,
        started: Instant,
        error_kind: Option<&str>,
    ) {
        let observation = ModelObservation {
            request_id,
            operation,
            stage,
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            attempt,
            duration_ms: elapsed_ms(started),
            error_kind: error_kind.map(str::to_owned),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

fn normalize_response(
    response: &mut ModelResponse,
    request: &ModelRequest,
    profile: &ModelProfile,
    started: Instant,
) -> ModelResult<()> {
    response.request_id = request.id;
    response.provider.clone_from(&profile.provider);
    response.model.clone_from(&profile.model);
    response.profile.clone_from(&profile.key);
    response.usage.normalize();
    if response.usage.latency_ms == 0 {
        response.usage.latency_ms = elapsed_ms(started);
    }
    if response.usage.cost.is_none() {
        response.usage.cost = profile.pricing.estimate(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.cache_tokens,
        );
    }
    response.usage.validate()
}

fn normalize_embedding(
    response: &mut EmbeddingResponse,
    request: &EmbeddingRequest,
    profile: &ModelProfile,
    started: Instant,
) -> ModelResult<()> {
    if response.embeddings.len() != request.inputs.len()
        || response.embeddings.is_empty()
        || response.embeddings[0].is_empty()
    {
        return Err(ModelError::Serialization(
            "embedding response count or dimensions do not match the request".into(),
        ));
    }
    let dimensions = response.embeddings[0].len();
    if response.embeddings.iter().any(|embedding| {
        embedding.len() != dimensions || embedding.iter().any(|value| !value.is_finite())
    }) {
        return Err(ModelError::Serialization(
            "embedding vectors must have equal, finite dimensions".into(),
        ));
    }
    response.request_id = request.id;
    response.provider.clone_from(&profile.provider);
    response.model.clone_from(&profile.model);
    response.profile.clone_from(&profile.key);
    response.dimensions = response.embeddings.first().map(Vec::len).unwrap_or(0);
    response.usage.normalize();
    if response.usage.latency_ms == 0 {
        response.usage.latency_ms = elapsed_ms(started);
    }
    if response.usage.cost.is_none() {
        response.usage.cost = profile.pricing.estimate(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.cache_tokens,
        );
    }
    response.usage.validate()
}

pub(crate) fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
