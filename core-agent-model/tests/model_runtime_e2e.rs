use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use core_agent_model::{
    ContentPart, EmbeddingRequest, EmbeddingResponse, FinishReason, ModelCapability, ModelCatalog,
    ModelError, ModelManager, ModelMessage, ModelObservation, ModelObserver, ModelProfile,
    ModelProvider, ModelRequest, ModelResponse, ModelRole, ModelStage, ModelStream,
    ModelStreamEvent, ModelUsage, OpenAiCompatibleProvider, ProviderDefinition, RateLimiter,
    RequestInterceptor, ResponseInterceptor, SqliteModelStore, UsageCollector, UsageRecord,
};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct FakeProvider {
    key: String,
    fail_until: usize,
    delay_ms: u64,
    calls: AtomicUsize,
}

impl FakeProvider {
    fn new(key: &str) -> Self {
        Self {
            key: key.into(),
            fail_until: 0,
            delay_ms: 0,
            calls: AtomicUsize::new(0),
        }
    }

    fn failing(key: &str, fail_until: usize) -> Self {
        Self {
            key: key.into(),
            fail_until,
            delay_ms: 0,
            calls: AtomicUsize::new(0),
        }
    }

    fn slow(key: &str, delay_ms: u64) -> Self {
        Self {
            key: key.into(),
            fail_until: 0,
            delay_ms,
            calls: AtomicUsize::new(0),
        }
    }

    fn response(&self, request: &ModelRequest, profile: &ModelProfile) -> ModelResponse {
        ModelResponse {
            request_id: request.id,
            provider: self.key.clone(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            content: vec![ContentPart::text(format!("{}:ok", self.key))],
            tool_calls: Vec::new(),
            usage: ModelUsage {
                prompt_tokens: 4,
                completion_tokens: 2,
                total_tokens: 6,
                ..Default::default()
            },
            finish_reason: FinishReason::Stop,
            metadata: request.metadata.clone(),
            raw_response: None,
        }
    }
}

#[async_trait]
impl ModelProvider for FakeProvider {
    fn key(&self) -> &str {
        &self.key
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        }
        if attempt <= self.fail_until {
            return Err(ModelError::Provider {
                provider: self.key.clone(),
                message: "temporary failure".into(),
                status: Some(503),
                retryable: true,
            });
        }
        Ok(self.response(request, profile))
    }

    async fn stream(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelStream, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let response = self.response(request, profile);
        Ok(Box::pin(futures_util::stream::iter(vec![
            Ok(ModelStreamEvent::Delta {
                content: "fake".into(),
            }),
            Ok(ModelStreamEvent::Usage(response.usage.clone())),
            Ok(ModelStreamEvent::Completed(response)),
        ])))
    }

    async fn embedding(
        &self,
        request: &EmbeddingRequest,
        profile: &ModelProfile,
    ) -> Result<EmbeddingResponse, ModelError> {
        Ok(EmbeddingResponse {
            request_id: request.id,
            provider: self.key.clone(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            embeddings: request.inputs.iter().map(|_| vec![0.1, 0.2, 0.3]).collect(),
            dimensions: 3,
            usage: ModelUsage {
                prompt_tokens: 3,
                total_tokens: 3,
                ..Default::default()
            },
            metadata: request.metadata.clone(),
            raw_response: None,
        })
    }

    async fn vision(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.response(request, profile))
    }
}

struct StartFailProvider {
    key: String,
    starts: AtomicUsize,
}

#[async_trait]
impl ModelProvider for StartFailProvider {
    fn key(&self) -> &str {
        &self.key
    }

    async fn invoke(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        unreachable!()
    }

    async fn stream(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelStream, ModelError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Err(ModelError::Provider {
            provider: self.key.clone(),
            message: "stream start failed".into(),
            status: Some(503),
            retryable: true,
        })
    }
}

struct MidStreamFailProvider {
    key: String,
}

#[async_trait]
impl ModelProvider for MidStreamFailProvider {
    fn key(&self) -> &str {
        &self.key
    }

    async fn invoke(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        unreachable!()
    }

