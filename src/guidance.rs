use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const DEFAULT_INSTRUCTION_BUDGET_BYTES: usize = 32 * 1024;
pub const DEFAULT_SKILL_METADATA_BUDGET_BYTES: usize = 8 * 1024;
pub const DEFAULT_SKILL_FILE_LIMIT_BYTES: usize = 256 * 1024;
pub const DEFAULT_MAX_SKILLS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuidanceScope {
    System,
    User,
    Project,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionDocument {
    pub scope: GuidanceScope,
    pub path: PathBuf,
    pub precedence: u32,
    pub content: String,
    pub content_sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstructionChain {
    pub documents: Vec<InstructionDocument>,
    pub total_bytes: usize,
}

impl InstructionChain {
    pub fn discover(
        global_directory: Option<&Path>,
        project_root: &Path,
        working_directory: &Path,
        max_bytes: usize,
    ) -> GuidanceResult<Self> {
        if max_bytes == 0 {
            return Err(GuidanceError::InvalidLimit(
                "instruction byte budget must be positive".into(),
            ));
        }
        let project_root = canonical_directory(project_root, "project root")?;
        let working_directory = canonical_directory(working_directory, "working directory")?;
        if !working_directory.starts_with(&project_root) {
            return Err(GuidanceError::OutsideProject {
                path: working_directory,
                root: project_root,
            });
        }

        let mut selected = Vec::new();
        if let Some(directory) = global_directory {
            if directory.exists() {
                let directory = canonical_directory(directory, "global guidance directory")?;
                if let Some(path) = select_instruction_file(&directory)? {
                    selected.push((GuidanceScope::User, path));
                }
            }
        }

        for directory in directory_chain(&project_root, &working_directory) {
            if let Some(path) = select_instruction_file(&directory)? {
                selected.push((GuidanceScope::Project, path));
            }
        }

        let mut chain = Self::default();
        for (precedence, (scope, path)) in selected.into_iter().enumerate() {
            let Some((content, bytes, content_sha256)) =
                read_utf8_file(&path, max_bytes, "instruction file")?
            else {
                continue;
            };
            let separator_bytes = usize::from(!chain.documents.is_empty()) * 2;
            let next_total = chain
                .total_bytes
                .checked_add(separator_bytes)
                .and_then(|value| value.checked_add(bytes))
                .ok_or_else(|| GuidanceError::LimitExceeded {
                    kind: "instruction chain".into(),
                    limit: max_bytes,
                })?;
            if next_total > max_bytes {
                return Err(GuidanceError::LimitExceeded {
                    kind: "instruction chain".into(),
                    limit: max_bytes,
                });
            }
            chain.documents.push(InstructionDocument {
                scope,
                path,
                precedence: u32::try_from(precedence).map_err(|_| {
                    GuidanceError::InvalidLimit("too many instruction documents".into())
                })?,
                content,
                content_sha256,
                bytes,
            });
            chain.total_bytes = next_total;
        }
        Ok(chain)
    }

    pub fn render(&self) -> String {
        self.documents
            .iter()
            .map(|document| document.content.trim())
            .filter(|content| !content.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRoot {
    pub scope: GuidanceScope,
    pub path: PathBuf,
    pub precedence: u32,
}

impl SkillRoot {
    pub fn new(scope: GuidanceScope, path: impl Into<PathBuf>, precedence: u32) -> Self {
        Self {
            scope,
            path: path.into(),
            precedence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDescriptor {
    pub name: String,
    pub description: String,
    pub scope: GuidanceScope,
    pub path: PathBuf,
    pub precedence: u32,
    pub content_sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedSkill {
    pub descriptor: SkillDescriptor,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SkillCatalog {
    descriptors: BTreeMap<String, SkillDescriptor>,
}

impl SkillCatalog {
    pub fn discover(roots: &[SkillRoot], max_skills: usize) -> GuidanceResult<Self> {
        if max_skills == 0 {
            return Err(GuidanceError::InvalidLimit(
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
            let mut directories = std::fs::read_dir(&root_path)?.collect::<Result<Vec<_>, _>>()?;
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
                let (name, description) = parse_skill_frontmatter(&content, &skill_path)?;
                let descriptor = SkillDescriptor {
                    name: name.clone(),
                    description,
                    scope: root.scope,
                    path: skill_path,
                    precedence: root.precedence,
                    content_sha256,
                    bytes,
                };
                if let Some(existing) = descriptors.get(&name) {
                    if existing.precedence == descriptor.precedence {
                        return Err(GuidanceError::DuplicateSkill {
                            name,
                            first: existing.path.clone(),
                            second: descriptor.path,
                        });
                    }
                }
                descriptors.insert(name, descriptor);
                if descriptors.len() > max_skills {
                    return Err(GuidanceError::LimitExceeded {
                        kind: "skill catalog".into(),
                        limit: max_skills,
                    });
                }
            }
        }
        Ok(Self { descriptors })
    }

    pub fn descriptors(&self) -> Vec<SkillDescriptor> {
        self.descriptors.values().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&SkillDescriptor> {
        self.descriptors.get(name)
    }

    pub fn metadata_prompt(&self, max_bytes: usize) -> GuidanceResult<String> {
        if max_bytes == 0 {
            return Err(GuidanceError::InvalidLimit(
                "skill metadata budget must be positive".into(),
            ));
        }
        let mut output = String::new();
        let mut omitted = 0_usize;
        for descriptor in self.descriptors.values() {
            let line = format!("- {}: {}", descriptor.name, descriptor.description.trim());
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
            let marker = format!("\n- ... {omitted} additional skill(s) omitted by budget");
            if output.len().saturating_add(marker.len()) <= max_bytes {
                output.push_str(&marker);
            }
        }
        Ok(output)
    }

    pub fn load(&self, name: &str, max_bytes: usize) -> GuidanceResult<LoadedSkill> {
        let descriptor = self
            .descriptors
            .get(name)
            .ok_or_else(|| GuidanceError::SkillNotFound(name.into()))?
            .clone();
        let Some((content, bytes, content_sha256)) =
            read_utf8_file(&descriptor.path, max_bytes, "skill file")?
        else {
            return Err(GuidanceError::InvalidSkill {
                path: descriptor.path,
                reason: "skill file is empty".into(),
            });
        };
        if bytes != descriptor.bytes || content_sha256 != descriptor.content_sha256 {
            return Err(GuidanceError::SkillChanged(descriptor.path));
        }
        Ok(LoadedSkill {
            descriptor,
            content,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

pub fn default_guidance_home() -> Option<PathBuf> {
    std::env::var_os("CORE_AGENT_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(|home| PathBuf::from(home).join("core-agent"))
        })
}

pub fn default_skill_roots(user_home: Option<&Path>, project_root: &Path) -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    if let Some(home) = user_home {
        roots.push(SkillRoot::new(
            GuidanceScope::System,
            home.join("skills").join(".system"),
            100,
        ));
        roots.push(SkillRoot::new(
            GuidanceScope::User,
            home.join("skills"),
            200,
        ));
    }
    roots.push(SkillRoot::new(
        GuidanceScope::Project,
        project_root.join(".agents").join("skills"),
        300,
    ));
    roots.push(SkillRoot::new(
        GuidanceScope::Project,
        project_root.join(".codex").join("skills"),
        301,
    ));
    roots
}

fn canonical_directory(path: &Path, label: &str) -> GuidanceResult<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(GuidanceError::InvalidDirectory {
            label: label.into(),
            path: path.to_path_buf(),
        });
    }
    Ok(std::fs::canonicalize(path)?)
}

fn directory_chain(root: &Path, working_directory: &Path) -> Vec<PathBuf> {
    let mut reversed = Vec::new();
    let mut current = Some(working_directory);
    while let Some(directory) = current {
        reversed.push(directory.to_path_buf());
        if directory == root {
            break;
        }
        current = directory.parent();
    }
    reversed.reverse();
    reversed
}

fn select_instruction_file(directory: &Path) -> GuidanceResult<Option<PathBuf>> {
    for name in ["AGENTS.override.md", "AGENTS.md"] {
        let candidate = directory.join(name);
        if !candidate.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&candidate)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(GuidanceError::UnsafeFile(candidate));
        }
        return Ok(Some(candidate));
    }
    Ok(None)
}

fn read_utf8_file(
    path: &Path,
    max_bytes: usize,
    kind: &str,
) -> GuidanceResult<Option<(String, usize, String)>> {
    if max_bytes == 0 {
        return Err(GuidanceError::InvalidLimit(format!(
            "{kind} byte limit must be positive"
        )));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(GuidanceError::UnsafeFile(path.to_path_buf()));
    }
    let declared_size =
        usize::try_from(metadata.len()).map_err(|_| GuidanceError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        })?;
    if declared_size > max_bytes {
        return Err(GuidanceError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        });
    }
    let bytes = std::fs::read(path)?;
    let post_read_metadata = std::fs::symlink_metadata(path)?;
    if post_read_metadata.file_type().is_symlink()
        || !post_read_metadata.is_file()
        || post_read_metadata.len() != metadata.len()
    {
        return Err(GuidanceError::UnsafeFile(path.to_path_buf()));
    }
    if bytes.len() > max_bytes {
        return Err(GuidanceError::LimitExceeded {
            kind: kind.into(),
            limit: max_bytes,
        });
    }
    let content = String::from_utf8(bytes).map_err(|_| GuidanceError::InvalidUtf8(path.into()))?;
    if content.trim().is_empty() {
        return Ok(None);
    }
    let content_sha256 = sha256(content.as_bytes());
    let bytes = content.len();
    Ok(Some((content, bytes, content_sha256)))
}

fn parse_skill_frontmatter(content: &str, path: &Path) -> GuidanceResult<(String, String)> {
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(GuidanceError::InvalidSkill {
            path: path.into(),
            reason: "SKILL.md must start with YAML frontmatter".into(),
        });
    }
    let mut name = None;
    let mut description = None;
    let mut closed = false;
    for line in lines {
        let line = line.trim();
        if line == "---" {
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = unquote(value.trim());
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                _ => {}
            }
        }
    }
    if !closed {
        return Err(GuidanceError::InvalidSkill {
            path: path.into(),
            reason: "SKILL.md frontmatter is not closed".into(),
        });
    }
    let name = name
        .filter(|value| valid_skill_name(value))
        .ok_or_else(|| GuidanceError::InvalidSkill {
            path: path.into(),
            reason: "skill name must use ASCII letters, digits, '-' or '_'".into(),
        })?;
    let description = description
        .filter(|value| !value.trim().is_empty() && value.len() <= 2_048)
        .ok_or_else(|| GuidanceError::InvalidSkill {
            path: path.into(),
            reason: "skill description is missing or invalid".into(),
        })?;
    Ok((name, description))
}

fn unquote(value: &str) -> String {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn valid_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Error)]
pub enum GuidanceError {
    #[error("guidance I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid guidance limit: {0}")]
    InvalidLimit(String),
    #[error("{label} is not a safe directory: {path}")]
    InvalidDirectory { label: String, path: PathBuf },
    #[error("guidance path {path} is outside project root {root}")]
    OutsideProject { path: PathBuf, root: PathBuf },
    #[error("guidance file is not a regular non-symlink file: {0}")]
    UnsafeFile(PathBuf),
    #[error("guidance file is not UTF-8: {0}")]
    InvalidUtf8(PathBuf),
    #[error("{kind} exceeds limit {limit}")]
    LimitExceeded { kind: String, limit: usize },
    #[error("invalid skill {path}: {reason}")]
    InvalidSkill { path: PathBuf, reason: String },
    #[error("duplicate skill {name} at the same precedence: {first} and {second}")]
    DuplicateSkill {
        name: String,
        first: PathBuf,
        second: PathBuf,
    },
    #[error("skill was not found: {0}")]
    SkillNotFound(String),
    #[error("skill changed after discovery: {0}")]
    SkillChanged(PathBuf),
}

pub type GuidanceResult<T> = Result<T, GuidanceError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_chain_applies_global_root_nested_and_override_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let global = directory.path().join("global");
        let project = directory.path().join("project");
        let nested = project.join("src").join("feature");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(global.join("AGENTS.md"), "global").unwrap();
        std::fs::write(project.join("AGENTS.md"), "project").unwrap();
        std::fs::write(project.join("src").join("AGENTS.md"), "ignored").unwrap();
        std::fs::write(project.join("src").join("AGENTS.override.md"), "override").unwrap();
        std::fs::write(nested.join("AGENTS.md"), "nested").unwrap();

        let chain = InstructionChain::discover(
            Some(&global),
            &project,
            &nested,
            DEFAULT_INSTRUCTION_BUDGET_BYTES,
        )
        .unwrap();

        assert_eq!(chain.documents.len(), 4);
        assert_eq!(chain.render(), "global\n\nproject\n\noverride\n\nnested");
        assert_eq!(chain.documents[0].scope, GuidanceScope::User);
        assert_eq!(chain.documents[3].precedence, 3);
        assert!(chain
            .documents
            .iter()
            .all(|document| document.content_sha256.len() == 64));
    }

    #[test]
    fn instruction_chain_rejects_outside_working_directory_and_budget_overflow() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("project");
        let outside = directory.path().join("outside");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(project.join("AGENTS.md"), "12345").unwrap();

        assert!(matches!(
            InstructionChain::discover(None, &project, &outside, 32),
            Err(GuidanceError::OutsideProject { .. })
        ));
        assert!(matches!(
            InstructionChain::discover(None, &project, &project, 4),
            Err(GuidanceError::LimitExceeded { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn instruction_chain_rejects_symlink_files() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(directory.path().join("rules.md"), "unsafe").unwrap();
        symlink(directory.path().join("rules.md"), project.join("AGENTS.md")).unwrap();

        assert!(matches!(
            InstructionChain::discover(None, &project, &project, 1024),
            Err(GuidanceError::UnsafeFile(_))
        ));
    }

    #[test]
    fn skill_catalog_is_lazy_bounded_and_honors_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let user = directory.path().join("user");
        let project = directory.path().join("project");
        write_skill(&user.join("review"), "review", "User review", "user body");
        write_skill(
            &project.join("review"),
            "review",
            "Project review",
            "project body",
        );
        write_skill(&project.join("test"), "test", "Run tests", "test body");
        let catalog = SkillCatalog::discover(
            &[
                SkillRoot::new(GuidanceScope::User, &user, 100),
                SkillRoot::new(GuidanceScope::Project, &project, 200),
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
    fn skill_catalog_detects_toctou_changes_and_same_priority_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        write_skill(&first.join("audit"), "audit", "Audit", "first");
        write_skill(&second.join("audit"), "audit", "Audit", "second");
        assert!(matches!(
            SkillCatalog::discover(
                &[
                    SkillRoot::new(GuidanceScope::User, &first, 100),
                    SkillRoot::new(GuidanceScope::User, &second, 100),
                ],
                10,
            ),
            Err(GuidanceError::DuplicateSkill { .. })
        ));

        let catalog =
            SkillCatalog::discover(&[SkillRoot::new(GuidanceScope::User, &first, 100)], 10)
                .unwrap();
        std::fs::write(
            first.join("audit").join("SKILL.md"),
            "---\nname: audit\ndescription: Audit\n---\nchanged",
        )
        .unwrap();
        assert!(matches!(
            catalog.load("audit", DEFAULT_SKILL_FILE_LIMIT_BYTES),
            Err(GuidanceError::SkillChanged(_))
        ));
    }

    fn write_skill(directory: &Path, name: &str, description: &str, body: &str) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n"),
        )
        .unwrap();
    }
}
