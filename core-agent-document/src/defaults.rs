use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    validate_actor, CodeBlock, Document, DocumentAST, DocumentChunk, DocumentLink, DocumentSection,
    DocumentStatus, DocumentTable, DocumentType, EmbeddingStatus,
};
use crate::error::DocumentResult;
use crate::infrastructure::{DocumentCleaner, DocumentParser, DocumentSplitter, DocumentStore};

// ── MarkdownParser ──

pub struct MarkdownParser;

impl DocumentParser for MarkdownParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Markdown {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "MarkdownParser only supports Markdown".into(),
            ));
        }
        let mut ast = DocumentAST::new();
        let parser = pulldown_cmark::Parser::new(content);
        let mut current_section: Option<(String, u32, String)> = None;
        let mut in_code_block = false;
        let mut code_language = None;
        let mut code_content = String::new();

        for event in parser {
            match event {
                pulldown_cmark::Event::Start(tag) => match tag {
                    pulldown_cmark::Tag::Heading {
                        level, id: _, classes: _, ..
                    } => {
                        if let Some((heading, lvl, c)) = current_section.take() {
                            ast.sections
                                .push(DocumentSection::new(heading, lvl, c));
                        }
                        current_section = Some((String::new(), level as u32, String::new()));
                    }
                    pulldown_cmark::Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_language = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(info) => {
                                if info.is_empty() {
                                    None
                                } else {
                                    Some(info.to_string())
                                }
                            }
                            pulldown_cmark::CodeBlockKind::Indented => None,
                        };
                        code_content = String::new();
                    }
                    pulldown_cmark::Tag::Link {
                        link_type: _,
                        dest_url,
                        title: _,
                        id: _,
                    } => {
                        ast.links.push(DocumentLink {
                            text: String::new(),
                            url: dest_url.to_string(),
                        });
                    }
                    _ => {}
                },
                pulldown_cmark::Event::End(tag) => match tag {
                    pulldown_cmark::TagEnd::Heading { .. } => {}
                    pulldown_cmark::TagEnd::CodeBlock => {
                        in_code_block = false;
                        ast.code_blocks.push(CodeBlock {
                            language: code_language.take(),
                            code: code_content.clone(),
                        });
                        code_content.clear();
                    }
                    _ => {}
                },
                pulldown_cmark::Event::Text(text) => {
                    if in_code_block {
                        code_content.push_str(&text);
                    } else if let Some((ref mut heading, _, ref mut content)) = current_section {
                        if heading.is_empty() {
                            *heading = text.to_string();
                        } else {
                            content.push_str(&text);
                        }
                    }
                }
                pulldown_cmark::Event::Code(text) => {
                    if let Some((_, _, ref mut content)) = current_section {
                        content.push_str(&text);
                    }
                }
                _ => {}
            }
        }
        if let Some((heading, lvl, c)) = current_section.take() {
            ast.sections.push(DocumentSection::new(heading, lvl, c));
        }
        if ast.title.is_none() && !ast.sections.is_empty() {
            ast.title = Some(ast.sections[0].heading.clone());
        }
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Markdown]
    }
}

// ── TxtParser ──

pub struct TxtParser;

impl DocumentParser for TxtParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Txt {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "TxtParser only supports TXT".into(),
            ));
        }
        let mut ast = DocumentAST::new();
        let lines: Vec<&str> = content.lines().collect();
        if !lines.is_empty() {
            ast.title = Some(lines[0].to_string());
        }
        let body = lines.join("\n");
        ast.sections.push(DocumentSection::new("body", 1, body));
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Txt]
    }
}

// ── CodeParser ──

pub struct CodeParser;

impl DocumentParser for CodeParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Code {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "CodeParser only supports Code".into(),
            ));
        }
        let mut ast = DocumentAST::new();
        let lines: Vec<&str> = content.lines().collect();
        let title = lines.first().map(|l| l.to_string());
        ast.title = title;
        let mut code_lang = None;
        if let Some(first) = lines.first() {
            if first.starts_with("//") || first.starts_with('#') {
                code_lang = Some("auto".to_string());
            }
        }
        ast.code_blocks.push(CodeBlock {
            language: code_lang,
            code: content.to_string(),
        });
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Code]
    }
}

