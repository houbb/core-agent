use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{SkillError, SkillResult};

pub const DEFAULT_SKILL_FILE_LIMIT_BYTES: usize = 256 * 1024;
pub const DEFAULT_MAX_SKILLS: usize = 256;
pub const DEFAULT_SKILL_METADATA_BUDGET_BYTES: usize = 8 * 1024;

/// The scope/lifecycle of a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    System,
    User,
    Project,
    Session,
}

/// A configured root directory for skill discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRoot {
    pub scope: SkillScope,
    pub path: PathBuf,
    pub precedence: u32,
}

impl SkillRoot {
    pub fn new(scope: SkillScope, path: impl Into<PathBuf>, precedence: u32) -> Self {
        Self {
            scope,
            path: path.into(),
            precedence,
        }
    }
}

/// Metadata for a discovered skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDescriptor {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub scope: SkillScope,
    pub path: PathBuf,
    pub precedence: u32,
    pub content_sha256: String,
    pub bytes: usize,
    pub tool_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A skill with its full instruction content loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSkill {
    pub descriptor: SkillDescriptor,
    pub content: String,
}

/// Parsed YAML frontmatter from a SKILL.md file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub steps: Vec<String>,
    #[serde(default)]
    pub examples: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// A catalog of discovered skills.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillCatalog {
    descriptors: BTreeMap<String, SkillDescriptor>,
}

impl SkillCatalog {
    /// Discover skills from the configured roots.
    ///
    /// Roots are processed in precedence order. If two roots define a skill
    /// with the same name, the higher-precedence root wins. Equal precedence
    /// causes a DuplicateSkill error.
    pub fn discover(roots: &[SkillRoot], max_skills: usize) -> SkillResult<Self> {
        if max_skills == 0 {
            return Err(SkillError::InvalidLimit(
                "maximum skill count must be positive".into(),
            ));
        }
        let mut roots = roots.to_vec();
        roots.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.path.cmp(&right.path))
        });

        let mut descriptors: BTreeMap<String, SkillDescriptor> = BTreeMap::new();
        for root in roots {
            if !root.path.exists() {
                continue;
            }
            let root_path = canonical_directory(&root.path, "skill root")?;
            let mut directories = std::fs::read_dir(&root_path)?
                .collect::<Result<Vec<_>, _>>()?;
            directories.sort_by_key(std::fs::DirEntry::file_name);
            for entry in directories {
                let metadata = std::fs::symlink_metadata(entry.path())?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    continue;
                }
                let skill_path = entry.path().join("SKILL.md");
                if !skill_path.exists() {
                    continue;
                }
                let Some((content, bytes, content_sha256)) =
                    read_utf8_file(&skill_path, DEFAULT_SKILL_FILE_LIMIT_BYTES, "skill file")?
                else {
                    continue;
                };
                let frontmatter = parse_skill_frontmatter(&content, &skill_path)?;
                let now = Utc::now();
                let descriptor = SkillDescriptor {
                    id: Uuid::new_v4(),
                    name: frontmatter.name.clone(),
                    description: frontmatter.description,
                    scope: root.scope,
                    path: skill_path,
                    precedence: root.precedence,
                    content_sha256,
                    bytes,
                    tool_count: frontmatter.tools.len(),
                    created_at: now,
                    updated_at: now,
                };
                if let Some(existing) = descriptors.get(&frontmatter.name) {
                    if existing.precedence == descriptor.precedence {
                        return Err(SkillError::DuplicateSkill {
                            name: frontmatter.name,
                            first: existing.path.display().to_string(),
                            second: descriptor.path.display().to_string(),
                        });
                    }
                }
                descriptors.insert(frontmatter.name, descriptor);
                if descriptors.len() > max_skills {
                    return Err(SkillError::LimitExceeded {
                        kind: "skill catalog".into(),
                        limit: max_skills,
                    });
                }
            }
        }
        Ok(Self { descriptors })
    }

    /// Return all skill descriptors.
    pub fn descriptors(&self) -> Vec<SkillDescriptor> {
        self.descriptors.values().cloned().collect()
    }

    /// Get a skill descriptor by name.
    pub fn get(&self, name: &str) -> Option<&SkillDescriptor> {
        self.descriptors.get(name)
    }

    /// Check if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    /// Number of skills in the catalog.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// Generate a metadata prompt describing all available skills.
    pub fn metadata_prompt(&self, max_bytes: usize) -> SkillResult<String> {
        if max_bytes == 0 {
            return Err(SkillError::InvalidLimit(
                "metadata budget must be positive".into(),
            ));
        }
        let mut output = String::new();
        let mut omitted = 0_usize;
        for descriptor in self.descriptors.values() {
            let line = format!(
                "- {}: {}",
                descriptor.name,
                descriptor.description.trim()
            );
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
            let marker = format!(
                "\n- ... {} additional skill(s) omitted by budget",
                omitted
            );
            if output.len().saturating_add(marker.len()) <= max_bytes {
                output.push_str(&marker);
            }
        }
        Ok(output)
    }

    /// Load a full skill by name.
    pub fn load(&self, name: &str, max_bytes: usize) -> SkillResult<LoadedSkill> {
        let descriptor = self
            .descriptors
            .get(name)
            .ok_or_else(|| SkillError::SkillNotFound(name.into()))?
            .clone();
        let Some((content, bytes, content_sha256)) =
            read_utf8_file(&descriptor.path, max_bytes, "skill file")?
        else {
            return Err(SkillError::InvalidSkill {
                path: descriptor.path.display().to_string(),
                reason: "skill file is empty".into(),
            });
        };
        if bytes != descriptor.bytes || content_sha256 != descriptor.content_sha256 {
            return Err(SkillError::SkillChanged(
                descriptor.path.display().to_string(),
            ));
        }
        Ok(LoadedSkill {
            descriptor,
            content,
        })
    }
}

