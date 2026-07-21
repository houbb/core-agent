//! MCP server configuration discovery and McpServerConfig type.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_MCP_CONFIG_BYTES, MAX_MCP_SERVERS};
use crate::error::{McpRuntimeError, McpRuntimeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env_vars: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
}

fn default_true() -> bool {
    true
}

fn default_request_timeout_ms() -> u64 {
    crate::constants::DEFAULT_REQUEST_TIMEOUT_MS
}

impl McpServerConfig {
    /// Validate the server configuration bounds.
    pub fn validate(&self) -> McpRuntimeResult<()> {
        if self.name.trim().is_empty()
            || self.name.len() > 64
            || self.name.chars().any(char::is_control)
            || self.command.trim().is_empty()
            || self.command.len() > 4_096
            || self.command.contains('\0')
            || self.args.len() > 128
            || self
                .args
                .iter()
                .any(|arg| arg.len() > 4_096 || arg.contains('\0'))
            || self.env_vars.len() > 64
            || self.env_vars.iter().any(|name| {
                name.is_empty()
                    || name.len() > 128
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
            || !(1_000..=120_000).contains(&self.request_timeout_ms)
        {
            return Err(McpRuntimeError::Invalid(format!(
                "invalid MCP server configuration: {}",
                self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpConfigFile {
    version: u32,
    #[serde(default)]
    servers: Vec<McpServerConfig>,
}

/// Discover MCP server configurations from layered sources.
///
/// Order: global directory (`<global_dir>/mcp.json`) → project directory
/// (`.core-agent/mcp.json`). Project-level config overrides global for
/// servers with the same name.
///
/// Requires `CORE_AGENT_ENABLE_MCP=1` environment variable to be set.
pub fn discover_mcp_servers(
    workspace: &Path,
    global_directory: Option<&Path>,
) -> McpRuntimeResult<Vec<McpServerConfig>> {
    if std::env::var("CORE_AGENT_ENABLE_MCP").as_deref() != Ok("1") {
        return Ok(Vec::new());
    }
    let workspace = std::fs::canonicalize(workspace)?;
    let mut merged = BTreeMap::new();
    if let Some(directory) = global_directory {
        let path = directory.join("mcp.json");
        if path.exists() {
            for server in read_config(&path, None)? {
                merged.insert(server.name.clone(), server);
            }
        }
    }
    let project_path = workspace.join(".core-agent").join("mcp.json");
    if project_path.exists() {
        for server in read_config(&project_path, Some(&workspace))? {
            merged.insert(server.name.clone(), server);
        }
    }
    let servers = merged
        .into_values()
        .filter(|server| server.enabled)
        .collect::<Vec<_>>();
    if servers.len() > MAX_MCP_SERVERS {
        return Err(McpRuntimeError::Invalid(
            "MCP configuration exceeds 32 enabled servers".into(),
        ));
    }
    Ok(servers)
}

fn read_config(
    path: &Path,
    required_root: Option<&Path>,
) -> McpRuntimeResult<Vec<McpServerConfig>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MCP_CONFIG_BYTES
    {
        return Err(McpRuntimeError::Invalid(format!(
            "{} must be a regular file no larger than 64 KiB",
            path.display()
        )));
    }
    let canonical = std::fs::canonicalize(path)?;
    if required_root.is_some_and(|root| !canonical.starts_with(root)) {
        return Err(McpRuntimeError::Invalid(
            "project MCP configuration escaped the workspace".into(),
        ));
    }
    let file: McpConfigFile = serde_json::from_slice(&std::fs::read(canonical)?)?;
    if file.version != 1 {
        return Err(McpRuntimeError::Invalid(
            "MCP configuration version must be 1".into(),
        ));
    }
    for server in &file.servers {
        server.validate()?;
    }
    Ok(file.servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_requires_explicit_enablement() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join(".core-agent")).unwrap();
        std::fs::write(
            directory.path().join(".core-agent/mcp.json"),
            r#"{"version":1,"servers":[{"name":"x","command":"x"}]}"#,
        )
        .unwrap();
        std::env::remove_var("CORE_AGENT_ENABLE_MCP");
        assert!(discover_mcp_servers(directory.path(), None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn server_validation_rejects_unbounded_or_empty_processes() {
        let invalid = McpServerConfig {
            name: "test".into(),
            command: String::new(),
            args: Vec::new(),
            env_vars: Vec::new(),
            enabled: true,
            request_timeout_ms: 30_000,
        };
        assert!(invalid.validate().is_err());
    }
}