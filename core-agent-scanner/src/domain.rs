use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The scope/lifecycle of an extension root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionScope {
    /// System-wide extensions (e.g. /etc/core-agent/extensions/).
    System,
    /// User-level extensions (e.g. ~/.core-agent/<kind>/).
    User,
    /// Project-level extensions (e.g. .core-agent/<kind>/).
    Project,
    /// Session-scoped extensions (loaded at runtime, not persisted).
    Session,
}

impl ExtensionScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Project => "project",
            Self::Session => "session",
        }
    }
}

/// The type of extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Custom agent definitions (agent.yaml).
    Agent,
    /// Custom tool definitions (tool.yaml).
    Tool,
    /// Skill workflows (SKILL.md).
    Skill,
    /// MCP server configurations (server.yaml).
    Mcp,
}

impl ExtensionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agents",
            Self::Tool => "tools",
            Self::Skill => "skills",
            Self::Mcp => "mcp",
        }
    }

    /// The expected manifest file name for this extension kind.
    pub fn manifest_filename(self) -> &'static str {
        match self {
            Self::Agent => "agent.yaml",
            Self::Tool => "tool.yaml",
            Self::Skill => "SKILL.md",
            Self::Mcp => "server.yaml",
        }
    }
}

/// A configured root directory for scanning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionRoot {
    pub scope: ExtensionScope,
    pub kind: ExtensionKind,
    pub path: PathBuf,
    pub precedence: u32,
}

impl ExtensionRoot {
    pub fn new(
        scope: ExtensionScope,
        kind: ExtensionKind,
        path: impl Into<PathBuf>,
        precedence: u32,
    ) -> Self {
        Self {
            scope,
            kind,
            path: path.into(),
            precedence,
        }
    }
}

/// A discovered extension entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    /// The name of the extension (directory name under the root).
    pub name: String,
    pub kind: ExtensionKind,
    pub scope: ExtensionScope,
    /// Path to the manifest file (e.g. tool.yaml, SKILL.md).
    pub manifest_path: PathBuf,
    /// Path to the extension directory (parent of the manifest).
    pub directory: PathBuf,
    pub precedence: u32,
}

/// Result of a scan operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    pub entries: Vec<ScanEntry>,
}

impl ScanResult {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn filter_by_kind(&self, kind: ExtensionKind) -> Vec<&ScanEntry> {
        self.entries
            .iter()
            .filter(|e| e.kind == kind)
            .collect()
    }

    pub fn filter_by_scope(&self, scope: ExtensionScope) -> Vec<&ScanEntry> {
        self.entries
            .iter()
            .filter(|e| e.scope == scope)
            .collect()
    }
}

/// Default root directories for each extension kind.
pub fn default_extension_roots(
    user_home: Option<&std::path::Path>,
    project_root: &std::path::Path,
) -> Vec<ExtensionRoot> {
    let mut roots = Vec::new();

    // User-level roots
    if let Some(home) = user_home {
        roots.push(ExtensionRoot::new(
            ExtensionScope::User,
            ExtensionKind::Agent,
            home.join(".core-agent").join("agents"),
            100,
        ));
        roots.push(ExtensionRoot::new(
            ExtensionScope::User,
            ExtensionKind::Tool,
            home.join(".core-agent").join("tools"),
            100,
        ));
        roots.push(ExtensionRoot::new(
            ExtensionScope::User,
            ExtensionKind::Skill,
            home.join(".core-agent").join("skills"),
            100,
        ));
        roots.push(ExtensionRoot::new(
            ExtensionScope::User,
            ExtensionKind::Mcp,
            home.join(".core-agent").join("mcp"),
            100,
        ));
    }

    // Project-level roots (highest precedence)
    roots.push(ExtensionRoot::new(
        ExtensionScope::Project,
        ExtensionKind::Agent,
        project_root.join(".core-agent").join("agents"),
        200,
    ));
    roots.push(ExtensionRoot::new(
        ExtensionScope::Project,
        ExtensionKind::Tool,
        project_root.join(".core-agent").join("tools"),
        200,
    ));
    roots.push(ExtensionRoot::new(
        ExtensionScope::Project,
        ExtensionKind::Skill,
        project_root.join(".core-agent").join("skills"),
        200,
    ));
    roots.push(ExtensionRoot::new(
        ExtensionScope::Project,
        ExtensionKind::Mcp,
        project_root.join(".core-agent").join("mcp"),
        200,
    ));

    roots
}

impl fmt::Display for ExtensionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for ExtensionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}