// ── PdfParser ──

pub struct PdfParser;

impl DocumentParser for PdfParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Pdf {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "PdfParser only supports PDF".into(),
            ));
        }
        let mut ast = DocumentAST::new();
        let text = pdf_extract::extract_text_from_mem(&content.as_bytes())
            .map_err(|e| crate::error::DocumentError::ParseError(format!("PDF extract: {e}")))?;
        let lines: Vec<&str> = text.lines().collect();
        if !lines.is_empty() {
            ast.title = Some(lines[0].to_string());
        }
        ast.sections.push(DocumentSection::new("body", 1, text));
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Pdf]
    }
}

// ── DocxParser (MVP: simple XML text extraction) ──

pub struct DocxParser;

impl DocumentParser for DocxParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Docx {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "DocxParser only supports DOCX".into(),
            ));
        }
        // MVP: basic text extraction — DOCX is a ZIP of XML files
        let mut ast = DocumentAST::new();
        if let Ok(text) = extract_docx_text(content) {
            let lines: Vec<&str> = text.lines().collect();
            if !lines.is_empty() {
                ast.title = Some(lines[0].to_string());
            }
            ast.sections.push(DocumentSection::new("body", 1, text));
        }
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Docx]
    }
}

fn extract_docx_text(_content: &str) -> Result<String, String> {
    // MVP: In production, use a DOCX parsing library (e.g., docx-rs)
    // For now, return the raw content as-is
    Ok(_content.to_string())
}

// ── HtmlParser (MVP: regex-based text and link extraction) ──

pub struct HtmlParser;

impl DocumentParser for HtmlParser {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST> {
        if doc_type != DocumentType::Html {
            return Err(crate::error::DocumentError::UnsupportedFormat(
                "HtmlParser only supports HTML".into(),
            ));
        }
        let mut ast = DocumentAST::new();
        // Extract title
        let re = regex::Regex::new(r"<title[^>]*>([^<]+)</title>").ok();
        if let Some(re) = re {
            if let Some(caps) = re.captures(content) {
                ast.title = Some(caps[1].to_string());
            }
        }
        // Extract links
        let link_re = regex::Regex::new(r#"<a[^>]*href="([^"]+)"[^>]*>([^<]*)</a>"#).ok();
        if let Some(re) = link_re {
            for caps in re.captures_iter(content) {
                ast.links.push(DocumentLink {
                    text: caps.get(2).map_or(String::new(), |m| m.as_str().to_string()),
                    url: caps[1].to_string(),
                });
            }
        }
        // Strip HTML tags for body text
        let strip_re = regex::Regex::new(r"<[^>]+>").ok();
        let body = strip_re
            .map(|re| re.replace_all(content, " "))
            .unwrap_or_else(|| content.into());
        let body = body.trim().to_string();
        if !body.is_empty() {
            ast.sections.push(DocumentSection::new("body", 1, body));
        }
        Ok(ast)
    }

