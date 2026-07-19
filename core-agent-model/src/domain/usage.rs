use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{ModelError, ModelResult};

use super::{audit_metadata, validate_audit_metadata};

/// Public operations recorded in the usage audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelOperation {
    Generate,
    Stream,
    Embedding,
    Vision,
}

impl ModelOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generate => "GENERATE",
            Self::Stream => "STREAM",
            Self::Embedding => "EMBEDDING",
            Self::Vision => "VISION",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "GENERATE" => Some(Self::Generate),
            "STREAM" => Some(Self::Stream),
            "EMBEDDING" => Some(Self::Embedding),
            "VISION" => Some(Self::Vision),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_tokens: u64,
    pub total_tokens: u64,
    pub latency_ms: u64,
    pub cost: Option<f64>,
}

impl ModelUsage {
    pub fn normalize(&mut self) {
        // Cached tokens are a subset of prompt_tokens for supported Providers.
        let calculated = self.prompt_tokens.saturating_add(self.completion_tokens);
        if self.total_tokens == 0 {
            self.total_tokens = calculated;
        }
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.cache_tokens > self.prompt_tokens {
            return Err(ModelError::InvalidArgument(
                "cache_tokens must be a subset of prompt_tokens".into(),
            ));
        }
        if self.total_tokens < self.prompt_tokens.saturating_add(self.completion_tokens) {
            return Err(ModelError::InvalidArgument(
                "total_tokens must include prompt and completion tokens".into(),
            ));
        }
        if self
            .cost
            .is_some_and(|cost| !cost.is_finite() || cost < 0.0)
        {
            return Err(ModelError::InvalidArgument(
                "usage cost must be finite and non-negative".into(),
            ));
        }
        Ok(())
    }
}

/// Durable, content-free audit record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Uuid,
    pub request_id: Uuid,
    pub operation: ModelOperation,
    pub provider: String,
    pub model: String,
    pub profile: String,
    pub usage: ModelUsage,
    pub success: bool,
    pub error_kind: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
}

impl UsageRecord {
    pub fn success(
        request_id: Uuid,
        operation: ModelOperation,
        provider: impl Into<String>,
        model: impl Into<String>,
        profile: impl Into<String>,
        usage: ModelUsage,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_id,
            operation,
            provider: provider.into(),
            model: model.into(),
            profile: profile.into(),
            usage,
            success: true,
            error_kind: None,
            metadata: audit_metadata(&metadata),
            created_at: Utc::now(),
        }
    }

    pub fn failure(
        request_id: Uuid,
        operation: ModelOperation,
        provider: impl Into<String>,
        model: impl Into<String>,
        profile: impl Into<String>,
        error_kind: impl Into<String>,
        metadata: BTreeMap<String, String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            request_id,
            operation,
            provider: provider.into(),
            model: model.into(),
            profile: profile.into(),
            usage: ModelUsage::default(),
            success: false,
            error_kind: Some(error_kind.into()),
            metadata: audit_metadata(&metadata),
            created_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> ModelResult<()> {
        self.usage.validate()?;
        validate_audit_metadata(&self.metadata)?;
        if self.success && self.error_kind.is_some() {
            return Err(ModelError::InvalidArgument(
                "successful usage record must not contain error_kind".into(),
            ));
        }
        if !self.success && self.error_kind.as_deref().is_none_or(str::is_empty) {
            return Err(ModelError::InvalidArgument(
                "failed usage record must contain error_kind".into(),
            ));
        }
        if self.success
            && (self.provider.is_empty() || self.model.is_empty() || self.profile.is_empty())
        {
            return Err(ModelError::InvalidArgument(
                "successful usage record requires provider, model and profile".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_normalizes_total() {
        let mut usage = ModelUsage {
            prompt_tokens: 3,
            completion_tokens: 2,
            cache_tokens: 1,
            ..Default::default()
        };
        usage.normalize();
        assert_eq!(usage.total_tokens, 5);
    }

    #[test]
    fn usage_record_keeps_only_allowlisted_metadata() {
        let record = UsageRecord::failure(
            Uuid::new_v4(),
            ModelOperation::Generate,
            "",
            "",
            "",
            "TEST",
            BTreeMap::from([
                ("trace_id".into(), "trace-1".into()),
                ("prompt".into(), "must not persist".into()),
            ]),
        );
        assert_eq!(record.metadata.len(), 1);
        assert_eq!(
            record.metadata.get("trace_id").map(String::as_str),
            Some("trace-1")
        );
    }
}