/// Default skill root directories.
pub fn default_skill_roots(user_home: Option<&Path>, project_root: &Path) -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    if let Some(home) = user_home {
        roots.push(SkillRoot::new(
            SkillScope::System,
            home.join("skills").join(".system"),
            100,
        ));
        roots.push(SkillRoot::new(
            SkillScope::User,
            home.join("skills"),
            200,
        ));
    }
    roots.push(SkillRoot::new(
        SkillScope::Project,
        project_root.join(".agents").join("skills"),
        300,
    ));
    roots.push(SkillRoot::new(
        SkillScope::Project,
        project_root.join(".codex").join("skills"),
        301,
    ));
    roots
}

// ── Internal helpers ──

fn canonical_directory(path: &Path, label: &str) -> SkillResult<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: format!("{label} is not a safe directory").into(),
        });
    }
    Ok(std::fs::canonicalize(path)?)
}

fn read_utf8_file(
    path: &Path,
    max_bytes: usize,
    kind: &str,
) -> SkillResult<Option<(String, usize, String)>> {
    if max_bytes == 0 {
        return Err(SkillError::InvalidLimit(format!(
            "{kind} byte limit must be positive"
        )));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: "file is not a regular non-symlink file".into(),
        });
    }
    let declared_size =
        usize::try_from(metadata.len()).map_err(|_| SkillError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        })?;
    if declared_size > max_bytes {
        return Err(SkillError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        });
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() > max_bytes {
        return Err(SkillError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        });
    }
    let content = String::from_utf8(bytes)
        .map_err(|_| SkillError::InvalidUtf8(path.display().to_string()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let content_sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
    let bytes = content.len();
    Ok(Some((content, bytes, content_sha256)))
}

fn parse_skill_frontmatter(content: &str, path: &Path) -> SkillResult<SkillFrontmatter> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: "SKILL.md must start with YAML frontmatter".into(),
        });
    }
    let mut frontmatter_lines = Vec::new();
    let mut closed = false;
    for line in lines {
        let line = line.trim();
        if line == "---" {
            closed = true;
            break;
        }
        frontmatter_lines.push(line);
    }
    if !closed {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: "SKILL.md frontmatter is not closed".into(),
        });
    }
    let yaml_content = frontmatter_lines.join("\n");
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(&yaml_content)?;
    if frontmatter.name.trim().is_empty() || frontmatter.name.len() > 128 {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: "skill name must be 1..128 characters".into(),
        });
    }
    if frontmatter.description.trim().is_empty() || frontmatter.description.len() > 2048 {
        return Err(SkillError::InvalidSkill {
            path: path.display().to_string(),
            reason: "skill description is missing or too long".into(),
        });
    }
    Ok(frontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(
        directory: &Path,
        name: &str,
        description: &str,
        tools: &[&str],
        body: &str,
    ) {
        std::fs::create_dir_all(directory).unwrap();
        let tools_yaml = if tools.is_empty() {
            String::new()
        } else {
            let tools_list = tools
                .iter()
                .map(|t| format!("  - {}", t))
                .collect::<Vec<_>>()
                .join("\n");
            format!("tools:\n{}\n", tools_list)
        };
        std::fs::write(
            directory.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: {description}\n{tools_yaml}---\n\n{body}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn skill_catalog_is_lazy_bounded_and_honors_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let user = directory.path().join("user");
        let project = directory.path().join("project");
        write_skill(
            &user.join("review"),
            "review",
            "User review",
            &[],
            "user body",
        );
        write_skill(
            &project.join("review"),
            "review",
            "Project review",
            &[],
            "project body",
        );
        write_skill(
            &project.join("test"),
            "test",
            "Run tests",
            &[],
            "test body",
        );
        let catalog = SkillCatalog::discover(
            &[
                SkillRoot::new(SkillScope::User, &user, 100),
                SkillRoot::new(SkillScope::Project, &project, 200),
            ],
            10,
        )
        .unwrap();

        assert_eq!(catalog.descriptors().len(), 2);
        assert_eq!(catalog.get("review").unwrap().description, "Project review");
        assert!(catalog.metadata_prompt(128).unwrap().contains("review"));
        let loaded = catalog
            .load("review", DEFAULT_SKILL_FILE_LIMIT_BYTES)
            .unwrap();
        assert!(loaded.content.contains("project body"));
        assert!(!loaded.content.contains("user body"));
    }

    #[test]
    fn skill_catalog_detects_changes_and_same_priority_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        write_skill(&first.join("audit"), "audit", "Audit", &[], "first");
        write_skill(&second.join("audit"), "audit", "Audit", &[], "second");
        assert!(matches!(
            SkillCatalog::discover(
                &[
                    SkillRoot::new(SkillScope::User, &first, 100),
                    SkillRoot::new(SkillScope::User, &second, 100),
                ],
                10,
            ),
            Err(SkillError::DuplicateSkill { .. })
        ));

        let catalog =
            SkillCatalog::discover(&[SkillRoot::new(SkillScope::User, &first, 100)], 10)
                .unwrap();
        std::fs::write(
            first.join("audit").join("SKILL.md"),
            "---\nname: audit\ndescription: Audit\n---\n\nchanged",
        )
        .unwrap();
        assert!(matches!(
            catalog.load("audit", DEFAULT_SKILL_FILE_LIMIT_BYTES),
            Err(SkillError::SkillChanged(_))
        ));
    }
}