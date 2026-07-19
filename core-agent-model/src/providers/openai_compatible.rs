use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, Url};
use serde_json::{json, Map, Value};

use crate::domain::{
    ContentPart, EmbeddingRequest, EmbeddingResponse, FinishReason, ImageDetail, ModelMessage,
    ModelProfile, ModelRequest, ModelResponse, ModelRole, ModelStreamEvent, ModelUsage,
    ToolCallDelta, ToolCallRequest,
};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{ModelProvider, ModelStream};

/// Real HTTP adapter for OpenAI-compatible chat/stream/embedding endpoints.
/// It also works with compatible endpoints such as DeepSeek, Qwen, Ollama,
/// LM Studio and OpenRouter. Credentials live only in this runtime instance.
pub struct OpenAiCompatibleProvider {
    key: String,
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        key: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> ModelResult<Self> {
        let key = key.into();
        if key.trim().is_empty() {
            return Err(ModelError::InvalidArgument(
                "OpenAI-compatible provider key must not be empty".into(),
            ));
        }
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        let parsed = Url::parse(&base_url)
            .map_err(|error| ModelError::InvalidArgument(format!("invalid endpoint: {error}")))?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(ModelError::InvalidArgument(
                "endpoint must be an http(s) base URL without credentials, query or fragment"
                    .into(),
            ));
        }
        let client = Client::builder()
            .build()
            .map_err(|error| ModelError::Internal(error.to_string()))?;
        Ok(Self {
            key,
            base_url,
            api_key,
            client,
        })
    }

    fn request(&self, path: &str, payload: &Value) -> reqwest::RequestBuilder {
        let request = self
            .client
            .post(format!("{}{path}", self.base_url))
            .json(payload);
        match &self.api_key {
            Some(api_key) => request.bearer_auth(api_key),
            None => request,
        }
    }

    async fn checked_response(&self, response: Response) -> ModelResult<Response> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let message = response
            .text()
            .await
            .unwrap_or_else(|_| "provider returned an unreadable error body".into());
        let message = self
            .api_key
            .as_deref()
            .filter(|secret| !secret.is_empty())
            .map_or(message.clone(), |secret| {
                message.replace(secret, "[REDACTED]")
            });
        Err(provider_http_error(&self.key, status, &message))
    }

    fn chat_payload(request: &ModelRequest, target: &ModelProfile, stream: bool) -> Value {
        let mut payload = Map::from_iter([
            ("model".into(), Value::String(target.model.clone())),
            (
                "messages".into(),
                Value::Array(request.messages.iter().map(message_json).collect()),
            ),
            ("stream".into(), Value::Bool(stream)),
        ]);
        if stream {
            payload.insert("stream_options".into(), json!({"include_usage": true}));
        }
        if let Some(value) = request.config.temperature {
            payload.insert("temperature".into(), json!(value));
        }
        if let Some(value) = request.config.top_p {
            payload.insert("top_p".into(), json!(value));
        }
        if let Some(value) = request.config.max_output_tokens {
            payload.insert("max_tokens".into(), json!(value));
        }
        if !request.config.stop.is_empty() {
            payload.insert("stop".into(), json!(request.config.stop));
        }
        if !request.tools.is_empty() {
            payload.insert(
                "tools".into(),
                Value::Array(
                    request
                        .tools
                        .iter()
                        .map(|tool| {
                            json!({
                                "type": "function",
                                "function": {
                                    "name": tool.name,
                                    "description": tool.description,
                                    "parameters": tool.parameters,
                                }
                            })
                        })
                        .collect(),
                ),
            );
            payload.insert("tool_choice".into(), Value::String("auto".into()));
        }
        Value::Object(payload)
    }
}

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    fn key(&self) -> &str {
        &self.key
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelResponse> {
        let started = Instant::now();
        let payload = Self::chat_payload(request, target, false);
        let response = self
            .request("/chat/completions", &payload)
            .send()
            .await
            .map_err(|error| transport_error(&self.key, error))?;
        let response = self.checked_response(response).await?;
        let raw: Value = response
            .json()
            .await
            .map_err(|error| ModelError::Serialization(error.to_string()))?;
        parse_chat_response(request, target, &self.key, raw, elapsed_ms(started))
    }

    async fn stream(
        &self,
        request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelStream> {
        let started = Instant::now();
        let payload = Self::chat_payload(request, target, true);
        let response = self
            .request("/chat/completions", &payload)
            .send()
            .await
            .map_err(|error| transport_error(&self.key, error))?;
        let response = self.checked_response(response).await?;
        let request_id = request.id;
        let provider_key = self.key.clone();
        let profile = target.clone();
        let metadata = request.metadata.clone();
        let stream = async_stream::try_stream! {
            let mut events = response.bytes_stream().eventsource();
            let mut content = String::new();
            let mut usage = ModelUsage::default();
            let mut finish_reason = FinishReason::Stop;
            let mut tool_parts: BTreeMap<usize, (String, String, String)> = BTreeMap::new();
            let mut completed = false;
            while let Some(event) = events.next().await {
                let event = event.map_err(|error| ModelError::Provider {
                    provider: provider_key.clone(),
                    message: format!("invalid SSE stream: {error}"),
                    status: None,
                    retryable: false,
                })?;
                if event.data == "[DONE]" {
                    let response = completed_stream_response(
                        request_id,
                        &provider_key,
                        &profile,
                        &content,
                        &tool_parts,
                        usage.clone(),
                        finish_reason.clone(),
                        metadata.clone(),
                        elapsed_ms(started),
                    );
                    yield ModelStreamEvent::Completed(response);
                    completed = true;
                    break;
                }
                let chunk: Value = serde_json::from_str(&event.data)
                    .map_err(|error| ModelError::Serialization(error.to_string()))?;
                if let Some(error) = chunk.get("error") {
                    Err(ModelError::Provider {
                        provider: provider_key.clone(),
                        message: truncate(&error.to_string(), 2_048),
                        status: None,
                        retryable: false,
                    })?;
                }
                if let Some(parsed) = chunk.get("usage") {
                    usage = parse_usage(Some(parsed));
                    yield ModelStreamEvent::Usage(usage.clone());
                }
                if let Some(choice) = chunk.get("choices").and_then(Value::as_array).and_then(|choices| choices.first()) {
                    if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                        finish_reason = FinishReason::from_provider(Some(reason));
                    }
                    if let Some(delta) = choice.get("delta") {
                        if let Some(text) = delta.get("content").and_then(Value::as_str) {
                            content.push_str(text);
                            yield ModelStreamEvent::Delta { content: text.to_owned() };
                        }
                        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
                            for call in calls {
                                let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                                let entry = tool_parts.entry(index).or_default();
                                let id = call.get("id").and_then(Value::as_str).map(str::to_owned);
                                if let Some(id) = &id {
                                    entry.0.clone_from(id);
                                }
                                let name = call.pointer("/function/name").and_then(Value::as_str).map(str::to_owned);
                                if let Some(name) = &name {
                                    entry.1.push_str(name);
                                }
                                let arguments_delta = call
                                    .pointer("/function/arguments")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned();
                                entry.2.push_str(&arguments_delta);
                                yield ModelStreamEvent::ToolCallDelta(ToolCallDelta {
                                    index,
                                    id,
                                    name,
                                    arguments_delta,
                                });
                            }
                        }
                    }
                }
            }
            if !completed {
                Err(ModelError::Provider {
                    provider: provider_key,
                    message: "SSE stream ended before [DONE]".into(),
                    status: None,
                    retryable: false,
                })?;
            }
        };
        Ok(Box::pin(stream))
    }

    async fn embedding(
        &self,
        request: &EmbeddingRequest,
        target: &ModelProfile,
    ) -> ModelResult<EmbeddingResponse> {
        let started = Instant::now();
        let payload = json!({"model": target.model, "input": request.inputs});
        let response = self
            .request("/embeddings", &payload)
            .send()
            .await
            .map_err(|error| transport_error(&self.key, error))?;
        let response = self.checked_response(response).await?;
        let raw: Value = response
            .json()
            .await
            .map_err(|error| ModelError::Serialization(error.to_string()))?;
        let embeddings = raw
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ModelError::Serialization("embedding response missing data".into()))?
            .iter()
            .map(|item| {
                item.get("embedding")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        ModelError::Serialization("embedding item missing vector".into())
                    })?
                    .iter()
                    .map(|value| {
                        let value = value.as_f64().ok_or_else(|| {
                            ModelError::Serialization("embedding contains non-number".into())
                        })?;
                        let converted = value as f32;
                        if !converted.is_finite() {
                            return Err(ModelError::Serialization(
                                "embedding contains an out-of-range number".into(),
                            ));
                        }
                        Ok(converted)
                    })
                    .collect::<ModelResult<Vec<_>>>()
            })
            .collect::<ModelResult<Vec<_>>>()?;
        let dimensions = embeddings.first().map(Vec::len).unwrap_or(0);
        let mut usage = parse_usage(raw.get("usage"));
        usage.latency_ms = elapsed_ms(started);
        Ok(EmbeddingResponse {
            request_id: request.id,
            provider: self.key.clone(),
            model: target.model.clone(),
            profile: target.key.clone(),
            embeddings,
            dimensions,
            usage,
            metadata: request.metadata.clone(),
            raw_response: Some(raw),
        })
    }

    async fn vision(
        &self,
        request: &ModelRequest,
        target: &ModelProfile,
    ) -> ModelResult<ModelResponse> {
        self.invoke(request, target).await
    }
}

