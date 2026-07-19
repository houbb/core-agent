use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::domain::{ModelOperation, ModelProfile, ModelRequest, ModelRoute};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{
    ModelCatalog, ModelObservation, ModelObserver, ModelStage, ModelStream, ProviderRegistry,
    RateLimiter, RetryPolicy,
};

/// A stream plus the actual profile selected after any pre-output fallback.
pub struct StartedModelStream {
    pub profile: ModelProfile,
    pub stream: ModelStream,
}

/// Starts Provider streams under centralized timeout/retry/fallback control.
/// Once a stream is returned, mid-stream failures are never retried invisibly.
pub struct StreamEngine {
    catalog: Arc<dyn ModelCatalog>,
    providers: Arc<ProviderRegistry>,
    retry_policy: Arc<dyn RetryPolicy>,
    rate_limiter: Arc<dyn RateLimiter>,
    observers: Vec<Arc<dyn ModelObserver>>,
}

impl StreamEngine {
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

    pub async fn start(
        &self,
        request: &ModelRequest,
        route: &ModelRoute,
    ) -> ModelResult<StartedModelStream> {
        let operation = ModelOperation::Stream;
        let started = Instant::now();
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(request.config.timeout_ms);
        let mut last_error = None;
        for (candidate_index, profile) in route.candidates().enumerate() {
            if candidate_index > 0 {
                self.observe(request.id, ModelStage::Fallback, profile, 0, started, None);
            }
            let (timeout_ms, retries) = match self.provider_execution_config(request, profile).await
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
                    provider.stream(request, profile).await
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
                    Ok(stream) => {
                        self.observe(
                            request.id,
                            ModelStage::Streaming,
                            profile,
                            attempt,
                            started,
                            None,
                        );
                        return Ok(StartedModelStream {
                            profile: profile.clone(),
                            stream,
                        });
                    }
                    Err(error) => {
                        let retry = attempt < max_attempts
                            && self.retry_policy.should_retry(&error, attempt);
                        let fallback = error.is_fallback_eligible();
                        self.observe(
                            request.id,
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
            ModelError::RouteNotFound("route contained no stream-capable Provider".into())
        }))
    }

    async fn provider_execution_config(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> ModelResult<(u64, Option<u32>)> {
        let Some(provider) = self.catalog.find_provider(&profile.provider).await? else {
            return Ok((request.config.timeout_ms, request.config.max_retries));
        };
        if !provider.enabled {
            return Err(ModelError::ProviderNotFound(format!(
                "{} is disabled",
                provider.key
            )));
        }
        Ok((
            request.config.timeout_ms.min(provider.timeout_ms),
            request.config.max_retries.or(Some(provider.max_retries)),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn observe(
        &self,
        request_id: uuid::Uuid,
        stage: ModelStage,
        profile: &ModelProfile,
        attempt: u32,
        started: Instant,
        error_kind: Option<&str>,
    ) {
        let observation = ModelObservation {
            request_id,
            operation: ModelOperation::Stream,
            stage,
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            attempt,
            duration_ms: super::engine::elapsed_ms(started),
            error_kind: error_kind.map(str::to_owned),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}