    async fn stream(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelStream, ModelError> {
        Ok(Box::pin(futures_util::stream::iter(vec![
            Ok(ModelStreamEvent::Delta {
                content: "partial".into(),
            }),
            Err(ModelError::Provider {
                provider: self.key.clone(),
                message: "connection lost".into(),
                status: None,
                retryable: true,
            }),
        ])))
    }
}

struct HangingStreamProvider;

#[async_trait]
impl ModelProvider for HangingStreamProvider {
    fn key(&self) -> &str {
        "hanging"
    }

    async fn invoke(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        unreachable!()
    }

    async fn stream(
        &self,
        _request: &ModelRequest,
        _profile: &ModelProfile,
    ) -> Result<ModelStream, ModelError> {
        Ok(Box::pin(futures_util::stream::pending()))
    }
}

#[derive(Default)]
struct PrimaryRateLimiter;

#[async_trait]
impl RateLimiter for PrimaryRateLimiter {
    async fn acquire(&self, provider: &str) -> Result<(), ModelError> {
        if provider == "primary" {
            Err(ModelError::RateLimited {
                provider: provider.into(),
                message: "test quota".into(),
            })
        } else {
            Ok(())
        }
    }
}

struct ClearingRequestInterceptor;

#[async_trait]
impl RequestInterceptor for ClearingRequestInterceptor {
    async fn intercept(&self, request: &mut ModelRequest) -> Result<(), ModelError> {
        request.required_capabilities.clear();
        Ok(())
    }
}

struct RemovingImageInterceptor;

#[async_trait]
impl RequestInterceptor for RemovingImageInterceptor {
    async fn intercept(&self, request: &mut ModelRequest) -> Result<(), ModelError> {
        for message in &mut request.messages {
            message
                .content
                .retain(|part| matches!(part, ContentPart::Text { .. }));
            if message.content.is_empty() {
                message.content.push(ContentPart::text("image removed"));
            }
        }
        Ok(())
    }
}

struct RejectingResponseInterceptor;

#[async_trait]
impl ResponseInterceptor for RejectingResponseInterceptor {
    async fn intercept(&self, _response: &mut ModelResponse) -> Result<(), ModelError> {
        Err(ModelError::Interceptor("response rejected".into()))
    }
}

struct FailingUsageCollector;

#[async_trait]
impl UsageCollector for FailingUsageCollector {
    async fn record(&self, _record: &UsageRecord) -> Result<(), ModelError> {
        Err(ModelError::Persistence("usage database unavailable".into()))
    }
}

fn profile(key: &str, provider: &str, priority: i32) -> ModelProfile {
    let mut profile = ModelProfile::new(key, provider, format!("{provider}-model"));
    profile.priority = priority;
    profile.capabilities = BTreeSet::from([
        ModelCapability::Chat,
        ModelCapability::Streaming,
        ModelCapability::Embedding,
        ModelCapability::Vision,
    ]);
    profile
}

async fn manager_with(
    providers: Vec<Arc<dyn ModelProvider>>,
    profiles: Vec<ModelProfile>,
) -> (ModelManager, Arc<SqliteModelStore>) {
    let store = Arc::new(SqliteModelStore::new(":memory:").unwrap());
    for provider in &providers {
        let mut definition = ProviderDefinition::new(provider.key(), provider.key());
        definition.max_retries = 0;
        store.upsert_provider(&definition).await.unwrap();
    }
    for profile in profiles {
        store.upsert_profile(&profile).await.unwrap();
    }
    let mut builder = ModelManager::builder(store.clone()).with_usage_collector(store.clone());
    for provider in providers {
        builder = builder.add_provider(provider);
    }
    (builder.build().unwrap(), store)
}

fn chat_request() -> ModelRequest {
    ModelRequest::new(vec![ModelMessage::text(ModelRole::User, "hello")])
}

#[tokio::test]
async fn manager_executes_all_four_operations_and_persists_usage() {
    let provider = Arc::new(FakeProvider::new("fake"));
    let (manager, store) = manager_with(vec![provider], vec![profile("general", "fake", 10)]).await;

    let generated = manager.generate(chat_request()).await.unwrap();
    assert_eq!(generated.text(), "fake:ok");

    let vision = manager
        .vision(ModelRequest::new(vec![core_agent_model::ModelMessage {
            role: ModelRole::User,
            content: vec![ContentPart::ImageUrl {
                url: "data:image/png;base64,abc".into(),
                detail: None,
            }],
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }]))
        .await
        .unwrap();
    assert_eq!(vision.provider, "fake");

    let embedding = manager
        .embedding(EmbeddingRequest::new(vec!["one".into(), "two".into()]))
        .await
        .unwrap();
    assert_eq!(embedding.embeddings.len(), 2);
    assert_eq!(embedding.dimensions, 3);

    let mut stream = manager.stream(chat_request()).await.unwrap();
    let mut stages = Vec::new();
    while let Some(event) = stream.next().await {
        stages.push(event.unwrap());
    }
    assert!(matches!(
        stages.first(),
        Some(ModelStreamEvent::Started { .. })
    ));
    assert!(matches!(
        stages.last(),
        Some(ModelStreamEvent::Completed(_))
    ));
    assert_eq!(store.usage_count().await.unwrap(), 4);
    assert!(store
        .list_usage(0, 10)
        .await
        .unwrap()
        .iter()
        .all(|record| record.success));
}

#[tokio::test]
async fn central_engine_retries_then_falls_back_before_output() {
    let primary = Arc::new(FakeProvider::failing("primary", usize::MAX));
    let backup = Arc::new(FakeProvider::new("backup"));
    let (manager, _) = manager_with(
        vec![primary.clone(), backup.clone()],
        vec![
            profile("primary", "primary", 10),
            profile("backup", "backup", 1),
        ],
    )
    .await;

    let mut request = chat_request();
    request.config.max_retries = Some(1);
    let response = manager.generate(request).await.unwrap();

    assert_eq!(response.provider, "backup");
    assert_eq!(primary.calls.load(Ordering::SeqCst), 2);
    assert_eq!(backup.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn limiter_and_stream_start_failures_follow_fallback_contract() {
    let primary = Arc::new(StartFailProvider {
        key: "primary".into(),
        starts: AtomicUsize::new(0),
    });
    let backup = Arc::new(FakeProvider::new("backup"));
    let store = Arc::new(SqliteModelStore::new(":memory:").unwrap());
    for key in ["primary", "backup"] {
        let mut definition = ProviderDefinition::new(key, key);
        definition.max_retries = 0;
        store.upsert_provider(&definition).await.unwrap();
    }
    store
        .upsert_profile(&profile("primary", "primary", 10))
        .await
        .unwrap();
    store
        .upsert_profile(&profile("backup", "backup", 1))
        .await
        .unwrap();

    let stream_manager = ModelManager::builder(store.clone())
        .add_provider(primary.clone())
        .add_provider(backup.clone())
        .with_usage_collector(store.clone())
        .build()
        .unwrap();
    let mut request = chat_request();
    request.config.max_retries = Some(0);
    let mut stream = stream_manager.stream(request).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ModelStreamEvent::Started { provider, .. } if provider == "backup"
    ));
    assert_eq!(primary.starts.load(Ordering::SeqCst), 1);

    let rate_store = Arc::new(core_agent_model::InMemoryModelCatalog::default());
    rate_store
        .upsert_profile(&profile("primary", "primary", 10))
        .await
        .unwrap();
    rate_store
        .upsert_profile(&profile("backup", "backup", 1))
        .await
        .unwrap();
    let rate_manager = ModelManager::builder(rate_store)
        .add_provider(Arc::new(FakeProvider::new("primary")))
        .add_provider(Arc::new(FakeProvider::new("backup")))
        .with_rate_limiter(Arc::new(PrimaryRateLimiter))
        .build()
        .unwrap();
    let mut request = chat_request();
    request.config.max_retries = Some(0);
    assert_eq!(
        rate_manager.generate(request).await.unwrap().provider,
        "backup"
    );
}

#[tokio::test]
async fn midstream_failure_never_switches_provider_and_hanging_stream_times_out() {
    let backup = Arc::new(FakeProvider::new("backup"));
    let (manager, store) = manager_with(
        vec![
            Arc::new(MidStreamFailProvider {
                key: "primary".into(),
            }),
            backup.clone(),
        ],
        vec![
            profile("primary", "primary", 10),
            profile("backup", "backup", 1),
        ],
    )
    .await;
    let mut stream = manager.stream(chat_request()).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ModelStreamEvent::Started { provider, .. } if provider == "primary"
    ));
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ModelStreamEvent::Delta { content } if content == "partial"
    ));
    assert!(stream.next().await.unwrap().is_err());
    assert_eq!(backup.calls.load(Ordering::SeqCst), 0);
    assert_eq!(store.usage_count().await.unwrap(), 1);

    let (timeout_manager, timeout_store) = manager_with(
        vec![Arc::new(HangingStreamProvider)],
        vec![profile("hanging", "hanging", 1)],
    )
    .await;
    let mut request = chat_request();
    request.config.timeout_ms = 10;
    request.config.max_retries = Some(0);
    let mut stream = timeout_manager.stream(request).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ModelStreamEvent::Started { .. }
    ));
    assert!(matches!(
        stream.next().await.unwrap(),
        Err(ModelError::Timeout { .. })
    ));
    assert_eq!(timeout_store.usage_count().await.unwrap(), 1);
}

