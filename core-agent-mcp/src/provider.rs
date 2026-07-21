//! MCP Tool Provider — wraps MCP client as a ToolProvider for the tool runtime.
//!
//! Discovers remote tools via `tools/list` and translates `tools/call` into
//! ToolRuntime-compatible execution.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use core_agent_tool::{
    FunctionTool, PermissionDecision, RawToolOutput, ToolContent, ToolDefinition,
    ToolError, ToolProvider, ToolProviderDefinition, ToolProviderKind, ToolRegistration,
    ToolRuntimeResult,
};

use crate::client::{safe_identity, McpClient};
use crate::error::{McpRuntimeError, McpRuntimeResult};
use crate::McpServerConfig;

/// A tool provider backed by an MCP server.
pub struct McpToolProvider {
    definition: ToolProviderDefinition,
    client: Arc<McpClient>,
}

const DEFAULT_MCP_METADATA_BUDGET_BYTES: usize = 4 * 1024;

impl McpToolProvider {
    /// Connect to an MCP server and create a provider.
    pub async fn connect(config: &McpServerConfig, workspace: &Path) -> McpRuntimeResult<Self> {
        let client = McpClient::connect(config, workspace).await?;
        let key = format!("mcp-{}", safe_identity(&config.name));
        Ok(Self {
            definition: ToolProviderDefinition::new(
                key,
                format!("MCP {}", config.name),
                ToolProviderKind::Mcp,
            ),
            client,
        })
    }

    /// The underlying MCP client reference.
    pub fn client(&self) -> &Arc<McpClient> {
        &self.client
    }

    /// Generate a compact, budget-bounded metadata prompt for MCP tools.
    ///
    /// Progressive disclosure: only tool names and descriptions are included,
    /// not full JSON Schemas. The LLM must call the specific tool to access
    /// its full definition and execute it.
    pub async fn metadata_prompt(&self, max_bytes: usize) -> McpRuntimeResult<String> {
        let tools = self.client.list_tools().await?;
        let server_name = safe_identity(self.client.server_name());
        let mut output = String::new();
        let mut omitted = 0_usize;
        for tool in &tools {
            let name = tool.get("name").and_then(Value::as_str).unwrap_or("?");
            let desc = tool.get("description").and_then(Value::as_str).unwrap_or("");
            let line = format!("- mcp_{server_name}_{}: {}", safe_identity(name), desc);
            let required = line.len() + usize::from(!output.is_empty());
            if output.len().saturating_add(required) > max_bytes {
                omitted += 1;
                continue;
            }
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&line);
        }
        if omitted > 0 {
            let marker = format!("\n... {omitted} more MCP tool(s) omitted by budget");
            if output.len().saturating_add(marker.len()) <= max_bytes {
                output.push_str(&marker);
            }
        }
        Ok(output)
    }
}

#[async_trait]
impl ToolProvider for McpToolProvider {
    fn definition(&self) -> ToolProviderDefinition {
        self.definition.clone()
    }

    async fn discover(&self) -> ToolRuntimeResult<Vec<ToolRegistration>> {
        let tools = self.client.list_tools().await.map_err(mcp_tool_error)?;
        let mut registrations = Vec::new();
        let mut visible_names = std::collections::BTreeSet::new();
        for advertised in tools {
            let remote_name = advertised
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Validation("MCP tool has no name".into()))?
                .to_owned();
            let visible_name = format!(
                "mcp_{}_{}",
                safe_identity(self.client.server_name()),
                safe_identity(&remote_name)
            );
            if !visible_names.insert(visible_name.clone()) {
                return Err(ToolError::Validation(format!(
                    "MCP tool name collision after normalization: {remote_name}"
                )));
            }
            let input_schema = advertised
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            let mut definition = ToolDefinition::new(
                self.definition.key.clone(),
                visible_name,
                "1.0.0",
                input_schema,
            );
            definition.description = advertised
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Tool provided by an explicitly enabled MCP server")
                .to_owned();
            definition.category = "mcp.remote".into();
            definition.default_permission = PermissionDecision::Ask;
            definition.timeout_ms = self.client.request_timeout_ms() + 1_000;
            let key = definition.key.clone();
            let client = self.client.clone();
            let tool = Arc::new(FunctionTool::new(key, move |request, context| {
                let client = client.clone();
                let remote_name = remote_name.clone();
                async move {
                    let result = client
                        .request(
                            "tools/call",
                            json!({"name": remote_name, "arguments": request.parameters}),
                            context.cancellation,
                        )
                        .await
                        .map_err(mcp_tool_error)?;
                    if result.get("isError").and_then(Value::as_bool) == Some(true) {
                        return Err(ToolError::execution("mcp_tool", result.to_string(), false));
                    }
                    Ok(RawToolOutput {
                        content: vec![ToolContent::Json(result)],
                        ..RawToolOutput::default()
                    })
                }
            }));
            registrations.push(ToolRegistration::new(definition, tool));
        }
        Ok(registrations)
    }
}

fn mcp_tool_error(error: McpRuntimeError) -> ToolError {
    match error {
        McpRuntimeError::Invalid(msg) => ToolError::Validation(msg),
        McpRuntimeError::Cancelled(server) => ToolError::Cancelled(server),
        McpRuntimeError::Timeout(server) => ToolError::Timeout {
            tool: format!("mcp:{server}"),
            timeout_ms: 0,
        },
        error => ToolError::execution("mcp", error.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_normalization_is_stable_and_bounded() {
        assert_eq!(safe_identity("GitHub Tools"), "github_tools");
        assert_eq!(safe_identity("///"), "unnamed");
        assert!(safe_identity(&"x".repeat(200)).len() <= 48);
    }
}