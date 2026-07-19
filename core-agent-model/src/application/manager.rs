use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;

use crate::domain::{
    EmbeddingRequest, EmbeddingResponse, ModelCapability, ModelOperation, ModelProfile,
    ModelRequest, ModelResponse, ModelRoute, ModelStreamEvent, ProviderDefinition, RoutingRequest,
    UsageRecord,
};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{
    CapabilityRegistry, DefaultCapabilityRegistry, FixedRetryPolicy, ModelCatalog,
    ModelObservation, ModelObserver, ModelProvider, ModelRouter, ModelStage, ModelStream,
    NoopRateLimiter, NoopUsageCollector, ProviderRegistry, RateLimiter, RequestInterceptor,
    ResponseInterceptor, RetryPolicy, UsageCollector,
};

use super::{DefaultModelRouter, InferenceEngine, StreamEngine};

/// Single public inference entry point used by all other Runtime layers.
pub struct ModelManager {
    catalog: Arc<dyn ModelCatalog>,
    router: Arc<dyn ModelRouter>,
    capabilities: Arc<dyn CapabilityRegistry>,
    providers: Arc<ProviderRegistry>,
    inference: InferenceEngine,
    streaming: StreamEngine,
    request_interceptors: Vec<Arc<dyn RequestInterceptor>>,
    response_interceptors: Vec<Arc<dyn ResponseInterceptor>>,
    usage_collector: Arc<dyn UsageCollector>,
    observers: Vec<Arc<dyn ModelObserver>>,
}

impl ModelManager {
    pub fn builder(catalog: Arc<dyn ModelCatalog>) -> ModelManagerBuilder {
        ModelManagerBuilder::new(catalog)
    }

    pub fn register_provider(&self, provider: Arc<dyn ModelProvider>) -> ModelResult<()> {
        self.providers.register(provider)
    }

    pub async fn upsert_provider(&self, provider: &ProviderDefinition) -> ModelResult<()> {
        self.catalog.upsert_provider(provider).await
    }

    pub async fn list_providers(&self) -> ModelResult<Vec<ProviderDefinition>> {
        self.catalog.list_providers().await
    }

    pub async fn upsert_profile(&self, profile: &ModelProfile) -> ModelResult<()> {
        self.catalog.upsert_profile(profile).await
    }

    pub async fn find_profile(&self, key: &str) -> ModelResult<Option<ModelProfile>> {
        self.catalog.find_profile(key).await
    }

    pub async fn list_profiles(&self) -> ModelResult<Vec<ModelProfile>> {
        self.catalog.list_profiles().await
    }

    pub async fn generate(&self, mut request: ModelRequest) -> ModelResult<ModelResponse> {
        self.prepare_request(&mut request, &[ModelCapability::Chat])
            .await?;
        let route = self
            .route_with_audit(
                &request.routing_request(),
                request.id,
                ModelOperation::Generate,
                request.metadata.clone(),
            )
            .await?;
        let result = self.inference.generate(&request, &route).await;
        self.finish_response(request, route, ModelOperation::Generate, result)
            .await
    }

    pub async fn vision(&self, mut request: ModelRequest) -> ModelResult<ModelResponse> {
        self.prepare_request(
            &mut request,
            &[ModelCapability::Chat, ModelCapability::Vision],
        )
        .await?;
        if !request.has_image() {
            return Err(ModelError::InvalidArgument(
                "vision request must contain at least one image after interception".into(),
            ));
        }
        let route = self
            .route_with_audit(
                &request.routing_request(),
                request.id,
                ModelOperation::Vision,
                request.metadata.clone(),
            )
            .await?;
        let result = self.inference.vision(&request, &route).await;
        self.finish_response(request, route, ModelOperation::Vision, result)
            .await
    }