#[tokio::test]
async fn operation_invariants_and_response_interceptor_audit_are_enforced() {
    let invariant_catalog = Arc::new(core_agent_model::InMemoryModelCatalog::default());
    invariant_catalog
        .upsert_profile(&ModelProfile::new("no-chat", "fake", "fake-model"))
        .await
        .unwrap();
    let invariant_provider = Arc::new(FakeProvider::new("fake"));
    let invariant_manager = ModelManager::builder(invariant_catalog)
        .add_provider(invariant_provider.clone())
        .add_request_interceptor(Arc::new(ClearingRequestInterceptor))
        .build()
        .unwrap();
    assert!(matches!(
        invariant_manager.generate(chat_request()).await,
        Err(ModelError::RouteNotFound(_))
    ));
    assert_eq!(invariant_provider.calls.load(Ordering::SeqCst), 0);

    let vision_catalog = Arc::new(core_agent_model::InMemoryModelCatalog::default());
    vision_catalog
        .upsert_profile(&profile("vision", "fake", 1))
        .await
        .unwrap();
    let vision_provider = Arc::new(FakeProvider::new("fake"));
    let vision_manager = ModelManager::builder(vision_catalog)
        .add_provider(vision_provider.clone())
        .add_request_interceptor(Arc::new(RemovingImageInterceptor))
        .build()
        .unwrap();
    let vision_request = ModelRequest::new(vec![ModelMessage {
        role: ModelRole::User,
        content: vec![ContentPart::ImageUrl {
            url: "https://example.com/image.png".into(),
            detail: None,
        }],
        name: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
    }]);
    assert!(matches!(
        vision_manager.vision(vision_request).await,
        Err(ModelError::InvalidArgument(_))
    ));
    assert_eq!(vision_provider.calls.load(Ordering::SeqCst), 0);

    let store = Arc::new(SqliteModelStore::new(":memory:").unwrap());
    store
        .upsert_profile(&profile("general", "fake", 1))
        .await
        .unwrap();
    let manager = ModelManager::builder(store.clone())
        .add_provider(Arc::new(FakeProvider::new("fake")))
        .add_request_interceptor(Arc::new(ClearingRequestInterceptor))
        .add_response_interceptor(Arc::new(RejectingResponseInterceptor))
        .with_usage_collector(store.clone())
        .build()
        .unwrap();

    assert!(matches!(
        manager.generate(chat_request()).await,
        Err(ModelError::Interceptor(_))
    ));
    let usage = store.list_usage(0, 10).await.unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].error_kind.as_deref(), Some("INTERCEPTOR"));
    assert_eq!(usage[0].provider, "fake");
    assert_eq!(usage[0].usage.total_tokens, 6);
}