fn message_json(message: &ModelMessage) -> Value {
    let role = match message.role {
        ModelRole::System => "system",
        ModelRole::User => "user",
        ModelRole::Assistant => "assistant",
        ModelRole::Tool => "tool",
    };
    let content = if message.content.is_empty() {
        Value::Null
    } else if let [ContentPart::Text { text }] = message.content.as_slice() {
        Value::String(text.clone())
    } else {
        Value::Array(
            message
                .content
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => json!({"type": "text", "text": text}),
                    ContentPart::ImageUrl { url, detail } => json!({
                        "type": "image_url",
                        "image_url": {
                            "url": url,
                            "detail": detail.map(image_detail).unwrap_or("auto")
                        }
                    }),
                })
                .collect(),
        )
    };
    let mut value = json!({"role": role, "content": content});
    if let Some(name) = &message.name {
        value["name"] = Value::String(name.clone());
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        value["tool_call_id"] = Value::String(tool_call_id.clone());
    }
    if !message.tool_calls.is_empty() {
        value["tool_calls"] = Value::Array(
            message
                .tool_calls
                .iter()
                .map(|call| {
                    json!({
                        "id": call.id,
                        "type": "function",
                        "function": {
                            "name": call.name,
                            "arguments": serde_json::to_string(&call.arguments)
                                .unwrap_or_else(|_| "{}".into()),
                        }
                    })
                })
                .collect(),
        );
    }
    value
}

