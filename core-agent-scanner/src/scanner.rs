use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::domain::*;
use crate::error::{ScannerError, ScannerResult};

const DEFAULT_MAX_ENTRIES: usize = 512;

/// Unified scanner that discovers extensions from configured roots.
///
/// Walks each root directory, looking for subdirectories that contain a
/// manifest file matching the expected name for that extension kind.
#[derive(Debug, Clone)]
pub struct ExtensionRootScanner {
    roots: Vec<ExtensionRoot>,
    max_entries: usize,
}

impl Default for ExtensionRootScanner {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }
}

impl ExtensionRootScanner {
    /// Create a scanner with the given roots.
    pub fn new(roots: Vec<ExtensionRoot>) -> Self {
        Self {
            roots,
            max_entries: DEFAULT_MAX_ENTRIES,
        }
    }

    /// Set the maximum number of entries to discover.
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Add a root to scan.
    pub fn add_root(&mut self, root: ExtensionRoot) {
        self.roots.push(root);
    }

    /// Set the roots.
    pub fn set_roots(&mut self, roots: Vec<ExtensionRoot>) {
        self.roots = roots;
    }

    /// Get the current roots.
    pub fn roots(&self) -> &[ExtensionRoot] {
        &self.roots
    }

    /// Scan all roots and discover extensions.
    ///
    /// Roots are processed in precedence order. If two roots define an extension
    /// with the same `(kind, name)` pair, the higher-precedence root wins.
    /// Equal precedence causes a DuplicateEntry error.
    pub fn scan(&self) -> ScannerResult<ScanResult> {
        let mut roots = self.roots.clone();
        roots.sort_by(|a, b| {
            a.precedence
                .cmp(&b.precedence)
                .then_with(|| a.path.cmp(&b.path))
        });

        // Use (kind, name) as the dedup key.
        let mut entries: BTreeMap<(ExtensionKind, String), ScanEntry> = BTreeMap::new();

        for root in roots {
            if !root.path.exists() {
                continue;
            }
            let root_path = canonical_directory(&root.path, "extension root")?;
            let manifest_filename = root.kind.manifest_filename();

            let mut directories = std::fs::read_dir(&root_path)?
                .collect::<Result<Vec<_>, _>>()?;
            directories.sort_by_key(|e| e.file_name());

            for entry in directories {
                let metadata = std::fs::symlink_metadata(entry.path())?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    continue;
                }

                let dir_name = entry.file_name();
                // Skip hidden directories.
                if dir_name.to_string_lossy().starts_with('.') {
                    continue;
                }

                let manifest_path = entry.path().join(manifest_filename);
                if !manifest_path.exists() {
                    continue;
                }

                // Verify manifest is a regular file.
                let manifest_meta = std::fs::symlink_metadata(&manifest_path)?;
                if manifest_meta.file_type().is_symlink() || !manifest_meta.is_file() {
                    continue;
                }

                let name = dir_name.to_string_lossy().to_string();
                let key = (root.kind, name.clone());

                if let Some(existing) = entries.get(&key) {
                    if existing.precedence == root.precedence {
                        return Err(ScannerError::DuplicateEntry {
                            name: format!("{}:{}", root.kind, name),
                            first: existing.manifest_path.display().to_string(),
                            second: manifest_path.display().to_string(),
                        });
                    }
                    // Higher precedence wins; skip this one.
                    if existing.precedence > root.precedence {
                        continue;
                    }
                }

                entries.insert(key, ScanEntry {
                    name,
                    kind: root.kind,
                    scope: root.scope,
                    manifest_path,
                    directory: entry.path(),
                    precedence: root.precedence,
                });

                if entries.len() > self.max_entries {
                    return Err(ScannerError::LimitExceeded {
                        kind: "extension scan".into(),
                        limit: self.max_entries,
                    });
                }
            }
        }

        Ok(ScanResult {
            entries: entries.into_values().collect(),
        })
    }

    /// Scan for a specific kind of extension.
    pub fn scan_kind(&self, kind: ExtensionKind) -> ScannerResult<ScanResult> {
        let filtered_roots: Vec<ExtensionRoot> = self
            .roots
            .iter()
            .filter(|r| r.kind == kind)
            .cloned()
            .collect();

        let scanner = Self {
            roots: filtered_roots,
            max_entries: self.max_entries,
        };
        scanner.scan()
    }
}