#[tokio::test]
async fn usage_collector_failure_does_not_hide_successful_inference() {
    let catalog = Arc::new(core_agent_model::InMemoryModelCatalog::default());
    catalog
        .upsert_profile(&profile("general", "fake", 1))
        .await
        .unwrap();
    let observer = Arc::new(RecordingObserver::default());
    let manager = ModelManager::builder(catalog)
        .add_provider(Arc::new(FakeProvider::new("fake")))
        .with_usage_collector(Arc::new(FailingUsageCollector))
        .add_observer(observer.clone())
        .build()
        .unwrap();

    let response = manager.generate(chat_request()).await.unwrap();
    assert_eq!(response.text(), "fake:ok");
    assert_eq!(
        response.metadata.get("core_agent.usage_collection"),
        Some(&"FAILED".to_owned())
    );
    assert!(observer
        .stages
        .lock()
        .unwrap()
        .contains(&ModelStage::UsageFailed));
}

#[tokio::test]
async fn final_fallback_failure_is_attributed_to_actual_provider() {
    let (manager, store) = manager_with(
        vec![
            Arc::new(FakeProvider::failing("primary", usize::MAX)),
            Arc::new(FakeProvider::failing("backup", usize::MAX)),
        ],
        vec![
            profile("primary", "primary", 10),
            profile("backup", "backup", 1),
        ],
    )
    .await;
    let mut request = chat_request();
    request.config.max_retries = Some(0);
    assert!(manager.generate(request).await.is_err());
    let usage = store.list_usage(0, 10).await.unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].provider, "backup");
    assert!(!usage[0].success);
}

