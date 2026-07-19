use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ContentPart, ModelUsage};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCall,
    ContentFilter,
    Error,
    Other(String),
}

impl FinishReason {
    pub fn from_provider(value: Option<&str>) -> Self {
        match value {
            Some("stop") | None => Self::Stop,
            Some("length") | Some("max_tokens") => Self::Length,
            Some("tool_calls") | Some("function_call") => Self::ToolCall,
            Some("content_filter") | Some("safety") => Self::ContentFilter,
            Some("error") => Self::Error,
            Some(value) => Self::Other(value.to_owned()),
        }
    }
}

/// Tool call requested by a model. Model Runtime returns it but never executes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub request_id: Uuid,
    pub provider: String,
    pub model: String,
    pub profile: String,
    pub content: Vec<ContentPart>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub usage: ModelUsage,
    pub finish_reason: FinishReason,
    pub metadata: BTreeMap<String, String>,
    pub raw_response: Option<serde_json::Value>,
}

impl ModelResponse {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                ContentPart::ImageUrl { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    pub request_id: Uuid,
    pub provider: String,
    pub model: String,
    pub profile: String,
    pub embeddings: Vec<Vec<f32>>,
    pub dimensions: usize,
    pub usage: ModelUsage,
    pub metadata: BTreeMap<String, String>,
    pub raw_response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_delta: String,
}

/// Every Provider stream must end with exactly one `Completed` event on success.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelStreamEvent {
    Started {
        request_id: Uuid,
        provider: String,
        model: String,
        profile: String,
    },
    Delta {
        content: String,
    },
    ToolCallDelta(ToolCallDelta),
    Usage(ModelUsage),
    Completed(ModelResponse),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_text_joins_text_parts_only() {
        let response = ModelResponse {
            request_id: Uuid::new_v4(),
            provider: "p".into(),
            model: "m".into(),
            profile: "profile".into(),
            content: vec![ContentPart::text("a"), ContentPart::text("b")],
            tool_calls: Vec::new(),
            usage: ModelUsage::default(),
            finish_reason: FinishReason::Stop,
            metadata: BTreeMap::new(),
            raw_response: None,
        };
        assert_eq!(response.text(), "ab");
    }
}
