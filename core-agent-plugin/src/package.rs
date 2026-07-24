//! PluginPackage — ZIP-based plugin packaging format for core-agent.
//!
//! A PluginPackage bundles a PluginManifest with tool definitions, skill
//! definitions, and agent definitions into a single `.zip` archive for
//! distribution and installation.
//!
//! Package layout:
//!
//! ```text
//! my-plugin-v1.0.0.zip
//! ├── manifest.json          # required: PluginManifest JSON
//! ├── tools/                 # optional: tool definitions
//! │   ├── tool-a.json
//! │   └── tool-b.json
//! ├── skills/                # optional: skill definitions
//! │   └── my-skill/
//! │       └── SKILL.md
//! └── agents/                # optional: agent definitions
//!     └── my-agent.json
//! ```

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{PluginManifest, PluginResult};
use crate::error::PluginError;

/// Maximum size for a single file inside a plugin package.
const MAX_FILE_BYTES: usize = 16 * 1024 * 1024; // 16 MiB
/// Maximum number of entries in a plugin package.
const MAX_ENTRIES: usize = 1024;
/// Maximum total size of a plugin package.
const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024; // 64 MiB

/// A definition for an agent inside a plugin package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl AgentDefinition {
    pub fn validate(&self) -> PluginResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(PluginError::Validation("agent name is invalid".into()));
        }
        if self.description.trim().is_empty() || self.description.len() > 2048 {
            return Err(PluginError::Validation(
                "agent description is invalid".into(),
            ));
        }
        Ok(())
    }
}

/// A parsed plugin package, read from a ZIP archive.
#[derive(Debug, Clone)]
pub struct PluginPackage {
    pub manifest: PluginManifest,
    pub tools: Vec<serde_json::Value>,
    pub skills: Vec<(String, String)>,
    pub agents: Vec<AgentDefinition>,
    pub checksum_sha256: String,
    pub bytes: usize,
    pub created_at: DateTime<Utc>,
}

impl PluginPackage {
    /// Validate the package manifest and all contained definitions.
    pub fn validate(&self) -> PluginResult<()> {
        self.manifest.validate()?;
        for tool in &self.tools {
            let name = tool
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            if !tool.is_object() {
                return Err(PluginError::Validation(format!("tool {name} is not a JSON object")));
            }
            if tool.get("key").and_then(|v| v.as_str()).unwrap_or("").trim().is_empty() {
                return Err(PluginError::Validation(format!("tool {name} is missing a key")));
            }
        }
        for (skill_name, _) in &self.skills {
            if skill_name.trim().is_empty() || skill_name.len() > 128 {
                return Err(PluginError::Validation(format!(
                    "skill name {skill_name} is invalid"
                )));
            }
        }
        for agent in &self.agents {
            agent.validate()?;
        }
        // Verify manifest tool/skill/agent references match what's in the package
        for tool_key in &self.manifest.tools {
            let found = self.tools.iter().any(|t| {
                t.get("key")
                    .and_then(|v| v.as_str())
                    .map(|k| k == tool_key)
                    .unwrap_or(false)
            });
            if !found {
                return Err(PluginError::Validation(format!(
                    "manifest references tool '{tool_key}' but it is not in the package"
                )));
            }
        }
        for skill_name in &self.manifest.skills {
            if !self.skills.iter().any(|(n, _)| n == skill_name) {
                return Err(PluginError::Validation(format!(
                    "manifest references skill '{skill_name}' but it is not in the package"
                )));
            }
        }
        for agent_name in &self.manifest.agents {
            if !self.agents.iter().any(|a| &a.name == agent_name) {
                return Err(PluginError::Validation(format!(
                    "manifest references agent '{agent_name}' but it is not in the package"
                )));
            }
        }
        Ok(())
    }
}

/// Builder for creating PluginPackage ZIP archives.
#[derive(Debug, Default)]
pub struct PluginPackageBuilder {
    manifest: Option<PluginManifest>,
    tools: Vec<serde_json::Value>,
    skills: Vec<(String, String)>,
    agents: Vec<AgentDefinition>,
}

