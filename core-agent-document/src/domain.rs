use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;
use uuid::Uuid;

use crate::error::{DocumentError, DocumentResult};

const MAX_CONTENT_BYTES: usize = 256 * 1024;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_ITEMS: usize = 256;

pub type DocumentMetadata = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

// ── DocumentType ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentType {
    Markdown,
    Txt,
    Code,
    Pdf,
    Docx,
    Html,
}
string_enum!(DocumentType {
    Markdown => "MARKDOWN",
    Txt => "TXT",
    Code => "CODE",
    Pdf => "PDF",
    Docx => "DOCX",
    Html => "HTML",
});

impl DocumentType {
    pub fn from_extension(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?.to_lowercase();
        match ext.as_str() {
            "md" | "markdown" => Some(Self::Markdown),
            "txt" | "text" => Some(Self::Txt),
            "rs" | "py" | "js" | "ts" | "java" | "go" | "c" | "cpp" | "h" | "hpp" | "rb"
            | "php" | "swift" | "kt" | "scala" | "sh" | "bash" | "yaml" | "yml"
            | "toml" | "json" | "xml" | "sql" | "css" | "vue" | "svelte" => {
                Some(Self::Code)
            }
            "pdf" => Some(Self::Pdf),
            "docx" | "doc" => Some(Self::Docx),
            "htm" | "html" => Some(Self::Html),
            _ => None,
        }
    }
}

// ── DocumentStatus ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentStatus {
    Uploaded,
    Parsing,
    Parsed,
    Cleaning,
    Cleaned,
    Splitting,
    Split,
    Embedding,
    Embedded,
    Failed,
}
string_enum!(DocumentStatus {
    Uploaded => "UPLOADED",
    Parsing => "PARSING",
    Parsed => "PARSED",
    Cleaning => "CLEANING",
    Cleaned => "CLEANED",
    Splitting => "SPLITTING",
    Split => "SPLIT",
    Embedding => "EMBEDDING",
    Embedded => "EMBEDDED",
    Failed => "FAILED",
});

// ── DocumentSourceKind ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DocumentSourceKind {
    Manual,
    FileUpload,
    Url,
    Api,
    Git,
    Agent,
}
string_enum!(DocumentSourceKind {
    Manual => "MANUAL",
    FileUpload => "FILE_UPLOAD",
    Url => "URL",
    Api => "API",
    Git => "GIT",
    Agent => "AGENT",
});

// ── EmbeddingStatus ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmbeddingStatus {
    Pending,
    Embedding,
    Embedded,
    Failed,
}
string_enum!(EmbeddingStatus {
    Pending => "PENDING",
    Embedding => "EMBEDDING",
    Embedded => "EMBEDDED",
    Failed => "FAILED",
});

// ── Document ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub name: String,
    pub doc_type: DocumentType,
    pub source: DocumentSourceKind,
    pub content: String,
    pub metadata: DocumentMetadata,
    pub status: DocumentStatus,
    pub chunk_count: u32,
    pub embedding_status: EmbeddingStatus,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Document {
    pub fn new(
        name: impl Into<String>,
        content: impl Into<String>,
        doc_type: DocumentType,
        source: DocumentSourceKind,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            doc_type,
            source,
            content: content.into(),
            metadata: BTreeMap::new(),
            status: DocumentStatus::Uploaded,
            chunk_count: 0,
            embedding_status: EmbeddingStatus::Pending,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> DocumentResult<()> {
        validate_text("document name", &self.name, 1024)?;
        if self.content.len() > MAX_DOCUMENT_BYTES {
            return Err(DocumentError::Validation(
                "document content exceeds 1 MiB".into(),
            ));
        }
        validate_actor(&self.actor)?;
        if self.updated_at < self.created_at {
            return Err(DocumentError::Validation(
                "document timestamps inconsistent".into(),
            ));
        }
        Ok(())
    }
}

// ── DocumentChunk ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub index: u32,
    pub content: String,
    pub metadata: DocumentMetadata,
    pub token_count: u32,
    pub hash: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DocumentChunk {
    pub fn new(
        document_id: Uuid,
        index: u32,
        content: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        let content = content.into();
        let hash = super::semantic_hash(&content);
        Self {
            id: Uuid::new_v4(),
            document_id,
            index,
            content,
            metadata: BTreeMap::new(),
            token_count: 0,
            hash,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> DocumentResult<()> {
        if self.content.is_empty() || self.content.len() > MAX_CONTENT_BYTES {
            return Err(DocumentError::Validation(
                "chunk content must be 1..=256 KiB".into(),
            ));
        }
        validate_actor(&self.actor)?;
        if self.updated_at < self.created_at {
            return Err(DocumentError::Validation(
                "chunk timestamps inconsistent".into(),
            ));
        }
        Ok(())
    }
}

// ── DocumentAST ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentAST {
    pub title: Option<String>,
    pub sections: Vec<DocumentSection>,
    pub tables: Vec<DocumentTable>,
    pub code_blocks: Vec<CodeBlock>,
    pub links: Vec<DocumentLink>,
}

impl DocumentAST {
    pub fn new() -> Self {
        Self {
            title: None,
            sections: Vec::new(),
            tables: Vec::new(),
            code_blocks: Vec::new(),
            links: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.sections.is_empty()
            && self.tables.is_empty()
            && self.code_blocks.is_empty()
            && self.links.is_empty()
    }
}

impl Default for DocumentAST {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSection {
    pub heading: String,
    pub level: u32,
    pub content: String,
    pub children: Vec<DocumentSection>,
}

impl DocumentSection {
    pub fn new(heading: impl Into<String>, level: u32, content: impl Into<String>) -> Self {
        Self {
            heading: heading.into(),
            level,
            content: content.into(),
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentLink {
    pub text: String,
    pub url: String,
}

// ── Validation helpers ──

pub(crate) fn validate_actor(value: &str) -> DocumentResult<()> {
    validate_text("document actor", value, 256)
}

pub(crate) fn validate_text(label: &str, value: &str, max: usize) -> DocumentResult<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(DocumentError::Validation(format!(
            "{label} must contain 1..={max} safe UTF-8 bytes"
        )));
    }
    Ok(())
}

pub(crate) fn semantic_hash(value: &str) -> String {
    format!("{:x}", sha2::Sha256::digest(value.as_bytes()))
}

pub(crate) fn validate_optional_text(label: &str, value: &str, max: usize) -> DocumentResult<()> {
    if !value.is_empty() {
        validate_text(label, value, max)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_type_from_extension_works() {
        assert_eq!(DocumentType::from_extension("readme.md"), Some(DocumentType::Markdown));
        assert_eq!(DocumentType::from_extension("main.rs"), Some(DocumentType::Code));
        assert_eq!(DocumentType::from_extension("report.pdf"), Some(DocumentType::Pdf));
        assert_eq!(DocumentType::from_extension("unknown.xyz"), None);
    }

    #[test]
    fn document_validation_rejects_empty_name() {
        let doc = Document::new("", "content", DocumentType::Txt, DocumentSourceKind::Manual, "tester");
        assert!(matches!(doc.validate(), Err(DocumentError::Validation(_))));
    }

    #[test]
    fn document_ast_default_is_empty() {
        let ast = DocumentAST::new();
        assert!(ast.is_empty());
    }

    #[test]
    fn chunk_validation_works() {
        let chunk = DocumentChunk::new(Uuid::new_v4(), 0, "content", "tester");
        assert!(chunk.validate().is_ok());
        let empty = DocumentChunk::new(Uuid::new_v4(), 0, "", "tester");
        assert!(empty.validate().is_err());
    }
}