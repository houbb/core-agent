use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CommandSandboxPolicy, SandboxNetworkPolicy, SandboxRequirement, ToolDefinition};

const MAX_MANAGED_POLICY_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManagedAgentPolicy {
    pub version: u32,
    pub allowed_tools: BTreeSet<String>,
    pub denied_tools: BTreeSet<String>,
    pub allowed_categories: BTreeSet<String>,
    pub denied_categories: BTreeSet<String>,
    pub allowed_mcp_servers: BTreeSet<String>,
    pub hooks_enabled: bool,
    pub mcp_enabled: bool,
    pub web_search_enabled: bool,
    pub web_allowed_domains: BTreeSet<String>,
    pub web_blocked_domains: BTreeSet<String>,
    pub memory_enabled: bool,
    pub memory_writes_enabled: bool,
    pub sandbox_requirement: SandboxRequirement,
    pub sandbox_network: SandboxNetworkPolicy,
}

impl Default for ManagedAgentPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            allowed_tools: BTreeSet::new(),
            denied_tools: BTreeSet::new(),
            allowed_categories: BTreeSet::new(),
            denied_categories: BTreeSet::new(),
            allowed_mcp_servers: BTreeSet::new(),
            hooks_enabled: true,
            mcp_enabled: true,
            web_search_enabled: true,
            web_allowed_domains: BTreeSet::new(),
            web_blocked_domains: BTreeSet::new(),
            memory_enabled: true,
            memory_writes_enabled: true,
            sandbox_requirement: SandboxRequirement::BestEffort,
            sandbox_network: SandboxNetworkPolicy::Deny,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedPolicyDecision {
    Allow,
    Deny,
}

#[derive(Debug, thiserror::Error)]
pub enum ManagedPolicyError {
    #[error("managed policy I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("managed policy is invalid: {0}")]
    Invalid(String),
    #[error("managed policy serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type ManagedPolicyResult<T> = Result<T, ManagedPolicyError>;

impl ManagedAgentPolicy {
    pub fn load_from_environment() -> ManagedPolicyResult<Option<(Self, PathBuf)>> {
        let Some(path) = std::env::var_os("CORE_AGENT_MANAGED_POLICY").map(PathBuf::from) else {
            return Ok(None);
        };
        if !path.is_absolute() {
            return Err(ManagedPolicyError::Invalid(
                "CORE_AGENT_MANAGED_POLICY must be an absolute path".into(),
            ));
        }
        let policy = Self::load(&path)?;
        Ok(Some((policy, std::fs::canonicalize(path)?)))
    }

    pub fn load(path: &Path) -> ManagedPolicyResult<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() > MAX_MANAGED_POLICY_BYTES
        {
            return Err(ManagedPolicyError::Invalid(
                "policy must be a regular non-symlink file no larger than 256 KiB".into(),
            ));
        }
        let policy: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> ManagedPolicyResult<()> {
        if self.version != 1 {
            return Err(ManagedPolicyError::Invalid(
                "policy version must be 1".into(),
            ));
        }
        for (label, values) in [
            ("allowedTools", &self.allowed_tools),
            ("deniedTools", &self.denied_tools),
            ("allowedCategories", &self.allowed_categories),
            ("deniedCategories", &self.denied_categories),
            ("allowedMcpServers", &self.allowed_mcp_servers),
            ("webAllowedDomains", &self.web_allowed_domains),
            ("webBlockedDomains", &self.web_blocked_domains),
        ] {
            if values.len() > 1_024
                || values.iter().any(|value| {
                    value.trim().is_empty()
                        || value.len() > 386
                        || value.chars().any(char::is_control)
                })
            {
                return Err(ManagedPolicyError::Invalid(format!(
                    "{label} contains too many or invalid values"
                )));
            }
        }
        Ok(())
    }

    pub fn evaluate_tool(&self, tool: &ToolDefinition) -> ManagedPolicyDecision {
        if self.denied_tools.contains(&tool.name)
            || self.denied_tools.contains(&tool.key)
            || self.denied_categories.contains(&tool.category)
            || (!self.allowed_tools.is_empty()
                && !self.allowed_tools.contains(&tool.name)
                && !self.allowed_tools.contains(&tool.key))
            || (!self.allowed_categories.is_empty()
                && !self.allowed_categories.contains(&tool.category))
            || (!self.web_search_enabled && tool.category == "network.read")
            || (!self.memory_enabled && tool.category.starts_with("memory."))
            || (!self.memory_writes_enabled && tool.category == "memory.write")
            || (!self.mcp_enabled && tool.category == "mcp.remote")
        {
            ManagedPolicyDecision::Deny
        } else {
            ManagedPolicyDecision::Allow
        }
    }

    pub fn permits_mcp_server(&self, name: &str) -> bool {
        self.mcp_enabled
            && (self.allowed_mcp_servers.is_empty() || self.allowed_mcp_servers.contains(name))
    }

    pub fn command_sandbox_policy(&self) -> CommandSandboxPolicy {
        CommandSandboxPolicy {
            requirement: self.sandbox_requirement,
            network: self.sandbox_network,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, category: &str) -> ToolDefinition {
        let mut tool = ToolDefinition::new("workspace", name, "1.0.0", json!({"type": "object"}));
        tool.category = category.into();
        tool
    }

    #[test]
    fn deny_rules_override_allow_rules_and_feature_switches() {
        let mut policy = ManagedAgentPolicy::default();
        policy.allowed_categories.insert("filesystem.read".into());
        policy.denied_tools.insert("read_file".into());
        assert_eq!(
            policy.evaluate_tool(&tool("read_file", "filesystem.read")),
            ManagedPolicyDecision::Deny
        );
        assert_eq!(
            policy.evaluate_tool(&tool("find_files", "filesystem.read")),
            ManagedPolicyDecision::Allow
        );
        policy.web_search_enabled = false;
        assert_eq!(
            policy.evaluate_tool(&tool("web_search", "network.read")),
            ManagedPolicyDecision::Deny
        );
    }

    #[test]
    fn managed_file_rejects_symlink_indirection_on_supported_platforms() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("policy.json");
        std::fs::write(&file, r#"{"version":1,"memoryEnabled":false}"#).unwrap();
        let policy = ManagedAgentPolicy::load(&file).unwrap();
        assert!(!policy.memory_enabled);
    }
}