#[tokio::test]
async fn timeout_and_capability_failures_are_audited() {
    let slow = Arc::new(FakeProvider::slow("slow", 50));
    let mut chat_only = profile("chat", "slow", 1);
    chat_only.capabilities = BTreeSet::from([ModelCapability::Chat]);
    let (manager, store) = manager_with(vec![slow], vec![chat_only]).await;

    let mut timeout_request = chat_request();
    timeout_request.config.timeout_ms = 5;
    timeout_request.config.max_retries = Some(0);
    assert!(matches!(
        manager.generate(timeout_request).await,
        Err(ModelError::Timeout { .. })
    ));

    let vision_request = ModelRequest::new(vec![ModelMessage {
        role: ModelRole::User,
        content: vec![ContentPart::ImageUrl {
            url: "https://example.com/a.png".into(),
            detail: None,
        }],
        name: None,
        tool_call_id: None,
        tool_calls: Vec::new(),
    }]);
    assert!(manager.vision(vision_request).await.is_err());

    let usage = store.list_usage(0, 10).await.unwrap();
    assert_eq!(usage.len(), 2);
    assert!(usage.iter().all(|record| !record.success));
    assert!(usage
        .iter()
        .any(|record| record.error_kind.as_deref() == Some("TIMEOUT")));
    assert!(usage
        .iter()
        .any(|record| record.error_kind.as_deref() == Some("ROUTE_NOT_FOUND")));
}

#[derive(Default)]
struct RecordingObserver {
    stages: Mutex<Vec<ModelStage>>,
}

impl ModelObserver for RecordingObserver {
    fn on_observation(&self, observation: &ModelObservation) {
        self.stages.lock().unwrap().push(observation.stage);
    }
}

struct PanickingObserver;

impl ModelObserver for PanickingObserver {
    fn on_observation(&self, _observation: &ModelObservation) {
        panic!("observer must be isolated");
    }
}

#[tokio::test]
async fn observer_panics_do_not_change_inference_result() {
    let catalog = Arc::new(core_agent_model::InMemoryModelCatalog::default());
    catalog
        .upsert_profile(&profile("general", "fake", 1))
        .await
        .unwrap();
    let observer = Arc::new(RecordingObserver::default());
    let manager = ModelManager::builder(catalog)
        .add_provider(Arc::new(FakeProvider::new("fake")))
        .add_observer(Arc::new(PanickingObserver))
        .add_observer(observer.clone())
        .build()
        .unwrap();

    assert_eq!(
        manager.generate(chat_request()).await.unwrap().text(),
        "fake:ok"
    );
    let stages = observer.stages.lock().unwrap();
    assert!(stages.contains(&ModelStage::Routed));
    assert!(stages.contains(&ModelStage::Completed));
}

