use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{ToolDefinition, ToolProviderDefinition, ToolProviderKind};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolProvider, ToolRegistration};

/// Provider that discovers tools from user-defined directories.
///
/// Scans directories for `tool.yaml` manifest files, parses them, and
/// registers each tool as a `CommandTool` that executes a shell command.
pub struct UserToolProvider {
    definition: ToolProviderDefinition,
    registrations: Vec<ToolRegistration>,
}

impl UserToolProvider {
    /// Discover tools from a list of root directories.
    ///
    /// Each root is scanned for subdirectories containing a `tool.yaml` file.
    /// If a tool has the same key as a previously registered tool, the later
    /// one wins (caller should order roots by precedence).
    pub fn discover(directories: &[&Path]) -> ToolRuntimeResult<Self> {
        let definition = ToolProviderDefinition::new(
            "user",
            "User Tools",
            ToolProviderKind::Plugin,
        );

        let mut seen: BTreeMap<String, ToolRegistration> = BTreeMap::new();

        for dir in directories {
            if !dir.exists() {
                continue;
            }
            let entries = match std::fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let meta = match std::fs::symlink_metadata(entry.path()) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    continue;
                }
                let manifest_path = entry.path().join("tool.yaml");
                if !manifest_path.exists() {
                    continue;
                }
                let tool_name = entry.file_name().to_string_lossy().to_string();
                match Self::load_tool(&definition, &tool_name, &manifest_path) {
                    Ok(registration) => {
                        seen.insert(registration.definition.key.clone(), registration);
                    }
                    Err(e) => {
                        // Log but continue — don't let one bad tool poison the whole provider.
                        eprintln!(
                            "[core-agent-tool] warning: failed to load tool '{}' from {}: {}",
                            tool_name,
                            manifest_path.display(),
                            e
                        );
                    }
                }
            }
        }

        let registrations: Vec<_> = seen.into_values().collect();
        Ok(Self {
            definition,
            registrations,
        })
    }

    fn load_tool(
        provider: &ToolProviderDefinition,
        tool_name: &str,
        manifest_path: &Path,
    ) -> ToolRuntimeResult<ToolRegistration> {
        let content = std::fs::read_to_string(manifest_path)?;
        let manifest: UserToolManifest = serde_yaml::from_str(&content)?;
        manifest.validate()?;

        let key = format!("{}/{}@{}", provider.key, manifest.name, manifest.version);
        let mut definition = ToolDefinition::new(
            &provider.key,
            &manifest.name,
            &manifest.version,
            manifest.input_schema.clone(),
        );
        definition.key = key.clone();
        definition.description = manifest.description.clone();
        definition.category = manifest.category.unwrap_or_else(|| "user".into());
        if let Some(timeout) = manifest.timeout_ms {
            definition.timeout_ms = timeout;
        }

        let tool = Arc::new(CommandTool {
            key,
            command: manifest.runtime.command,
            args: manifest.runtime.args.unwrap_or_default(),
        });

        Ok(ToolRegistration::new(definition, tool))
    }
}

#[async_trait]
impl ToolProvider for UserToolProvider {
    fn definition(&self) -> ToolProviderDefinition {
        self.definition.clone()
    }

    async fn discover(&self) -> ToolRuntimeResult<Vec<ToolRegistration>> {
        Ok(self.registrations.clone())
    }
}

/// A tool that executes a shell command with the given arguments.
struct CommandTool {
    key: String,
    command: String,
    args: Vec<String>,
}

#[async_trait]
impl Tool for CommandTool {
    fn key(&self) -> &str {
        &self.key
    }

    async fn execute(
        &self,
        request: &crate::domain::ToolRequest,
        _context: &crate::infrastructure::ToolContext,
    ) -> ToolRuntimeResult<crate::domain::RawToolOutput> {
        // Serialize the parameters as JSON and pass via stdin.
        let input = serde_json::to_string(&request.parameters)
            .map_err(|e| ToolError::execution("user", e, false))?;

        let mut cmd = tokio::process::Command::new(&self.command);
        cmd.args(&self.args)
            .arg("--input")
            .arg(&input)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let output = cmd
            .output()
            .await
            .map_err(|e| ToolError::execution("user", format!("command failed: {e}"), false))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::execution(
                "user",
                format!("command exited with {}: {}", output.status, stderr),
                false,
            ));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| ToolError::execution("user", format!("invalid UTF-8 output: {e}"), false))?;

        Ok(crate::domain::RawToolOutput::text(stdout))
    }
}

