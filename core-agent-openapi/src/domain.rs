//! OpenAPI Platform — domain types for external API integration.
//!
//! Defines the request/response types, API key model, rate limit
//! model, and gateway abstraction for external systems to call agents.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{OpenApiError, OpenApiResult};

// ── API Key ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiKeyScope {
    AgentChat,
    AgentExecute,
    WorkflowRun,
    KnowledgeSearch,
    Admin,
}

impl ApiKeyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentChat => "agent.chat",
            Self::AgentExecute => "agent.execute",
            Self::WorkflowRun => "workflow.run",
            Self::KnowledgeSearch => "knowledge.search",
            Self::Admin => "admin",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "agent.chat" => Some(Self::AgentChat),
            "agent.execute" => Some(Self::AgentExecute),
            "workflow.run" => Some(Self::WorkflowRun),
            "knowledge.search" => Some(Self::KnowledgeSearch),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiKeyState {
    Active,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub key_prefix: String,
    pub key_hash: String,
    pub name: String,
    pub scopes: Vec<ApiKeyScope>,
    pub state: ApiKeyState,
    pub quota: ApiKeyQuota,
    pub expires_at: Option<DateTime<Utc>>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn new(
        tenant_id: Uuid,
        key_prefix: impl Into<String>,
        key_hash: impl Into<String>,
        name: impl Into<String>,
        scopes: Vec<ApiKeyScope>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            key_prefix: key_prefix.into(),
            key_hash: key_hash.into(),
            name: name.into(),
            scopes,
            state: ApiKeyState::Active,
            quota: ApiKeyQuota::default(),
            expires_at: None,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> OpenApiResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(OpenApiError::Validation("api key name is invalid".into()));
        }
        if self.key_prefix.len() > 16 {
            return Err(OpenApiError::Validation("key prefix too long".into()));
        }
        if self.key_hash.len() != 64 || !self.key_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(OpenApiError::Validation("key hash must be SHA-256 hex".into()));
        }
        if self.scopes.is_empty() {
            return Err(OpenApiError::Validation("at least one scope required".into()));
        }
        Ok(())
    }

    pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
        self.scopes.contains(&scope) || self.scopes.contains(&ApiKeyScope::Admin)
    }

    pub fn is_active(&self) -> bool {
        self.state == ApiKeyState::Active
            && self.expires_at.map_or(true, |exp| Utc::now() < exp)
    }
}

// ── API Key Quota ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ApiKeyQuota {
    pub max_requests_per_day: u64,
    pub max_tokens_per_day: u64,
    pub requests_used: u64,
    pub tokens_used: u64,
    pub quota_reset_at: DateTime<Utc>,
}

impl Default for ApiKeyQuota {
    fn default() -> Self {
        Self {
            max_requests_per_day: 1000,
            max_tokens_per_day: 1_000_000,
            requests_used: 0,
            tokens_used: 0,
            quota_reset_at: Utc::now(),
        }
    }
}

impl ApiKeyQuota {
    pub fn is_exhausted(&self) -> bool {
        self.requests_used >= self.max_requests_per_day
            || self.tokens_used >= self.max_tokens_per_day
    }

    pub fn record_usage(&mut self, tokens: u64) {
        self.requests_used = self.requests_used.saturating_add(1);
        self.tokens_used = self.tokens_used.saturating_add(tokens);
    }
}

// ── Rate Limit ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests_per_second: u64,
    pub burst_size: u64,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            requests_per_second: 10,
            burst_size: 20,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RateLimitStatus {
    pub allowed: bool,
    pub remaining: u64,
    pub reset_at: DateTime<Utc>,
}

// ── API Request Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatApiRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub stream: Option<bool>,
    pub context: Option<BTreeMap<String, Value>>,
}