impl PluginPackageBuilder {
    pub fn new(manifest: PluginManifest) -> Self {
        Self {
            manifest: Some(manifest),
            ..Self::default()
        }
    }

    pub fn add_tool(mut self, definition: serde_json::Value) -> PluginResult<Self> {
        if !definition.is_object() {
            return Err(PluginError::Validation("tool definition must be a JSON object".into()));
        }
        self.tools.push(definition);
        Ok(self)
    }

    pub fn add_skill(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.skills.push((name.into(), content.into()));
        self
    }

    pub fn add_agent(mut self, definition: AgentDefinition) -> PluginResult<Self> {
        definition.validate()?;
        self.agents.push(definition);
        Ok(self)
    }

    /// Build the package and produce a ZIP archive as bytes.
    pub fn build_zip(&self) -> PluginResult<Vec<u8>> {
        let manifest = self
            .manifest
            .as_ref()
            .ok_or_else(|| PluginError::Validation("manifest is required".into()))?;
        manifest.validate()?;

        let mut buffer = Cursor::new(Vec::new());
        let mut zip_writer = zip::ZipWriter::new(&mut buffer);

        let options = zip::write::FileOptions::<()>::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        // Write manifest.json
        let manifest_json = serde_json::to_string_pretty(manifest)?;
        zip_writer
            .start_file("manifest.json", options)
            .map_err(|e| PluginError::Package(e.to_string()))?;
        zip_writer
            .write_all(manifest_json.as_bytes())
            .map_err(|e| PluginError::Package(e.to_string()))?;

        // Write tools/
        for tool in &self.tools {
            let key = tool
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tool_json = serde_json::to_string_pretty(tool)?;
            zip_writer
                .start_file(format!("tools/{}.json", key), options)
                .map_err(|e| PluginError::Package(e.to_string()))?;
            zip_writer
                .write_all(tool_json.as_bytes())
                .map_err(|e| PluginError::Package(e.to_string()))?;
        }

        // Write skills/
        for (skill_name, skill_content) in &self.skills {
            zip_writer
                .start_file(
                    format!("skills/{}/SKILL.md", skill_name),
                    options,
                )
                .map_err(|e| PluginError::Package(e.to_string()))?;
            zip_writer
                .write_all(skill_content.as_bytes())
                .map_err(|e| PluginError::Package(e.to_string()))?;
        }

        // Write agents/
        for agent in &self.agents {
            let agent_json = serde_json::to_string_pretty(agent)?;
            zip_writer
                .start_file(format!("agents/{}.json", agent.name), options)
                .map_err(|e| PluginError::Package(e.to_string()))?;
            zip_writer
                .write_all(agent_json.as_bytes())
                .map_err(|e| PluginError::Package(e.to_string()))?;
        }

        zip_writer
            .finish()
            .map_err(|e| PluginError::Package(e.to_string()))?;

        let bytes = buffer.into_inner();
        Ok(bytes)
    }

    /// Write the package as a .zip file to disk.
    pub fn write_zip(&self, path: &Path) -> PluginResult<()> {
        let bytes = self.build_zip()?;
        std::fs::write(path, &bytes)?;

        // Verify the written file
        let written = std::fs::read(path)?;
        let checksum = format!("{:x}", Sha256::digest(&written));
        log_checksum(&path, &checksum);

        Ok(())
    }
}

/// Reader for parsing PluginPackage from ZIP archives.
pub struct PluginPackageReader;

