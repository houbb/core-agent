//! Developer Platform — domain types for agent development, testing, and publishing.
//!
//! Provides the data models for the Developer Portal including:
//! - AgentManifest — YAML-based agent definition
//! - DeveloperProject — a developer's project workspace
//! - AgentTestRun — test and evaluation results
//! - DeveloperDashboard — analytics snapshot

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{DeveloperError, DeveloperResult};

// ── Developer Profile ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperProfile {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub subject: String,
    pub display_name: String,
    pub email: String,
    pub api_keys: Vec<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DeveloperProfile {
    pub fn new(
        tenant_id: Uuid,
        subject: impl Into<String>,
        display_name: impl Into<String>,
        email: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            subject: subject.into(),
            display_name: display_name.into(),
            email: email.into(),
            api_keys: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> DeveloperResult<()> {
        if self.subject.trim().is_empty() || self.subject.len() > 256 {
            return Err(DeveloperError::Validation("subject is invalid".into()));
        }
        if self.display_name.trim().is_empty() || self.display_name.len() > 256 {
            return Err(DeveloperError::Validation(
                "display name is invalid".into(),
            ));
        }
        if self.email.trim().is_empty() || self.email.len() > 256 {
            return Err(DeveloperError::Validation("email is invalid".into()));
        }
        Ok(())
    }
}

// ── Agent Manifest ────────────────────────────────────────────────────────

/// Agent manifest — analogous to package.json for npm.
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
    pub fn from_yaml(value: &str) -> DeveloperResult<Self> {
        let manifest: Self = serde_yaml::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_json(value: &str) -> DeveloperResult<Self> {
        let manifest: Self = serde_json::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> DeveloperResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(DeveloperError::Validation(
                "agent name must be 1..=256 characters".into(),
            ));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(DeveloperError::Validation(
                "agent version must be 1..=64 characters".into(),
            ));
        }
        if self.tools.len() > 256 {
            return Err(DeveloperError::Validation("too many tools (max 256)".into()));
        }
        if self.skills.len() > 256 {
            return Err(DeveloperError::Validation("too many skills (max 256)".into()));
        }
        if self.permissions.len() > 64 {
            return Err(DeveloperError::Validation(
                "too many permissions (max 64)".into(),
            ));
        }
        Ok(())
    }
}

// ── Developer Project ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectState {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperProject {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub developer_id: Uuid,
    pub name: String,
    pub description: String,
    pub manifest: AgentManifest,
    pub state: ProjectState,
    pub version: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DeveloperProject {
    pub fn new(
        tenant_id: Uuid,
        developer_id: Uuid,
        name: impl Into<String>,
        manifest: AgentManifest,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            developer_id,
            name: name.into(),
            description: String::new(),
            manifest,
            state: ProjectState::Active,
            version: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> DeveloperResult<()> {
        self.manifest.validate()?;
        if self.name.trim().is_empty() || self.name.len() > 256 {
            return Err(DeveloperError::Validation(
                "project name must be 1..=256 characters".into(),
            ));
        }
        if self.version == 0 {
            return Err(DeveloperError::Validation("version must be >= 1".into()));
        }
        Ok(())
    }

    pub fn archive(&mut self, actor: &str) {
        self.state = ProjectState::Archived;
        self.version = self.version.saturating_add(1);
        self.actor = actor.into();
        self.updated_at = Utc::now();
    }
}

// ── Agent Test Run ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TestStatus {
    Passed,
    Failed,
    Error,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTestRun {
    pub id: Uuid,
    pub project_id: Uuid,
    pub input: String,
    pub expected_output: Option<String>,
    pub actual_output: String,
    pub status: TestStatus,
    pub score: Option<f64>,
    pub duration_ms: u64,
    pub error_message: Option<String>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
}

impl AgentTestRun {
    pub fn new(
        project_id: Uuid,
        input: impl Into<String>,
        actual_output: impl Into<String>,
        status: TestStatus,
        actor: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            input: input.into(),
            expected_output: None,
            actual_output: actual_output.into(),
            status,
            score: None,
            duration_ms: 0,
            error_message: None,
            actor: actor.into(),
            created_at: Utc::now(),
        }
    }
}