impl AgentChatApiRequest {
    pub fn validate(&self) -> OpenApiResult<()> {
        if self.message.trim().is_empty() || self.message.len() > 65536 {
            return Err(OpenApiError::Validation(
                "message must be 1..=65536 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentChatApiResponse {
    pub id: Uuid,
    pub message: String,
    pub finish_reason: String,
    pub usage: Option<TokenUsage>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskApiRequest {
    pub task: String,
    pub inputs: BTreeMap<String, Value>,
    pub timeout_secs: Option<u64>,
}

impl TaskApiRequest {
    pub fn validate(&self) -> OpenApiResult<()> {
        if self.task.trim().is_empty() || self.task.len() > 65536 {
            return Err(OpenApiError::Validation(
                "task must be 1..=65536 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskApiResponse {
    pub id: Uuid,
    pub status: String,
    pub output: Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunApiRequest {
    pub workflow_key: String,
    pub inputs: BTreeMap<String, Value>,
    pub timeout_secs: Option<u64>,
}

impl WorkflowRunApiRequest {
    pub fn validate(&self) -> OpenApiResult<()> {
        if self.workflow_key.trim().is_empty() || self.workflow_key.len() > 256 {
            return Err(OpenApiError::Validation(
                "workflow key must be 1..=256 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunApiResponse {
    pub id: Uuid,
    pub status: String,
    pub output: Value,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchApiRequest {
    pub query: String,
    pub max_results: Option<u64>,
    pub filters: Option<BTreeMap<String, String>>,
}

impl KnowledgeSearchApiRequest {
    pub fn validate(&self) -> OpenApiResult<()> {
        if self.query.trim().is_empty() || self.query.len() > 4096 {
            return Err(OpenApiError::Validation(
                "query must be 1..=4096 characters".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchApiResponse {
    pub results: Vec<KnowledgeSearchResult>,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSearchResult {
    pub id: String,
    pub content: String,
    pub score: f64,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

// ── Error Response ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub code: String,
    pub message: String,
    pub request_id: Uuid,
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_api_key() {
        let key = ApiKey::new(
            Uuid::new_v4(),
            "sk-agent",
            "a".repeat(64),
            "My API Key",
            vec![ApiKeyScope::AgentChat],
            "admin",
        );
        assert!(key.validate().is_ok());
        assert!(key.is_active());
        assert!(key.has_scope(ApiKeyScope::AgentChat));
    }

    #[test]
    fn api_key_scope_check() {
        let key = ApiKey::new(
            Uuid::new_v4(),
            "sk-agent",
            "a".repeat(64),
            "Admin Key",
            vec![ApiKeyScope::Admin],
            "admin",
        );
        assert!(key.has_scope(ApiKeyScope::AgentChat));
        assert!(key.has_scope(ApiKeyScope::WorkflowRun));
    }

    #[test]
    fn api_key_empty_name() {
        let key = ApiKey::new(
            Uuid::new_v4(),
            "sk-agent",
            "a".repeat(64),
            "",
            vec![ApiKeyScope::AgentChat],
            "admin",
        );
        assert!(key.validate().is_err());
    }

    #[test]
    fn api_key_no_scopes() {
        let key = ApiKey::new(
            Uuid::new_v4(),
            "sk-agent",
            "a".repeat(64),
            "Key",
            vec![],
            "admin",
        );
        assert!(key.validate().is_err());
    }

    #[test]
    fn quota_exhaustion() {
        let mut quota = ApiKeyQuota::default();
        assert!(!quota.is_exhausted());
        quota.requests_used = quota.max_requests_per_day;
        assert!(quota.is_exhausted());
    }

    #[test]
    fn quota_record_usage() {
        let mut quota = ApiKeyQuota::default();
        quota.record_usage(500);
        assert_eq!(quota.requests_used, 1);
        assert_eq!(quota.tokens_used, 500);
    }

    #[test]
    fn api_key_scope_serde() {
        let scope = ApiKeyScope::AgentChat;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"AGENT_CHAT\"");
        let back: ApiKeyScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ApiKeyScope::AgentChat);
    }

    #[test]
    fn api_key_scope_parse() {
        assert_eq!(ApiKeyScope::parse("agent.chat"), Some(ApiKeyScope::AgentChat));
        assert_eq!(ApiKeyScope::parse("admin"), Some(ApiKeyScope::Admin));
        assert_eq!(ApiKeyScope::parse("unknown"), None);
    }

    #[test]
    fn chat_request_validation() {
        let req = AgentChatApiRequest {
            message: "".into(),
            session_id: None,
            stream: None,
            context: None,
        };
        assert!(req.validate().is_err());

        let req = AgentChatApiRequest {
            message: "Hello".into(),
            session_id: None,
            stream: None,
            context: None,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn rate_limit_defaults() {
        let limit = RateLimit::default();
        assert_eq!(limit.requests_per_second, 10);
        assert_eq!(limit.burst_size, 20);
    }
}