impl PluginPackageReader {
    /// Read a plugin package from a .zip file on disk.
    pub fn from_zip(path: &Path) -> PluginResult<PluginPackage> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Read a plugin package from memory bytes.
    pub fn from_bytes(bytes: &[u8]) -> PluginResult<PluginPackage> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err(PluginError::LimitExceeded {
                kind: "plugin package".into(),
                limit: MAX_PACKAGE_BYTES,
            });
        }

        let checksum = format!("{:x}", Sha256::digest(bytes));
        let package_bytes = bytes.len();

        let cursor = Cursor::new(bytes.to_vec());
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| PluginError::Package(format!("invalid ZIP archive: {e}")))?;

        if archive.len() > MAX_ENTRIES {
            return Err(PluginError::LimitExceeded {
                kind: "plugin entries".into(),
                limit: MAX_ENTRIES,
            });
        }

        let mut manifest: Option<PluginManifest> = None;
        let mut tools: Vec<serde_json::Value> = Vec::new();
        let mut skills: Vec<(String, String)> = Vec::new();
        let mut agents: Vec<AgentDefinition> = Vec::new();

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| PluginError::Package(format!("corrupt entry #{i}: {e}")))?;

            let entry_path = entry.name().to_owned();
            let entry_size = entry.size() as usize;

            if entry_size > MAX_FILE_BYTES {
                return Err(PluginError::LimitExceeded {
                    kind: format!("file size in plugin: {entry_path}"),
                    limit: MAX_FILE_BYTES,
                });
            }

            let mut content = Vec::with_capacity(entry_size);
            entry
                .read_to_end(&mut content)
                .map_err(|e| PluginError::Package(format!("failed to read {entry_path}: {e}")))?;

            match entry_path.as_str() {
                "manifest.json" => {
                    let m: PluginManifest = serde_json::from_slice(&content)?;
                    m.validate()?;
                    manifest = Some(m);
                }
                path if path.starts_with("tools/") && path.ends_with(".json") => {
                    let tool: serde_json::Value = serde_json::from_slice(&content)?;
                    if !tool.is_object() {
                        return Err(PluginError::Validation(format!(
                            "tool definition in {path} is not a JSON object"
                        )));
                    }
                    tools.push(tool);
                }
                path if path.starts_with("skills/") && path.ends_with("/SKILL.md") => {
                    let skill_name = path
                        .strip_prefix("skills/")
                        .and_then(|p| p.strip_suffix("/SKILL.md"))
                        .unwrap_or("unknown");
                    let content_str = String::from_utf8(content).map_err(|_| {
                        PluginError::Validation(format!("skill {skill_name} is not valid UTF-8"))
                    })?;
                    skills.push((skill_name.to_owned(), content_str));
                }
                path if path.starts_with("agents/") && path.ends_with(".json") => {
                    let agent: AgentDefinition = serde_json::from_slice(&content)?;
                    agent.validate()?;
                    agents.push(agent);
                }
                _ => {
                    // Skip unknown files (e.g. __MACOSX, .DS_Store)
                }
            }
        }

        let manifest = manifest.ok_or_else(|| {
            PluginError::Validation("plugin package is missing manifest.json".into())
        })?;

        let package = PluginPackage {
            manifest,
            tools,
            skills,
            agents,
            checksum_sha256: checksum,
            bytes: package_bytes,
            created_at: Utc::now(),
        };

        package.validate()?;
        Ok(package)
    }
}

use std::io::Write;