// ── Developer Dashboard ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperDashboard {
    pub total_projects: u64,
    pub published_agents: u64,
    pub total_downloads: u64,
    pub average_rating: f64,
    pub recent_runs: Vec<AgentTestRun>,
    pub api_keys_count: u64,
}

// ── Publish Request ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishRequest {
    pub project_id: Uuid,
    pub manifest: AgentManifest,
    pub content: Value,
    pub checksum_sha256: String,
    pub signing_key_id: String,
}

impl PublishRequest {
    pub fn new(
        project_id: Uuid,
        manifest: AgentManifest,
        content: Value,
    ) -> Self {
        Self {
            project_id,
            manifest,
            content,
            checksum_sha256: "0".repeat(64),
            signing_key_id: "unsigned-local".into(),
        }
    }

    pub fn validate(&self) -> DeveloperResult<()> {
        self.manifest.validate()?;
        if self.checksum_sha256.len() != 64
            || !self.checksum_sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(DeveloperError::Validation(
                "checksum must be SHA-256 hex".into(),
            ));
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_agent_manifest() {
        let manifest = AgentManifest {
            name: "java-reviewer".into(),
            version: "1.0.0".into(),
            description: "Java code review agent".into(),
            tools: vec!["git.read".into(), "code.search".into()],
            skills: vec!["java-analysis".into()],
            permissions: vec!["repository.read".into()],
            model: "gpt-4".into(),
            instructions: "Review Java code".into(),
            metadata: BTreeMap::new(),
        };
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn empty_manifest_name() {
        let manifest = AgentManifest {
            name: "".into(),
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
    fn manifest_yaml_parse() {
        let yaml = r#"
name: test-agent
version: "1.0.0"
description: A test agent
tools:
  - tool-a
skills:
  - skill-a
permissions:
  - permission-a
model: gpt-4
"#;
        let manifest = AgentManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "test-agent");
        assert_eq!(manifest.tools.len(), 1);
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn valid_developer_profile() {
        let profile = DeveloperProfile::new(Uuid::new_v4(), "alice", "Alice", "alice@example.com");
        assert!(profile.validate().is_ok());
    }

    #[test]
    fn empty_profile_subject() {
        let profile = DeveloperProfile::new(Uuid::new_v4(), "", "Alice", "alice@example.com");
        assert!(profile.validate().is_err());
    }

    #[test]
    fn valid_project() {
        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };
        let project = DeveloperProject::new(Uuid::new_v4(), Uuid::new_v4(), "my-project", manifest, "alice");
        assert!(project.validate().is_ok());
    }

    #[test]
    fn project_archive() {
        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };
        let mut project = DeveloperProject::new(Uuid::new_v4(), Uuid::new_v4(), "proj", manifest, "alice");
        assert_eq!(project.state, ProjectState::Active);
        project.archive("admin");
        assert_eq!(project.state, ProjectState::Archived);
        assert_eq!(project.version, 2);
    }

    #[test]
    fn test_run_creation() {
        let run = AgentTestRun::new(
            Uuid::new_v4(),
            "test input",
            "test output",
            TestStatus::Passed,
            "tester",
        );
        assert_eq!(run.status, TestStatus::Passed);
        assert!(run.score.is_none());
    }

    #[test]
    fn publish_request_validation() {
        let manifest = AgentManifest {
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "".into(),
            tools: vec![],
            skills: vec![],
            permissions: vec![],
            model: "default".into(),
            instructions: "".into(),
            metadata: BTreeMap::new(),
        };
        let req = PublishRequest::new(Uuid::new_v4(), manifest, Value::Null);
        assert!(req.validate().is_ok());
    }
}