/// YAML manifest structure for user-defined tools.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UserToolManifest {
    name: String,
    version: String,
    description: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    input_schema: serde_json::Value,
    runtime: UserToolRuntime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct UserToolRuntime {
    command: String,
    #[serde(default)]
    args: Option<Vec<String>>,
}

impl UserToolManifest {
    fn validate(&self) -> ToolRuntimeResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(ToolError::InvalidArgument(
                "tool name must be 1..128 characters".into(),
            ));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(ToolError::InvalidArgument(
                "tool version must be 1..64 characters".into(),
            ));
        }
        if self.description.trim().is_empty() || self.description.len() > 4096 {
            return Err(ToolError::InvalidArgument(
                "tool description must be 1..4096 characters".into(),
            ));
        }
        if !self.input_schema.is_object() {
            return Err(ToolError::InvalidArgument(
                "input_schema must be a JSON object".into(),
            ));
        }
        if self.runtime.command.trim().is_empty() || self.runtime.command.len() > 4096 {
            return Err(ToolError::InvalidArgument(
                "runtime command must be 1..4096 characters".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_tool_provider_discovers_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let tools_dir = tmp.path().join("tools");

        // Create a tool definition
        let tool_dir = tools_dir.join("my-tool");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::write(
            tool_dir.join("tool.yaml"),
            r#"
name: my-tool
version: "1.0.0"
description: My custom tool
input_schema:
  type: object
  required: [input]
  properties:
    input:
      type: string
      description: Input text
runtime:
  command: echo
  args: ["--tool"]
"#,
        )
        .unwrap();

        let provider = UserToolProvider::discover(&[&tools_dir]).unwrap();
        let registrations = provider.registrations;
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].definition.name, "my-tool");
        assert!(registrations[0].definition.key.contains("user/my-tool"));
    }

    #[test]
    fn user_tool_provider_skips_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let tools_dir = tmp.path().join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        // Create a directory without tool.yaml
        std::fs::create_dir_all(tools_dir.join("empty-dir")).unwrap();

        let provider = UserToolProvider::discover(&[&tools_dir]).unwrap();
        assert!(provider.registrations.is_empty());
    }

    #[test]
    fn user_tool_provider_handles_multiple_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");
        let dir_b = tmp.path().join("b");

        // Tool in dir_a
        std::fs::create_dir_all(dir_a.join("tool-a")).unwrap();
        std::fs::write(
            dir_a.join("tool-a").join("tool.yaml"),
            r#"
name: tool-a
version: "1.0.0"
description: Tool A
input_schema:
  type: object
  properties: {}
runtime:
  command: echo
"#,
        )
        .unwrap();

        // Tool in dir_b
        std::fs::create_dir_all(dir_b.join("tool-b")).unwrap();
        std::fs::write(
            dir_b.join("tool-b").join("tool.yaml"),
            r#"
name: tool-b
version: "1.0.0"
description: Tool B
input_schema:
  type: object
  properties: {}
runtime:
  command: cat
"#,
        )
        .unwrap();

        let provider = UserToolProvider::discover(&[&dir_a, &dir_b]).unwrap();
        assert_eq!(provider.registrations.len(), 2);
    }

    #[test]
    fn user_tool_manifest_validates_name() {
        let manifest = UserToolManifest {
            name: String::new(),
            version: "1.0.0".into(),
            description: "desc".into(),
            category: None,
            timeout_ms: None,
            input_schema: serde_json::json!({"type": "object"}),
            runtime: UserToolRuntime {
                command: "echo".into(),
                args: None,
            },
        };
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn user_tool_manifest_validates_schema() {
        let manifest = UserToolManifest {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "desc".into(),
            category: None,
            timeout_ms: None,
            input_schema: serde_json::json!("string"), // not an object
            runtime: UserToolRuntime {
                command: "echo".into(),
                args: None,
            },
        };
        assert!(manifest.validate().is_err());
    }
}