    fn supported_types(&self) -> Vec<DocumentType> {
        vec![DocumentType::Html]
    }
}

// ── DefaultDocumentCleaner ──

pub struct DefaultDocumentCleaner;

impl DocumentCleaner for DefaultDocumentCleaner {
    fn clean(&self, mut ast: DocumentAST) -> DocumentResult<DocumentAST> {
        // Remove empty sections
        ast.sections.retain(|s| !s.content.is_empty());
        // Normalize whitespace
        for section in &mut ast.sections {
            section.content = section.content.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        Ok(ast)
    }
}

// ── DefaultDocumentSplitter ──

pub struct DefaultDocumentSplitter;

impl DocumentSplitter for DefaultDocumentSplitter {
    fn split(&self, ast: &DocumentAST, max_chunk_size: usize) -> DocumentResult<Vec<String>> {
        let mut chunks = Vec::new();
        for section in &ast.sections {
            if section.content.len() <= max_chunk_size {
                let chunk = format!(
                    "# {}\n\n{}",
                    section.heading,
                    section.content
                );
                chunks.push(chunk);
            } else {
                // Split large sections by paragraph
                for paragraph in section.content.split("\n\n") {
                    if paragraph.len() <= max_chunk_size {
                        let chunk = format!(
                            "# {}\n\n{}",
                            section.heading,
                            paragraph
                        );
                        chunks.push(chunk);
                    } else {
                        // Split by sentence
                        for (i, sentence) in paragraph.split('.').enumerate() {
                            if !sentence.trim().is_empty() {
                                let chunk = format!(
                                    "# {} (part {})\n\n{}.",
                                    section.heading, i + 1,
                                    sentence.trim()
                                );
                                chunks.push(chunk);
                            }
                        }
                    }
                }
            }
        }
        if chunks.is_empty() && ast.title.is_some() {
            chunks.push(ast.title.clone().unwrap_or_default());
        }
        Ok(chunks)
    }
}

// ── InMemoryDocumentStore ──

#[derive(Clone, Default)]
struct InMemoryState {
    documents: HashMap<Uuid, Document>,
    chunks: HashMap<Uuid, Vec<DocumentChunk>>,
}

#[derive(Default)]
pub struct InMemoryDocumentStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryDocumentStore {
    fn read(&self) -> DocumentResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| crate::error::DocumentError::Internal("store lock poisoned".into()))
    }

    fn write(&self) -> DocumentResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| crate::error::DocumentError::Internal("store lock poisoned".into()))
    }
}

#[async_trait]
impl DocumentStore for InMemoryDocumentStore {
    async fn save_document(&self, document: &Document, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        document.validate()?;
        let mut state = self.write()?;
        if state.documents.contains_key(&document.id) {
            return Err(crate::error::DocumentError::Conflict(
                "document already exists".into(),
            ));
        }
        state.documents.insert(document.id, document.clone());
        Ok(())
    }

    async fn find_document(&self, id: Uuid) -> DocumentResult<Option<Document>> {
        Ok(self.read()?.documents.get(&id).cloned())
    }

    async fn list_documents(&self, _namespace: &str) -> DocumentResult<Vec<Document>> {
        let mut values: Vec<Document> = self
            .read()?
            .documents
            .values()
            .cloned()
            .collect();
        values.sort_by_key(|d| (std::cmp::Reverse(d.updated_at), d.id));
        Ok(values)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: DocumentStatus,
        chunk_count: u32,
        embedding_status: EmbeddingStatus,
        actor: &str,
    ) -> DocumentResult<()> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        let doc = state
            .documents
            .get_mut(&id)
            .ok_or_else(|| crate::error::DocumentError::NotFound(id.to_string()))?;
        doc.status = status;
        doc.chunk_count = chunk_count;
        doc.embedding_status = embedding_status;
        doc.updated_at = Utc::now();
        doc.actor = actor.into();
        Ok(())
    }

    async fn delete_document(&self, id: Uuid, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        let mut state = self.write()?;
        state
            .documents
            .remove(&id)
            .ok_or_else(|| crate::error::DocumentError::NotFound(id.to_string()))?;
        state.chunks.remove(&id);
        Ok(())
    }

    async fn save_chunks(&self, chunks: &[DocumentChunk], actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        if chunks.is_empty() {
            return Ok(());
        }
        let document_id = chunks[0].document_id;
        let mut state = self.write()?;
        let entry = state.chunks.entry(document_id).or_default();
        for chunk in chunks {
            chunk.validate()?;
            entry.push(chunk.clone());
        }
        Ok(())
    }

    async fn find_chunks(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentChunk>> {
        Ok(self
            .read()?
            .chunks
            .get(&document_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn delete_chunks(&self, document_id: Uuid, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        self.write()?.chunks.remove(&document_id);
        Ok(())
    }
}