fn log_checksum(path: &Path, checksum: &str) {
    #[cfg(debug_assertions)]
    eprintln!(
        "[plugin] wrote {} with sha256={}",
        path.display(),
        checksum
    );
    let _ = path;
    let _ = checksum;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: "Test plugin".into(),
            author: "tester".into(),
            tools: vec!["tool-a".into(), "tool-b".into()],
            skills: vec!["my-skill".into()],
            agents: Vec::new(),
            permissions: BTreeSet::new(),
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn package_round_trips_zip() {
        let manifest = sample_manifest();
        let builder = PluginPackageBuilder::new(manifest.clone())
            .add_tool(serde_json::json!({
                "key": "tool-a",
                "name": "Tool A",
                "description": "First tool",
                "input_schema": {"type": "object", "properties": {}},
                "version": "1.0.0"
            }))
            .unwrap()
            .add_tool(serde_json::json!({
                "key": "tool-b",
                "name": "Tool B",
                "description": "Second tool",
                "input_schema": {"type": "object", "properties": {}},
                "version": "1.0.0"
            }))
            .unwrap()
            .add_skill("my-skill", "---\nname: my-skill\ndescription: My skill\n---\n\nDo something");

        let zip_bytes = builder.build_zip().unwrap();
        assert!(!zip_bytes.is_empty());

        let package = PluginPackageReader::from_bytes(&zip_bytes).unwrap();
        assert_eq!(package.manifest.name, "test-plugin");
        assert_eq!(package.tools.len(), 2);
        assert_eq!(package.skills.len(), 1);
        assert_eq!(package.skills[0].0, "my-skill");
        assert_eq!(package.agents.len(), 0);
        assert!(!package.checksum_sha256.is_empty());
        assert!(package.bytes > 0);
    }

    #[test]
    fn package_rejects_missing_manifest() {
        let buffer = Cursor::new(Vec::new());
        let mut zip_writer = zip::ZipWriter::new(buffer);
        let options = zip::write::FileOptions::<()>::default();
        zip_writer
            .start_file("random.txt", options)
            .unwrap();
        zip_writer.write_all(b"hello").unwrap();
        let buffer = zip_writer.finish().unwrap().into_inner();

        let result = PluginPackageReader::from_bytes(&buffer);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("manifest.json") || err.contains("missing"));
    }

    #[test]
    fn package_rejects_unreferenced_tool() {
        let manifest = PluginManifest {
            tools: vec!["tool-a".into()],
            ..sample_manifest()
        };
        let builder = PluginPackageBuilder::new(manifest);
        // Add no tools even though manifest references tool-a
        let result = builder.build_zip();
        // Should fail at validation inside build_zip
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("tool-a"));
    }

    #[test]
    fn package_with_agents() {
        let manifest = PluginManifest {
            agents: vec!["reviewer".into()],
            ..sample_manifest()
        };
        let agent = AgentDefinition {
            name: "reviewer".into(),
            description: "Code reviewer agent".into(),
            model: Some("gpt-4".into()),
            tools: vec!["file.read".into()],
            skills: Vec::new(),
            metadata: BTreeMap::new(),
        };
        let builder = PluginPackageBuilder::new(manifest)
            .add_agent(agent)
            .unwrap();
        let zip_bytes = builder.build_zip().unwrap();
        let package = PluginPackageReader::from_bytes(&zip_bytes).unwrap();
        assert_eq!(package.agents.len(), 1);
        assert_eq!(package.agents[0].name, "reviewer");
    }

    #[test]
    fn package_rejects_invalid_agent() {
        let agent = AgentDefinition {
            name: String::new(),
            description: "test".into(),
            model: None,
            tools: Vec::new(),
            skills: Vec::new(),
            metadata: BTreeMap::new(),
        };
        assert!(agent.validate().is_err());
    }

    #[test]
    fn package_write_and_read_zip_file() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test-plugin-v1.0.0.zip");

        let manifest = sample_manifest();
        let builder = PluginPackageBuilder::new(manifest)
            .add_tool(serde_json::json!({
                "key": "tool-a",
                "name": "Tool A",
                "description": "First tool",
                "input_schema": {"type": "object", "properties": {}},
                "version": "1.0.0"
            }))
            .unwrap()
            .add_tool(serde_json::json!({
                "key": "tool-b",
                "name": "Tool B",
                "description": "Second tool",
                "input_schema": {"type": "object", "properties": {}},
                "version": "1.0.0"
            }))
            .unwrap()
            .add_skill("my-skill", "---\nname: my-skill\ndescription: My skill\n---\n\nDo something");

        builder.write_zip(&zip_path).unwrap();
        assert!(zip_path.exists());

        let package = PluginPackageReader::from_zip(&zip_path).unwrap();
        assert_eq!(package.manifest.name, "test-plugin");
        assert_eq!(package.tools.len(), 2);
    }
}