    pub async fn embedding(&self, mut request: EmbeddingRequest) -> ModelResult<EmbeddingResponse> {
        request.validate()?;
        for interceptor in &self.request_interceptors {
            interceptor.intercept_embedding(&mut request).await?;
        }
        request.validate()?;
        let route = self
            .route_with_audit(
                &request.routing_request(),
                request.id,
                ModelOperation::Embedding,
                request.metadata.clone(),
            )
            .await?;
        let result = self.inference.embedding(&request, &route).await;
        match result {
            Ok(mut response) => {
                if self
                    .usage_collector
                    .record(&UsageRecord::success(
                        request.id,
                        ModelOperation::Embedding,
                        &response.provider,
                        &response.model,
                        &response.profile,
                        response.usage.clone(),
                        request.metadata.clone(),
                    ))
                    .await
                    .is_err()
                {
                    response
                        .metadata
                        .insert("core_agent.usage_collection".into(), "FAILED".into());
                    self.observe_usage_failure(
                        request.id,
                        ModelOperation::Embedding,
                        route
                            .candidates()
                            .find(|profile| profile.provider == response.provider),
                    );
                }
                Ok(response)
            }
            Err(error) => {
                if let Err(audit) = self
                    .record_failure(
                        request.id,
                        ModelOperation::Embedding,
                        Some(&route),
                        &error,
                        request.metadata,
                    )
                    .await
                {
                    let _ = audit;
                    self.observe_usage_failure(
                        request.id,
                        ModelOperation::Embedding,
                        route.candidates().find(|profile| {
                            error
                                .provider_key()
                                .is_some_and(|provider| provider == profile.provider)
                        }),
                    );
                }
                Err(error)
            }
        }
    }

    pub async fn stream(&self, mut request: ModelRequest) -> ModelResult<ModelStream> {
        let stream_started = Instant::now();
        let total_timeout_ms = request.config.timeout_ms;
        self.prepare_request(
            &mut request,
            &[ModelCapability::Chat, ModelCapability::Streaming],
        )
        .await?;
        let route = self
            .route_with_audit(
                &request.routing_request(),
                request.id,
                ModelOperation::Stream,
                request.metadata.clone(),
            )
            .await?;
        let remaining_ms = total_timeout_ms.saturating_sub(
            u64::try_from(stream_started.elapsed().as_millis()).unwrap_or(u64::MAX),
        );
        if remaining_ms == 0 {
            let error = ModelError::Timeout {
                provider: route.primary.provider.clone(),
                timeout_ms: total_timeout_ms,
            };
            if let Err(audit) = self
                .record_failure(
                    request.id,
                    ModelOperation::Stream,
                    Some(&route),
                    &error,
                    request.metadata,
                )
                .await
            {
                let _ = audit;
                self.observe_usage_failure(
                    request.id,
                    ModelOperation::Stream,
                    Some(&route.primary),
                );
            }
            return Err(error);
        }
        request.config.timeout_ms = remaining_ms;
        let started = match self.streaming.start(&request, &route).await {
            Ok(started) => started,
            Err(error) => {
                if let Err(audit) = self
                    .record_failure(
                        request.id,
                        ModelOperation::Stream,
                        Some(&route),
                        &error,
                        request.metadata.clone(),
                    )
                    .await
                {
                    let _ = audit;
                    self.observe_usage_failure(
                        request.id,
                        ModelOperation::Stream,
                        route.candidates().find(|profile| {
                            error
                                .provider_key()
                                .is_some_and(|provider| provider == profile.provider)
                        }),
                    );
                }
                return Err(error);
            }
        };

        let profile = started.profile;
        let mut source = started.stream;
        let request_id = request.id;
        let request_metadata = request.metadata;
        let interceptors = self.response_interceptors.clone();
        let usage_collector = self.usage_collector.clone();
        let observers = self.observers.clone();
        let output = async_stream::stream! {
            yield Ok(ModelStreamEvent::Started {
                request_id,
                provider: profile.provider.clone(),
                model: profile.model.clone(),
                profile: profile.key.clone(),
            });
            loop {
                let remaining = Duration::from_millis(total_timeout_ms)
                    .saturating_sub(stream_started.elapsed());
                let item = match tokio::time::timeout(remaining, source.next()).await {
                    Ok(Some(item)) => item,
                    Ok(None) => Err(ModelError::Provider {
                        provider: profile.provider.clone(),
                        message: "stream ended without a Completed event".into(),
                        status: None,
                        retryable: false,
                    }),
                    Err(_) => Err(ModelError::Timeout {
                        provider: profile.provider.clone(),
                        timeout_ms: total_timeout_ms,
                    }),
                };
                match item {
                    Ok(ModelStreamEvent::Started { .. }) => {
                        // Provider-level Started events are normalized to one Manager event.
                    }
                    Ok(ModelStreamEvent::Completed(mut response)) => {
                        let mut intercept_error = normalize_stream_response(
                            &mut response,
                            request_id,
                            &profile,
                        )
                        .err();
                        if intercept_error.is_none() {
                            for interceptor in &interceptors {
                                if let Err(error) = interceptor.intercept(&mut response).await {
                                    intercept_error = Some(error);
                                    break;
                                }
                            }
                        }
                        if let Some(error) = intercept_error {
                            let mut record = UsageRecord::failure(
                                request_id,
                                ModelOperation::Stream,
                                &profile.provider,
                                &profile.model,
                                &profile.key,
                                error.kind(),
                                request_metadata.clone(),
                            );
                            record.usage = response.usage.clone();
                            if let Err(usage_error) = usage_collector.record(&record).await {
                                let _ = usage_error;
                                observe_stream(&observers, request_id, ModelStage::UsageFailed, &profile, stream_started, Some("USAGE"));
                                yield Err(error);
                            } else {
                                observe_stream(&observers, request_id, ModelStage::Failed, &profile, stream_started, Some(error.kind()));
                                yield Err(error);
                            }
                            return;
                        }
                        let record = UsageRecord::success(
                            request_id,
                            ModelOperation::Stream,
                            &response.provider,
                            &response.model,
                            &response.profile,
                            response.usage.clone(),
                            request_metadata.clone(),
                        );
                        if usage_collector.record(&record).await.is_err() {
                            response
                                .metadata
                                .insert("core_agent.usage_collection".into(), "FAILED".into());
                            observe_stream(&observers, request_id, ModelStage::UsageFailed, &profile, stream_started, Some("USAGE"));
                        }
                        observe_stream(&observers, request_id, ModelStage::Completed, &profile, stream_started, None);
                        yield Ok(ModelStreamEvent::Completed(response));
                        return;
                    }
                    Ok(event) => yield Ok(event),
                    Err(error) => {
                        let record = UsageRecord::failure(
                            request_id,
                            ModelOperation::Stream,
                            &profile.provider,
                            &profile.model,
                            &profile.key,
                            error.kind(),
                            request_metadata.clone(),
                        );
                        if let Err(usage_error) = usage_collector.record(&record).await {
                            let _ = usage_error;
                            observe_stream(&observers, request_id, ModelStage::UsageFailed, &profile, stream_started, Some("USAGE"));
                            yield Err(error);
                        } else {
                            observe_stream(&observers, request_id, ModelStage::Failed, &profile, stream_started, Some(error.kind()));
                            yield Err(error);
                        }
                        return;
                    }
                }
            }
        };
        Ok(Box::pin(output))
    }

