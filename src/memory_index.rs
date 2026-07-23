//! File-based memory index using `.claude/memory/MEMORY.md`
//!
//! Provides a lightweight, transparent memory system where each memory
//! is a separate markdown file with YAML frontmatter, and `MEMORY.md`
//! serves as the discoverable index. This complements the SQLite-backed
//! `MemoryManager` for users who prefer file-visible memories.
//!
//! # Directory layout
//!
//! ```text
//! .claude/memory/
//! ├── MEMORY.md          # Index file listing all memories
//! ├── my-fact.md         # Individual memory file
//! └── another-rule.md    # Another memory file
//! ```
//!
//! # Memory file format
//!
//! ```markdown
//! ---
//! name: my-fact
//! description: One-line summary used during recall
//! metadata:
//!   type: reference
//! ---
//! Content body. Link related memories with [[their-name]].
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const MEMORY_DIR_NAME: &str = ".claude";
const MEMORY_SUBDIR_NAME: &str = "memory";
const INDEX_FILE_NAME: &str = "MEMORY.md";

/// The maximum number of memory entries in the index.
const MAX_INDEX_ENTRIES: usize = 256;

/// A single entry in the MEMORY.md index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMemoryEntry {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub metadata: FileMemoryMetadata,
}

/// Metadata block for a memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMemoryMetadata {
    #[serde(rename = "type")]
    pub kind: String,
}

impl Default for FileMemoryMetadata {
    fn default() -> Self {
        Self {
            kind: "reference".into(),
        }
    }
}

/// The file-based memory index manager.
#[derive(Debug, Clone)]
pub struct MemoryIndex {
    directory: PathBuf,
    entries: BTreeMap<String, FileMemoryEntry>,
}

impl MemoryIndex {
    /// Open (or create) the memory index for the given workspace.
    ///
    /// Creates `.claude/memory/` if it does not exist, and reads the
    /// existing `MEMORY.md` index if present.
    pub fn open(workspace: &Path) -> Result<Self, MemoryIndexError> {
        let directory = workspace
            .join(MEMORY_DIR_NAME)
            .join(MEMORY_SUBDIR_NAME);
        fs::create_dir_all(&directory)?;

        let index_path = directory.join(INDEX_FILE_NAME);
        let entries = if index_path.exists() {
            Self::parse_index(&index_path)?
        } else {
            BTreeMap::new()
        };

        Ok(Self { directory, entries })
    }

    /// Save a new memory fact to the file index.
    ///
    /// Writes a `.md` file and updates `MEMORY.md`.
    pub fn save(
        &mut self,
        name: &str,
        description: &str,
        kind: &str,
        content: &str,
    ) -> Result<(), MemoryIndexError> {
        if !valid_name(name) {
            return Err(MemoryIndexError::InvalidName(name.into()));
        }
        if self.entries.len() >= MAX_INDEX_ENTRIES {
            return Err(MemoryIndexError::LimitExceeded {
                kind: "memory index entries".into(),
                limit: MAX_INDEX_ENTRIES,
            });
        }
        if description.trim().is_empty() || description.len() > 256 {
            return Err(MemoryIndexError::InvalidDescription(description.into()));
        }

        let file_path = self.directory.join(format!("{name}.md"));
        let frontmatter = format!(
            "---\nname: {name}\ndescription: {description}\nmetadata:\n  type: {kind}\n---\n\n{content}\n"
        );
        fs::write(&file_path, frontmatter)?;

        self.entries.insert(
            name.into(),
            FileMemoryEntry {
                name: name.into(),
                description: description.into(),
                metadata: FileMemoryMetadata { kind: kind.into() },
            },
        );

        self.write_index()?;
        Ok(())
    }

