use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ModelError, ModelResult};

use super::{validate_metadata, ModelCapability, RoutingRequest, RoutingStrategy};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageDetail {
    Auto,
    Low,
    High,
}

/// Multimodal content without any Provider wire-format fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    ImageUrl {
        url: String,
        detail: Option<ImageDetail>,
    },
}

impl ContentPart {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text { text: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: ModelRole,
    pub content: Vec<ContentPart>,
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ModelToolCall>,
}

impl ModelMessage {
    pub fn text(role: ModelRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentPart::text(content)],
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_tool_calls(
        content: impl Into<String>,
        tool_calls: Vec<ModelToolCall>,
    ) -> Self {
        let content = content.into();
        Self {
            role: ModelRole::Assistant,
            content: (!content.is_empty())
                .then(|| ContentPart::text(content))
                .into_iter()
                .collect(),
            name: None,
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn tool_result(
        tool_call_id: impl Into<String>,
        name: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            role: ModelRole::Tool,
            content: vec![ContentPart::text(content)],
            name: Some(name.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelConfig {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u64>,
    pub stop: Vec<String>,
    pub timeout_ms: u64,
    pub max_retries: Option<u32>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            stop: Vec::new(),
            timeout_ms: 60_000,
            max_retries: None,
        }
    }
}

impl ModelConfig {
    pub fn validate(&self) -> ModelResult<()> {
        if self.timeout_ms == 0 {
            return Err(ModelError::InvalidArgument(
                "timeout_ms must be greater than zero".into(),
            ));
        }
        if self
            .temperature
            .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
        {
            return Err(ModelError::InvalidArgument(
                "temperature must be finite and between 0 and 2".into(),
            ));
        }
        if self
            .top_p
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(ModelError::InvalidArgument(
                "top_p must be finite and between 0 and 1".into(),
            ));
        }
        if self.max_output_tokens == Some(0) {
            return Err(ModelError::InvalidArgument(
                "max_output_tokens must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

/// Unified request for Chat, Vision and future tool-call return values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub id: Uuid,
    pub messages: Vec<ModelMessage>,
    pub config: ModelConfig,
    pub profile: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub required_capabilities: BTreeSet<ModelCapability>,
    pub strategy: RoutingStrategy,
    pub metadata: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ModelToolDefinition>,
    pub created_at: DateTime<Utc>,
}

impl ModelRequest {
    pub fn new(messages: Vec<ModelMessage>) -> Self {
        Self {
            id: Uuid::new_v4(),
            messages,
            config: ModelConfig::default(),
            profile: None,
            provider: None,
            model: None,
            required_capabilities: BTreeSet::new(),
            strategy: RoutingStrategy::Auto,
            metadata: BTreeMap::new(),
            tools: Vec::new(),
            created_at: Utc::now(),
        }
    }

    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn require(mut self, capability: ModelCapability) -> Self {
        self.required_capabilities.insert(capability);
        self
    }

    pub fn validate(&self) -> ModelResult<()> {
        self.config.validate()?;
        if self.messages.is_empty() {
            return Err(ModelError::InvalidArgument(
                "model request must contain at least one message".into(),
            ));
        }
        if self.messages.iter().any(|message| {
            message.content.is_empty()
                && !(message.role == ModelRole::Assistant && !message.tool_calls.is_empty())
        }) {
            return Err(ModelError::InvalidArgument(
                "every message must contain at least one content part".into(),
            ));
        }
        if self
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|part| matches!(part, ContentPart::Text { text } if text.is_empty()))
        {
            return Err(ModelError::InvalidArgument(
                "text content must not be empty".into(),
            ));
        }
        if self.messages.iter().any(|message| {
            (message.role == ModelRole::Tool) != message.tool_call_id.is_some()
                || (message.role != ModelRole::Assistant && !message.tool_calls.is_empty())
                || message.tool_call_id.as_ref().is_some_and(|value| {
                    value.trim().is_empty()
                        || value.len() > 256
                        || value.chars().any(char::is_control)
                })
                || message.tool_calls.iter().any(|call| {
                    call.id.trim().is_empty()
                        || call.id.len() > 256
                        || call.name.trim().is_empty()
                        || call.name.len() > 128
                        || !call.name.bytes().all(|value| {
                            value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-')
                        })
                        || !call.arguments.is_object()
                })
        }) {
            return Err(ModelError::InvalidArgument(
                "model tool message correlation is invalid".into(),
            ));
        }
        validate_metadata(&self.metadata, "request metadata")?;
        for tool in &self.tools {
            if tool.name.trim().is_empty()
                || tool.name.len() > 128
                || !tool
                    .name
                    .bytes()
                    .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-'))
                || tool.description.len() > 4_096
                || !tool.parameters.is_object()
            {
                return Err(ModelError::InvalidArgument(
                    "model tool definition is invalid".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn has_image(&self) -> bool {
        self.messages
            .iter()
            .flat_map(|message| &message.content)
            .any(|part| matches!(part, ContentPart::ImageUrl { .. }))
    }

    pub fn routing_request(&self) -> RoutingRequest {
        RoutingRequest {
            profile: self.profile.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            required_capabilities: self.required_capabilities.clone(),
            max_output_tokens: self.config.max_output_tokens,
            strategy: self.strategy,
        }
    }
}

/// Unified Embedding input kept separate from Chat messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingRequest {
    pub id: Uuid,
    pub inputs: Vec<String>,
    pub profile: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub strategy: RoutingStrategy,
    pub metadata: BTreeMap<String, String>,
    pub timeout_ms: u64,
    pub max_retries: Option<u32>,
}

impl EmbeddingRequest {
    pub fn new(inputs: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            inputs,
            profile: None,
            provider: None,
            model: None,
            strategy: RoutingStrategy::Auto,
            metadata: BTreeMap::new(),
            timeout_ms: 60_000,
            max_retries: None,
        }
    }

    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.inputs.is_empty() || self.inputs.iter().any(String::is_empty) {
            return Err(ModelError::InvalidArgument(
                "embedding inputs must not be empty".into(),
            ));
        }
        if self.timeout_ms == 0 {
            return Err(ModelError::InvalidArgument(
                "timeout_ms must be greater than zero".into(),
            ));
        }
        validate_metadata(&self.metadata, "embedding metadata")?;
        Ok(())
    }

    pub fn routing_request(&self) -> RoutingRequest {
        RoutingRequest {
            profile: self.profile.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            required_capabilities: BTreeSet::from([ModelCapability::Embedding]),
            max_output_tokens: None,
            strategy: self.strategy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_model_request_is_rejected() {
        assert!(ModelRequest::new(Vec::new()).validate().is_err());
    }

    #[test]
    fn invalid_sampling_config_is_rejected() {
        let mut request = ModelRequest::new(vec![ModelMessage::text(ModelRole::User, "hello")]);
        request.config.temperature = Some(3.0);
        assert!(request.validate().is_err());
    }

    #[test]
    fn vision_input_is_detected() {
        let request = ModelRequest::new(vec![ModelMessage {
            role: ModelRole::User,
            content: vec![ContentPart::ImageUrl {
                url: "https://example.com/image.png".into(),
                detail: Some(ImageDetail::High),
            }],
            name: None,
            tool_call_id: None,
            tool_calls: Vec::new(),
        }]);
        assert!(request.has_image());
    }
}