    async fn prepare_request(
        &self,
        request: &mut ModelRequest,
        required: &[ModelCapability],
    ) -> ModelResult<()> {
        request
            .required_capabilities
            .extend(required.iter().copied());
        request.validate()?;
        for interceptor in &self.request_interceptors {
            interceptor.intercept(request).await?;
        }
        request
            .required_capabilities
            .extend(required.iter().copied());
        request.validate()
    }

    async fn route(
        &self,
        request: &RoutingRequest,
        request_id: uuid::Uuid,
        operation: ModelOperation,
    ) -> ModelResult<ModelRoute> {
        let provider_state = self
            .catalog
            .list_providers()
            .await?
            .into_iter()
            .map(|provider| (provider.key, provider.enabled))
            .collect::<HashMap<_, _>>();
        let profiles = self
            .catalog
            .list_profiles()
            .await?
            .into_iter()
            .filter(|profile| {
                provider_state
                    .get(&profile.provider)
                    .copied()
                    .unwrap_or(true)
                    && profile.enabled
                    && profile.policy.allowed
                    && request
                        .required_capabilities
                        .iter()
                        .all(|capability| self.capabilities.supports(profile, *capability))
                    && request
                        .max_output_tokens
                        .is_none_or(|tokens| tokens <= profile.limits.max_output_tokens)
            })
            .collect::<Vec<_>>();
        let selected = self
            .router
            .select(request, &profiles, self.capabilities.as_ref())
            .await?;
        let primary = profiles
            .iter()
            .find(|profile| profile.key == selected.primary.key)
            .cloned()
            .ok_or_else(|| {
                ModelError::RouteNotFound(
                    "router returned a profile outside the eligible Catalog set".into(),
                )
            })?;
        self.validate_capabilities(&primary, &request.required_capabilities)?;
        let mut seen = std::collections::BTreeSet::from([primary.key.clone()]);
        let fallbacks = selected
            .fallbacks
            .into_iter()
            .filter_map(|selected| {
                profiles
                    .iter()
                    .find(|profile| profile.key == selected.key)
                    .cloned()
            })
            .filter(|profile| seen.insert(profile.key.clone()))
            .collect();
        let route = ModelRoute {
            primary,
            fallbacks,
            strategy: selected.strategy,
        };
        self.observe_routed(request_id, operation, &route.primary);
        Ok(route)
    }

