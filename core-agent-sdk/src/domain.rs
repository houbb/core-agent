//! Core-Agent SDK — domain types for building, testing, and publishing agents.
//!
//! Defines the public contracts that third-party developers use to
//! interact with the Core-Agent ecosystem.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{SdkError, SdkResult};

// ── Agent Client ──────────────────────────────────────────────────────────

/// Request to send a chat message to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session_id: Option<String>,
    pub context: Option<BTreeMap<String, Value>>,
    pub stream: bool,
}

impl Default for ChatRequest {
    fn default() -> Self {
        Self {
            message: String::new(),
            session_id: None,
            context: None,
            stream: false,
        }
    }
}

impl ChatRequest {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            ..Default::default()
        }
    }
}

/// Response from a chat interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: Uuid,
    pub message: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Option<TokenUsage>,
    pub created_at: DateTime<Utc>,
}

/// Request to execute a task on an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteRequest {
    pub task: String,
    pub inputs: BTreeMap<String, Value>,
    pub timeout_secs: Option<u64>,
}

/// Response from a task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponse {
    pub id: Uuid,
    pub output: Value,
    pub status: ExecutionStatus,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    Success,
    Failure,
    Timeout,
    Cancelled,
}

/// A tool call made during agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub result: Option<Value>,
}

/// Token usage tracking.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

// ── Agent Builder ─────────────────────────────────────────────────────────

/// Builder for constructing an AgentClient.
#[derive(Debug, Clone)]
pub struct AgentBuilder {
    pub name: String,
    pub version: String,
    pub model: String,
    pub tools: Vec<String>,
    pub skills: Vec<String>,
    pub instructions: String,
    pub metadata: BTreeMap<String, String>,
}

impl AgentBuilder {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            model: "default".into(),
            tools: Vec::new(),
            skills: Vec::new(),
            instructions: String::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn model(mut self, value: impl Into<String>) -> Self {
        self.model = value.into();
        self
    }

    pub fn tool(mut self, value: impl Into<String>) -> Self {
        self.tools.push(value.into());
        self
    }

    pub fn tools(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools.extend(values.into_iter().map(Into::into));
        self
    }

    pub fn skill(mut self, value: impl Into<String>) -> Self {
        self.skills.push(value.into());
        self
    }

    pub fn skills(mut self, values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.skills.extend(values.into_iter().map(Into::into));
        self
    }

    pub fn instructions(mut self, value: impl Into<String>) -> Self {
        self.instructions = value.into();
        self
    }

    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn validate(&self) -> SdkResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(SdkError::Validation("agent name must be 1..=256 characters".into()));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(SdkError::Validation("agent version must be 1..=64 characters".into()));
        }
        if self.model.trim().is_empty() || self.model.len() > 128 {
            return Err(SdkError::Validation("model name must be 1..=128 characters".into()));
        }
        Ok(())
    }
}

// ── Tool / Skill / Plugin Trait Definitions ───────────────────────────────

/// Trait for SDK-defined tools.
///
/// Implement this trait to define a custom tool that can be registered
/// with an agent at build time.
#[async_trait::async_trait]
pub trait AgentTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: Value) -> SdkResult<Value>;
}

/// Trait for SDK-defined skills.
///
/// Skills compose multiple tools into reusable workflows.
#[async_trait::async_trait]
pub trait AgentSkill: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn required_tools(&self) -> Vec<String>;
    async fn run(&self, context: Value) -> SdkResult<Value>;
}

/// Trait for SDK-defined plugins.
///
/// A plugin packs one or more tools and skills together.
pub trait AgentPlugin: Send + Sync {
    fn manifest(&self) -> PluginManifest;
    fn tools(&self) -> Vec<Box<dyn AgentTool>>;
    fn skills(&self) -> Vec<Box<dyn AgentSkill>>;
}