    /// Read the full content of a memory file by name.
    pub fn read(&self, name: &str) -> Result<Option<String>, MemoryIndexError> {
        let file_path = self.directory.join(format!("{name}.md"));
        if !file_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&file_path)?;
        Ok(Some(content))
    }

    /// List all entries in the index.
    pub fn list(&self) -> Vec<&FileMemoryEntry> {
        self.entries.values().collect()
    }

    /// Get a single entry by name.
    pub fn get(&self, name: &str) -> Option<&FileMemoryEntry> {
        self.entries.get(name)
    }

    /// Delete a memory file and remove it from the index.
    pub fn delete(&mut self, name: &str) -> Result<(), MemoryIndexError> {
        let file_path = self.directory.join(format!("{name}.md"));
        if file_path.exists() {
            fs::remove_file(&file_path)?;
        }
        self.entries.remove(name);
        self.write_index()?;
        Ok(())
    }

    /// The total number of indexed memories.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true when the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Path to the `.claude/memory/` directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    // ── Index I/O ──

    fn write_index(&self) -> Result<(), MemoryIndexError> {
        let index_path = self.directory.join(INDEX_FILE_NAME);
        let mut lines = Vec::new();
        for entry in self.entries.values() {
            let line = format!(
                "- [{}]({}.md) — {}",
                entry.name, entry.name, entry.description
            );
            lines.push(line);
        }
        lines.push(String::new()); // trailing newline
        fs::write(&index_path, lines.join("\n"))?;
        Ok(())
    }

    fn parse_index(path: &Path) -> Result<BTreeMap<String, FileMemoryEntry>, MemoryIndexError> {
        let content = fs::read_to_string(path)?;
        let mut entries = BTreeMap::new();
        for line in content.lines() {
            if let Some(entry) = Self::parse_index_line(line) {
                entries.insert(entry.name.clone(), entry);
            }
        }
        Ok(entries)
    }

    fn parse_index_line(line: &str) -> Option<FileMemoryEntry> {
        let line = line.trim();
        if !line.starts_with("- [") {
            return None;
        }
        let rest = line.strip_prefix("- [")?;
        let (name, rest) = rest.split_once("](")?;
        let (_, rest) = rest.split_once(".md)")?;
        let description = rest.strip_prefix(" — ").unwrap_or("").trim().to_owned();
        Some(FileMemoryEntry {
            name: name.into(),
            description,
            metadata: FileMemoryMetadata::default(),
        })
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

#[derive(Debug, Error)]
pub enum MemoryIndexError {
    #[error("memory index I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid memory name: {0} (use ASCII alphanumeric, '-' or '_')")]
    InvalidName(String),

    #[error("invalid memory description: {0} (must be 1..=256 bytes)")]
    InvalidDescription(String),

    #[error("{kind} exceeds limit {limit}")]
    LimitExceeded { kind: String, limit: usize },
}

pub type MemoryIndexResult<T> = Result<T, MemoryIndexError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_index_creates_directory_and_returns_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let index = MemoryIndex::open(dir.path()).unwrap();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.directory().ends_with(".claude/memory"));
    }

    #[test]
    fn save_and_list_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        index
            .save("my-fact", "A test fact", "reference", "This is the content.")
            .unwrap();
        assert_eq!(index.len(), 1);

        let entries = index.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my-fact");
        assert_eq!(entries[0].description, "A test fact");
    }

    #[test]
    fn read_returns_full_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        index
            .save("hello", "Greeting", "user", "Hello, world!")
            .unwrap();
        let content = index.read("hello").unwrap().unwrap();
        assert!(content.contains("name: hello"));
        assert!(content.contains("Hello, world!"));
    }

    #[test]
    fn read_returns_none_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        let index = MemoryIndex::open(dir.path()).unwrap();
        assert!(index.read("nonexistent").unwrap().is_none());
    }

    #[test]
    fn delete_removes_entry_and_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        index
            .save("temp", "Temporary", "reference", "Temp content")
            .unwrap();
        assert_eq!(index.len(), 1);

        index.delete("temp").unwrap();
        assert!(index.is_empty());

        let file_path = dir.path().join(".claude/memory/temp.md");
        assert!(!file_path.exists());
    }

    #[test]
    fn index_file_is_written_and_parseable() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        index
            .save("fact-a", "Fact A", "reference", "Content A")
            .unwrap();
        index
            .save("fact-b", "Fact B", "project", "Content B")
            .unwrap();

        // Re-open and verify persistence
        let reopened = MemoryIndex::open(dir.path()).unwrap();
        assert_eq!(reopened.len(), 2);
        assert!(reopened.get("fact-a").is_some());
        assert!(reopened.get("fact-b").is_some());
    }

    #[test]
    fn invalid_name_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        assert!(matches!(
            index.save("has space", "Bad", "reference", "content"),
            Err(MemoryIndexError::InvalidName(_))
        ));
        assert!(matches!(
            index.save("UPPERCASE", "Bad", "reference", "content"),
            Err(MemoryIndexError::InvalidName(_))
        ));
        assert!(matches!(
            index.save("", "Bad", "reference", "content"),
            Err(MemoryIndexError::InvalidName(_))
        ));
    }

    #[test]
    fn empty_description_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = MemoryIndex::open(dir.path()).unwrap();

        assert!(matches!(
            index.save("valid-name", "", "reference", "content"),
            Err(MemoryIndexError::InvalidDescription(_))
        ));
    }
}