fn image_detail(detail: ImageDetail) -> &'static str {
    match detail {
        ImageDetail::Auto => "auto",
        ImageDetail::Low => "low",
        ImageDetail::High => "high",
    }
}

fn parse_chat_response(
    request: &ModelRequest,
    target: &ModelProfile,
    provider: &str,
    raw: Value,
    latency_ms: u64,
) -> ModelResult<ModelResponse> {
    let choice = raw
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .ok_or_else(|| ModelError::Serialization("chat response missing choices".into()))?;
    let message = choice
        .get("message")
        .ok_or_else(|| ModelError::Serialization("chat response missing message".into()))?;
    let content = parse_content(message.get("content"));
    let tool_calls = parse_tool_calls(message.get("tool_calls"));
    let mut usage = parse_usage(raw.get("usage"));
    usage.latency_ms = latency_ms;
    Ok(ModelResponse {
        request_id: request.id,
        provider: provider.to_owned(),
        model: target.model.clone(),
        profile: target.key.clone(),
        content,
        tool_calls,
        usage,
        finish_reason: FinishReason::from_provider(
            choice.get("finish_reason").and_then(Value::as_str),
        ),
        metadata: request.metadata.clone(),
        raw_response: Some(raw),
    })
}

fn parse_content(value: Option<&Value>) -> Vec<ContentPart> {
    match value {
        Some(Value::String(text)) => vec![ContentPart::text(text)],
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .map(ContentPart::text)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_tool_calls(value: Option<&Value>) -> Vec<ToolCallRequest> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| {
            let id = call.get("id")?.as_str()?.to_owned();
            let name = call.pointer("/function/name")?.as_str()?.to_owned();
            let raw_arguments = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(raw_arguments)
                .unwrap_or_else(|_| Value::String(raw_arguments.to_owned()));
            Some(ToolCallRequest {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

fn parse_usage(value: Option<&Value>) -> ModelUsage {
    let mut usage = ModelUsage {
        prompt_tokens: value
            .and_then(|value| value.get("prompt_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        completion_tokens: value
            .and_then(|value| value.get("completion_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        cache_tokens: value
            .and_then(|value| value.pointer("/prompt_tokens_details/cached_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        total_tokens: value
            .and_then(|value| value.get("total_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
        latency_ms: 0,
        cost: None,
    };
    usage.normalize();
    usage
}

#[allow(clippy::too_many_arguments)]
fn completed_stream_response(
    request_id: uuid::Uuid,
    provider: &str,
    profile: &ModelProfile,
    content: &str,
    tool_parts: &BTreeMap<usize, (String, String, String)>,
    mut usage: ModelUsage,
    finish_reason: FinishReason,
    metadata: BTreeMap<String, String>,
    latency_ms: u64,
) -> ModelResponse {
    usage.latency_ms = latency_ms;
    usage.normalize();
    ModelResponse {
        request_id,
        provider: provider.to_owned(),
        model: profile.model.clone(),
        profile: profile.key.clone(),
        content: if content.is_empty() {
            Vec::new()
        } else {
            vec![ContentPart::text(content)]
        },
        tool_calls: tool_parts
            .values()
            .map(|(id, name, arguments)| ToolCallRequest {
                id: id.clone(),
                name: name.clone(),
                arguments: serde_json::from_str(arguments)
                    .unwrap_or_else(|_| Value::String(arguments.clone())),
            })
            .collect(),
        usage,
        finish_reason,
        metadata,
        raw_response: None,
    }
}

fn transport_error(provider: &str, error: reqwest::Error) -> ModelError {
    ModelError::Provider {
        provider: provider.to_owned(),
        message: error.to_string(),
        status: error.status().map(|status| status.as_u16()),
        retryable: error.is_timeout() || error.is_connect() || error.is_request(),
    }
}

fn provider_http_error(provider: &str, status: StatusCode, body: &str) -> ModelError {
    ModelError::Provider {
        provider: provider.to_owned(),
        message: truncate(body, 2_048),
        status: Some(status.as_u16()),
        retryable: matches!(status.as_u16(), 408 | 409 | 425 | 429 | 500..=599),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ModelRole, ModelToolDefinition, ModelUsage};

    #[test]
    fn endpoint_rejects_embedded_credentials() {
        assert!(OpenAiCompatibleProvider::new(
            "openai",
            "https://user:password@example.com/v1",
            None,
        )
        .is_err());
    }

    #[test]
    fn chat_response_parses_tool_call_and_usage() {
        let request = ModelRequest::new(vec![ModelMessage::text(ModelRole::User, "hello")]);
        let profile = ModelProfile::new("coding", "openai", "gpt");
        let response = parse_chat_response(
            &request,
            &profile,
            "openai",
            json!({
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": [{"id":"call-1","function":{"name":"search","arguments":"{\"q\":\"rust\"}"}}]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7}
            }),
            12,
        )
        .unwrap();
        assert_eq!(response.tool_calls[0].name, "search");
        assert_eq!(
            response.usage,
            ModelUsage {
                prompt_tokens: 5,
                completion_tokens: 2,
                total_tokens: 7,
                latency_ms: 12,
                ..Default::default()
            }
        );
        assert_eq!(response.finish_reason, FinishReason::ToolCall);
    }

    #[test]
    fn chat_payload_advertises_tools_and_preserves_call_correlation() {
        let mut request = ModelRequest::new(vec![ModelMessage::text(ModelRole::User, "inspect")]);
        request.tools.push(ModelToolDefinition {
            name: "read_file".into(),
            description: "Read a workspace file".into(),
            parameters: json!({"type":"object","properties":{"path":{"type":"string"}}}),
        });
        request.messages.push(ModelMessage::assistant_tool_calls(
            "",
            vec![crate::domain::ModelToolCall {
                id: "call-1".into(),
                name: "read_file".into(),
                arguments: json!({"path":"README.md"}),
            }],
        ));
        request
            .messages
            .push(ModelMessage::tool_result("call-1", "read_file", "# Demo"));
        request.validate().unwrap();

        let payload = OpenAiCompatibleProvider::chat_payload(
            &request,
            &ModelProfile::new("coding", "openai", "gpt"),
            false,
        );
        assert_eq!(payload["tools"][0]["function"]["name"], "read_file");
        assert_eq!(payload["tool_choice"], "auto");
        assert_eq!(payload["messages"][1]["tool_calls"][0]["id"], "call-1");
        assert_eq!(payload["messages"][2]["tool_call_id"], "call-1");
    }

    #[test]
    fn image_message_uses_multimodal_wire_shape() {
        let message = ModelMessage {
            role: ModelRole::User,
            content: vec![ContentPart::ImageUrl {
                url: "data:image/png;base64,abc".into(),
                detail: Some(ImageDetail::Low),
            }],
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        };
        assert_eq!(
            message_json(&message)["content"][0]["image_url"]["detail"],
            "low"
        );
    }
}