#[tokio::test]
async fn openai_compatible_provider_works_against_real_http_and_sse() {
    let (base_url, server) = spawn_openai_server(3).await;
    let store = Arc::new(SqliteModelStore::new(":memory:").unwrap());
    let mut definition = ProviderDefinition::new("compatible", "Compatible");
    definition.endpoint = Some(base_url.clone());
    definition.max_retries = 0;
    store.upsert_provider(&definition).await.unwrap();
    let chat = profile("chat", "compatible", 10);
    let embedding = ModelProfile::new("embedding", "compatible", "embed-model")
        .with_capability(ModelCapability::Embedding);
    store.upsert_profile(&chat).await.unwrap();
    store.upsert_profile(&embedding).await.unwrap();
    let provider = Arc::new(
        OpenAiCompatibleProvider::new("compatible", base_url, Some("test-secret".into())).unwrap(),
    );
    let manager = ModelManager::builder(store.clone())
        .add_provider(provider)
        .with_usage_collector(store.clone())
        .build()
        .unwrap();

    let generated = manager
        .generate(chat_request().with_profile("chat"))
        .await
        .unwrap();
    assert_eq!(generated.text(), "http-ok");
    assert_eq!(generated.usage.total_tokens, 8);

    let mut stream = manager
        .stream(chat_request().with_profile("chat"))
        .await
        .unwrap();
    let mut deltas = String::new();
    let mut completed = false;
    while let Some(event) = stream.next().await {
        match event.unwrap() {
            ModelStreamEvent::Delta { content } => deltas.push_str(&content),
            ModelStreamEvent::Completed(response) => {
                completed = true;
                assert_eq!(response.text(), "hello");
            }
            _ => {}
        }
    }
    assert_eq!(deltas, "hello");
    assert!(completed);

    let embeddings = manager
        .embedding(EmbeddingRequest::new(vec!["hello".into()]).with_profile("embedding"))
        .await
        .unwrap();
    assert_eq!(embeddings.embeddings, vec![vec![0.1, 0.2, 0.3]]);
    assert_eq!(store.usage_count().await.unwrap(), 3);

    server.await.unwrap();
}

#[tokio::test]
async fn openai_compatible_provider_rejects_truncated_sse() {
    let (base_url, server) = spawn_truncated_sse_server().await;
    let provider = OpenAiCompatibleProvider::new("compatible", base_url, None).unwrap();
    let request = chat_request();
    let profile = profile("chat", "compatible", 1);
    let mut stream = provider.stream(&request, &profile).await.unwrap();
    assert!(matches!(
        stream.next().await.unwrap().unwrap(),
        ModelStreamEvent::Delta { content } if content == "partial"
    ));
    assert!(matches!(
        stream.next().await.unwrap(),
        Err(ModelError::Provider { message, .. }) if message.contains("[DONE]")
    ));
    server.await.unwrap();
}

async fn spawn_openai_server(requests: usize) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for _ in 0..requests {
            let (mut socket, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut socket).await;
            let first_line = request.lines().next().unwrap_or_default();
            let body = request.split("\r\n\r\n").nth(1).unwrap_or_default();
            let (content_type, response_body) = if first_line.contains("/embeddings") {
                (
                    "application/json",
                    r#"{"data":[{"embedding":[0.1,0.2,0.3]}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#.to_owned(),
                )
            } else if body.contains("\"stream\":true") {
                (
                    "text/event-stream",
                    concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5}}\n\n",
                        "data: [DONE]\n\n"
                    )
                    .to_owned(),
                )
            } else {
                (
                    "application/json",
                    r#"{"choices":[{"message":{"content":"http-ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}}"#.to_owned(),
                )
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (format!("http://{address}/v1"), task)
}

async fn spawn_truncated_sse_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut socket).await;
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });
    (format!("http://{address}/v1"), task)
}

async fn read_http_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 2_048];
    let mut expected = None;
    loop {
        let read = socket.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(header_end) = find_bytes(&bytes, b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length: ")
                        .or_else(|| line.strip_prefix("Content-Length: "))
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            expected = Some(header_end + 4 + content_length);
        }
        if expected.is_some_and(|expected| bytes.len() >= expected) {
            break;
        }
    }
    String::from_utf8(bytes).unwrap()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}