fn canonical_directory(path: &Path, label: &str) -> ScannerResult<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ScannerError::InvalidRoot {
            path: path.display().to_string(),
            reason: format!("{label} is not a safe directory").into(),
        });
    }
    Ok(std::fs::canonicalize(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tool_yaml(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(
            dir.join(name).join("tool.yaml"),
            format!(
                r#"name: {}
version: "1.0.0"
description: "Test tool"
input_schema:
  type: object
  properties: {{}}
"#,
                name
            ),
        )
        .unwrap();
    }

    fn create_skill_md(dir: &Path, name: &str, description: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(
            dir.join(name).join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"
            ),
        )
        .unwrap();
    }

    fn create_server_yaml(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(
            dir.join(name).join("server.yaml"),
            format!(
                r#"name: {}
command: "echo"
args: ["hello"]
"#,
                name
            ),
        )
        .unwrap();
    }

    fn create_agent_yaml(dir: &Path, name: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
        std::fs::write(
            dir.join(name).join("agent.yaml"),
            format!(
                r#"name: {}
version: "1.0.0"
description: "Test agent"
model: "gpt-4"
"#,
                name
            ),
        )
        .unwrap();
    }

    #[test]
    fn scanner_discovers_all_extension_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        let user = tmp.path().join("user");
        let project = tmp.path().join("project");

        // User-level tools
        create_tool_yaml(&user.join("tools"), "my-tool");
        // User-level skills
        create_skill_md(&user.join("skills"), "my-skill", "User skill");
        // User-level mcp
        create_server_yaml(&user.join("mcp"), "my-server");
        // User-level agents
        create_agent_yaml(&user.join("agents"), "my-agent");

        // Project-level overrides
        create_skill_md(&project.join("skills"), "my-skill", "Project skill (override)");
        create_tool_yaml(&project.join("tools"), "project-tool");

        let roots = vec![
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, user.join("tools"), 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Skill, user.join("skills"), 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Mcp, user.join("mcp"), 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Agent, user.join("agents"), 100),
            ExtensionRoot::new(ExtensionScope::Project, ExtensionKind::Skill, project.join("skills"), 200),
            ExtensionRoot::new(ExtensionScope::Project, ExtensionKind::Tool, project.join("tools"), 200),
        ];

        let scanner = ExtensionRootScanner::new(roots);
        let result = scanner.scan().unwrap();

        // 5 entries: 1 agent + 2 tools + 1 skill (project overrides user) + 1 mcp
        assert_eq!(result.len(), 5);

        // Verify skill override: project version wins
        let skills = result.filter_by_kind(ExtensionKind::Skill);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].scope, ExtensionScope::Project);
        assert!(skills[0].manifest_path.to_string_lossy().contains("project"));

        // Verify kinds
        assert_eq!(result.filter_by_kind(ExtensionKind::Tool).len(), 2);
        assert_eq!(result.filter_by_kind(ExtensionKind::Agent).len(), 1);
        assert_eq!(result.filter_by_kind(ExtensionKind::Mcp).len(), 1);
    }

    #[test]
    fn scanner_rejects_duplicate_same_precedence() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");

        create_tool_yaml(&first, "same-tool");
        create_tool_yaml(&second, "same-tool");

        let roots = vec![
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, first, 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, second, 100),
        ];

        let scanner = ExtensionRootScanner::new(roots);
        assert!(matches!(
            scanner.scan(),
            Err(ScannerError::DuplicateEntry { .. })
        ));
    }

    #[test]
    fn scanner_skips_missing_roots_and_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let tools = tmp.path().join("tools");

        // Create a tool and a hidden directory (should be skipped)
        create_tool_yaml(&tools, "visible-tool");
        std::fs::create_dir_all(tools.join(".hidden")).unwrap();

        let roots = vec![
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, tmp.path().join("nonexistent"), 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, tools, 100),
        ];

        let scanner = ExtensionRootScanner::new(roots);
        let result = scanner.scan().unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn scanner_scan_kind_filters_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let tools = tmp.path().join("tools");
        let skills = tmp.path().join("skills");

        create_tool_yaml(&tools, "tool-a");
        create_tool_yaml(&tools, "tool-b");
        create_skill_md(&skills, "skill-a", "Skill A");

        let roots = vec![
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Tool, tools, 100),
            ExtensionRoot::new(ExtensionScope::User, ExtensionKind::Skill, skills, 100),
        ];

        let scanner = ExtensionRootScanner::new(roots);
        let tool_result = scanner.scan_kind(ExtensionKind::Tool).unwrap();
        assert_eq!(tool_result.len(), 2);

        let skill_result = scanner.scan_kind(ExtensionKind::Skill).unwrap();
        assert_eq!(skill_result.len(), 1);
    }

    #[test]
    fn default_roots_include_all_kinds() {
        let tmp = tempfile::tempdir().unwrap();
        let roots = default_extension_roots(Some(tmp.path()), tmp.path());
        assert_eq!(roots.len(), 8); // 4 user + 4 project
        assert!(roots.iter().any(|r| r.kind == ExtensionKind::Agent));
        assert!(roots.iter().any(|r| r.kind == ExtensionKind::Tool));
        assert!(roots.iter().any(|r| r.kind == ExtensionKind::Skill));
        assert!(roots.iter().any(|r| r.kind == ExtensionKind::Mcp));
    }
}