    async fn route_with_audit(
        &self,
        request: &RoutingRequest,
        request_id: uuid::Uuid,
        operation: ModelOperation,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> ModelResult<ModelRoute> {
        match self.route(request, request_id, operation).await {
            Ok(route) => Ok(route),
            Err(error) => {
                if let Err(audit) = self
                    .record_failure(request_id, operation, None, &error, metadata)
                    .await
                {
                    let _ = audit;
                    self.observe_usage_failure(request_id, operation, None);
                }
                Err(error)
            }
        }
    }

    fn validate_capabilities(
        &self,
        profile: &ModelProfile,
        required: &std::collections::BTreeSet<ModelCapability>,
    ) -> ModelResult<()> {
        for capability in required {
            if !self.capabilities.supports(profile, *capability) {
                return Err(ModelError::UnsupportedCapability {
                    profile: profile.key.clone(),
                    capability: *capability,
                });
            }
        }
        Ok(())
    }

    async fn finish_response(
        &self,
        request: ModelRequest,
        route: ModelRoute,
        operation: ModelOperation,
        result: ModelResult<ModelResponse>,
    ) -> ModelResult<ModelResponse> {
        match result {
            Ok(mut response) => {
                for interceptor in &self.response_interceptors {
                    if let Err(error) = interceptor.intercept(&mut response).await {
                        let mut record = UsageRecord::failure(
                            request.id,
                            operation,
                            &response.provider,
                            &response.model,
                            &response.profile,
                            error.kind(),
                            request.metadata,
                        );
                        record.usage = response.usage.clone();
                        if let Err(audit) = self.usage_collector.record(&record).await {
                            let _ = audit;
                            self.observe_usage_failure(
                                request.id,
                                operation,
                                route
                                    .candidates()
                                    .find(|profile| profile.provider == response.provider),
                            );
                        }
                        return Err(error);
                    }
                }
                if self
                    .usage_collector
                    .record(&UsageRecord::success(
                        request.id,
                        operation,
                        &response.provider,
                        &response.model,
                        &response.profile,
                        response.usage.clone(),
                        request.metadata,
                    ))
                    .await
                    .is_err()
                {
                    response
                        .metadata
                        .insert("core_agent.usage_collection".into(), "FAILED".into());
                    self.observe_usage_failure(
                        request.id,
                        operation,
                        route
                            .candidates()
                            .find(|profile| profile.provider == response.provider),
                    );
                }
                Ok(response)
            }
            Err(error) => {
                if let Err(audit) = self
                    .record_failure(
                        request.id,
                        operation,
                        Some(&route),
                        &error,
                        request.metadata,
                    )
                    .await
                {
                    let _ = audit;
                    self.observe_usage_failure(
                        request.id,
                        operation,
                        route.candidates().find(|profile| {
                            error
                                .provider_key()
                                .is_some_and(|provider| provider == profile.provider)
                        }),
                    );
                }
                Err(error)
            }
        }
    }

    async fn record_failure(
        &self,
        request_id: uuid::Uuid,
        operation: ModelOperation,
        route: Option<&ModelRoute>,
        error: &ModelError,
        metadata: std::collections::BTreeMap<String, String>,
    ) -> ModelResult<()> {
        let target = route.map(|route| {
            error
                .provider_key()
                .and_then(|provider| {
                    route
                        .candidates()
                        .find(|profile| profile.provider == provider)
                })
                .unwrap_or(&route.primary)
        });
        let (provider, model, profile) = target
            .map(|profile| {
                (
                    profile.provider.as_str(),
                    profile.model.as_str(),
                    profile.key.as_str(),
                )
            })
            .unwrap_or(("", "", ""));
        self.usage_collector
            .record(&UsageRecord::failure(
                request_id,
                operation,
                provider,
                model,
                profile,
                error.kind(),
                metadata,
            ))
            .await
            .map_err(|error| ModelError::usage(error.to_string()))
    }

    fn observe_usage_failure(
        &self,
        request_id: uuid::Uuid,
        operation: ModelOperation,
        profile: Option<&ModelProfile>,
    ) {
        let observation = ModelObservation {
            request_id,
            operation,
            stage: ModelStage::UsageFailed,
            provider: profile
                .map(|profile| profile.provider.clone())
                .unwrap_or_default(),
            model: profile
                .map(|profile| profile.model.clone())
                .unwrap_or_default(),
            profile: profile
                .map(|profile| profile.key.clone())
                .unwrap_or_default(),
            attempt: 0,
            duration_ms: 0,
            error_kind: Some("USAGE".into()),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }

    fn observe_routed(
        &self,
        request_id: uuid::Uuid,
        operation: ModelOperation,
        profile: &ModelProfile,
    ) {
        let observation = ModelObservation {
            request_id,
            operation,
            stage: ModelStage::Routed,
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            attempt: 0,
            duration_ms: 0,
            error_kind: None,
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

fn observe_stream(
    observers: &[Arc<dyn ModelObserver>],
    request_id: uuid::Uuid,
    stage: ModelStage,
    profile: &ModelProfile,
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
        attempt: 0,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        error_kind: error_kind.map(str::to_owned),
    };
    for observer in observers {
        let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
    }
}

fn normalize_stream_response(
    response: &mut ModelResponse,
    request_id: uuid::Uuid,
    profile: &ModelProfile,
) -> ModelResult<()> {
    response.request_id = request_id;
    response.provider.clone_from(&profile.provider);
    response.model.clone_from(&profile.model);
    response.profile.clone_from(&profile.key);
    response.usage.normalize();
    if response.usage.cost.is_none() {
        response.usage.cost = profile.pricing.estimate(
            response.usage.prompt_tokens,
            response.usage.completion_tokens,
            response.usage.cache_tokens,
        );
    }
    response.usage.validate()
}

pub struct ModelManagerBuilder {
    catalog: Arc<dyn ModelCatalog>,
    router: Arc<dyn ModelRouter>,
    capabilities: Arc<dyn CapabilityRegistry>,
    providers: Vec<Arc<dyn ModelProvider>>,
    retry_policy: Arc<dyn RetryPolicy>,
    rate_limiter: Arc<dyn RateLimiter>,
    request_interceptors: Vec<Arc<dyn RequestInterceptor>>,
    response_interceptors: Vec<Arc<dyn ResponseInterceptor>>,
    usage_collector: Arc<dyn UsageCollector>,
    observers: Vec<Arc<dyn ModelObserver>>,
}

impl ModelManagerBuilder {
    pub fn new(catalog: Arc<dyn ModelCatalog>) -> Self {
        Self {
            catalog,
            router: Arc::new(DefaultModelRouter),
            capabilities: Arc::new(DefaultCapabilityRegistry),
            providers: Vec::new(),
            retry_policy: Arc::new(FixedRetryPolicy::default()),
            rate_limiter: Arc::new(NoopRateLimiter),
            request_interceptors: Vec::new(),
            response_interceptors: Vec::new(),
            usage_collector: Arc::new(NoopUsageCollector),
            observers: Vec::new(),
        }
    }

    pub fn with_router(mut self, router: Arc<dyn ModelRouter>) -> Self {
        self.router = router;
        self
    }

    pub fn with_capability_registry(mut self, capabilities: Arc<dyn CapabilityRegistry>) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn add_provider(mut self, provider: Arc<dyn ModelProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn with_retry_policy(mut self, policy: Arc<dyn RetryPolicy>) -> Self {
        self.retry_policy = policy;
        self
    }

    pub fn with_rate_limiter(mut self, limiter: Arc<dyn RateLimiter>) -> Self {
        self.rate_limiter = limiter;
        self
    }

    pub fn add_request_interceptor(mut self, interceptor: Arc<dyn RequestInterceptor>) -> Self {
        self.request_interceptors.push(interceptor);
        self
    }

    pub fn add_response_interceptor(mut self, interceptor: Arc<dyn ResponseInterceptor>) -> Self {
        self.response_interceptors.push(interceptor);
        self
    }

    pub fn with_usage_collector(mut self, collector: Arc<dyn UsageCollector>) -> Self {
        self.usage_collector = collector;
        self
    }

    pub fn add_observer(mut self, observer: Arc<dyn ModelObserver>) -> Self {
        self.observers.push(observer);
        self
    }

    pub fn build(self) -> ModelResult<ModelManager> {
        let registry = Arc::new(ProviderRegistry::default());
        for provider in self.providers {
            registry.register(provider)?;
        }
        let inference = InferenceEngine::new(
            self.catalog.clone(),
            registry.clone(),
            self.retry_policy.clone(),
            self.rate_limiter.clone(),
            self.observers.clone(),
        );
        let streaming = StreamEngine::new(
            self.catalog.clone(),
            registry.clone(),
            self.retry_policy,
            self.rate_limiter,
            self.observers.clone(),
        );
        Ok(ModelManager {
            catalog: self.catalog,
            router: self.router,
            capabilities: self.capabilities,
            providers: registry,
            inference,
            streaming,
            request_interceptors: self.request_interceptors,
            response_interceptors: self.response_interceptors,
            usage_collector: self.usage_collector,
            observers: self.observers,
        })
    }
}