// ── Plugin Manifest ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl PluginManifest {
    pub fn from_yaml(value: &str) -> SdkResult<Self> {
        let manifest: Self = serde_yaml::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_json(value: &str) -> SdkResult<Self> {
        let manifest: Self = serde_json::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> SdkResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(SdkError::Validation("plugin name is invalid".into()));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(SdkError::Validation("plugin version is invalid".into()));
        }
        if self.author.trim().is_empty() || self.author.len() > 256 {
            return Err(SdkError::Validation("plugin author is invalid".into()));
        }
        Ok(())
    }
}

// ── Agent Manifest ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl AgentManifest {
    pub fn from_yaml(value: &str) -> SdkResult<Self> {
        let manifest: Self = serde_yaml::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_json(value: &str) -> SdkResult<Self> {
        let manifest: Self = serde_json::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> SdkResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(SdkError::Validation("agent name must be 1..=256 characters".into()));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(SdkError::Validation("agent version must be 1..=64 characters".into()));
        }
        if self.tools.len() > 256 {
            return Err(SdkError::Validation("too many tools (max 256)".into()));
        }
        if self.skills.len() > 256 {
            return Err(SdkError::Validation("too many skills (max 256)".into()));
        }
        Ok(())
    }
}

// ── Publish Request ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    pub manifest: AgentManifest,
    pub content: Value,
    pub checksum_sha256: String,
    pub signing_key_id: String,
}

impl PublishRequest {
    pub fn new(manifest: AgentManifest, content: Value) -> Self {
        Self {
            manifest,
            content,
            checksum_sha256: "0".repeat(64),
            signing_key_id: "unsigned-local".into(),
        }
    }

    pub fn validate(&self) -> SdkResult<()> {
        self.manifest.validate()?;
        if self.checksum_sha256.len() != 64
            || !self.checksum_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(SdkError::Validation("checksum must be SHA-256 hex".into()));
        }
        if self.signing_key_id.trim().is_empty() || self.signing_key_id.len() > 128 {
            return Err(SdkError::Validation("signing key id is invalid".into()));
        }
        Ok(())
    }
}

// ── AgentIdentity ─────────────────────────────────────────────────────────

/// A unique agent identity within the ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub created_at: DateTime<Utc>,
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_agent_builder() {
        let builder = AgentBuilder::new("my-agent", "1.0.0")
            .model("gpt-4")
            .tool("code.search")
            .skill("java-analysis")
            .instructions("You are a Java code reviewer.");
        assert!(builder.validate().is_ok());
        assert_eq!(builder.name, "my-agent");
        assert_eq!(builder.tools.len(), 1);
        assert_eq!(builder.skills.len(), 1);
    }

    #[test]
    fn agent_builder_empty_name() {
        let builder = AgentBuilder::new("", "1.0.0");
        assert!(builder.validate().is_err());
    }

    #[test]
    fn agent_manifest_yaml_roundtrip() {
        let yaml = r#"
name: java-reviewer
version: "1.0.0"
description: Java code review agent
tools:
  - git.read
  - code.search
skills:
  - java-analysis
permissions:
  - repository.read
model: gpt-4
"#;
        let manifest = AgentManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "java-reviewer");
        assert_eq!(manifest.tools.len(), 2);
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn agent_manifest_validation() {
        let manifest = AgentManifest {
            name: String::new(),
            version: "1.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn plugin_manifest_yaml_roundtrip() {
        let yaml = r#"
name: my-plugin
version: "1.0.0"
description: My first plugin
author: core-agent
tools:
  - tool-a
  - tool-b
skills:
  - skill-a
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "my-plugin");
        assert_eq!(manifest.tools.len(), 2);
    }

    #[test]
    fn publish_request_validation() {
        let manifest = AgentManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };
        let req = PublishRequest::new(manifest, Value::Null);
        assert!(req.validate().is_ok());
    }

    #[test]
    fn chat_request_defaults() {
        let req = ChatRequest::new("Hello");
        assert_eq!(req.message, "Hello");
        assert!(!req.stream);
        assert!(req.session_id.is_none());
    }

    #[test]
    fn execution_status_serde() {
        let status = ExecutionStatus::Success;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"SUCCESS\"");
        let back: ExecutionStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ExecutionStatus::Success);
    }
}