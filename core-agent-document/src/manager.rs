use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    validate_actor, Document, DocumentAST, DocumentChunk, DocumentStatus, DocumentType,
    EmbeddingStatus, DocumentSourceKind,
};
use crate::error::{DocumentError, DocumentResult};
use crate::infrastructure::{
    DocumentCleaner, DocumentParser, DocumentSplitter, SharedDocumentStore,
};

pub struct DocumentManagerBuilder {
    store: SharedDocumentStore,
    parsers: Vec<Arc<dyn DocumentParser>>,
    cleaner: Arc<dyn DocumentCleaner>,
    splitter: Arc<dyn DocumentSplitter>,
}

impl Default for DocumentManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(crate::defaults::InMemoryDocumentStore::default()),
            parsers: vec![
                Arc::new(crate::defaults::MarkdownParser),
                Arc::new(crate::defaults::TxtParser),
                Arc::new(crate::defaults::CodeParser),
                Arc::new(crate::defaults::PdfParser),
                Arc::new(crate::defaults::DocxParser),
                Arc::new(crate::defaults::HtmlParser),
            ],
            cleaner: Arc::new(crate::defaults::DefaultDocumentCleaner),
            splitter: Arc::new(crate::defaults::DefaultDocumentSplitter),
        }
    }
}

impl DocumentManagerBuilder {
    pub fn store(mut self, value: SharedDocumentStore) -> Self {
        self.store = value;
        self
    }

    pub fn parser(mut self, value: Arc<dyn DocumentParser>) -> Self {
        self.parsers.push(value);
        self
    }

    pub fn cleaner(mut self, value: Arc<dyn DocumentCleaner>) -> Self {
        self.cleaner = value;
        self
    }

    pub fn splitter(mut self, value: Arc<dyn DocumentSplitter>) -> Self {
        self.splitter = value;
        self
    }

    pub fn build(self) -> DocumentManager {
        DocumentManager {
            store: self.store,
            parsers: self.parsers,
            cleaner: self.cleaner,
            splitter: self.splitter,
        }
    }
}

pub struct DocumentManager {
    store: SharedDocumentStore,
    parsers: Vec<Arc<dyn DocumentParser>>,
    cleaner: Arc<dyn DocumentCleaner>,
    splitter: Arc<dyn DocumentSplitter>,
}

impl DocumentManager {
    pub fn builder() -> DocumentManagerBuilder {
        DocumentManagerBuilder::default()
    }

    pub fn new(store: SharedDocumentStore) -> Self {
        Self::builder().store(store).build()
    }

    /// Process a document through the full pipeline:
    /// Upload → Parse → Clean → Split → Store
    pub async fn process_document(
        &self,
        name: &str,
        content: &str,
        doc_type: DocumentType,
        source: DocumentSourceKind,
        max_chunk_size: usize,
        actor: &str,
    ) -> DocumentResult<Document> {
        let mut document = Document::new(name, content, doc_type, source, actor);
        document.validate()?;

        // Upload
        document.status = DocumentStatus::Uploaded;
        self.store.save_document(&document, actor).await?;

        // Parse
        document.status = DocumentStatus::Parsing;
        let parser = self
            .parsers
            .iter()
            .find(|p| p.supported_types().contains(&doc_type))
            .ok_or_else(|| {
                DocumentError::UnsupportedFormat(format!("no parser for {doc_type:?}"))
            })?;
        let mut ast = parser.parse(content, doc_type)?;
        document.status = DocumentStatus::Parsed;
        self.store
            .update_status(document.id, document.status, 0, document.embedding_status, actor)
            .await?;

        // Clean
        document.status = DocumentStatus::Cleaning;
        ast = self.cleaner.clean(ast)?;
        document.status = DocumentStatus::Cleaned;

        // Split
        document.status = DocumentStatus::Splitting;
        let chunk_texts = self.splitter.split(&ast, max_chunk_size)?;
        let chunks: Vec<DocumentChunk> = chunk_texts
            .into_iter()
            .enumerate()
            .map(|(i, text)| DocumentChunk::new(document.id, i as u32, text, actor))
            .collect();
        document.status = DocumentStatus::Split;
        document.chunk_count = chunks.len() as u32;

        // Store chunks
        self.store.save_chunks(&chunks, actor).await?;
        document.status = DocumentStatus::Embedding;
        document.embedding_status = EmbeddingStatus::Pending;
        self.store
            .update_status(
                document.id,
                document.status,
                document.chunk_count,
                document.embedding_status,
                actor,
            )
            .await?;

        Ok(document)
    }

    pub async fn get_document(&self, id: Uuid) -> DocumentResult<Option<Document>> {
        self.store.find_document(id).await
    }

    pub async fn get_chunks(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentChunk>> {
        self.store.find_chunks(document_id).await
    }

    pub async fn list_documents(&self, namespace: &str) -> DocumentResult<Vec<Document>> {
        self.store.list_documents(namespace).await
    }

    pub async fn delete_document(&self, id: Uuid, actor: &str) -> DocumentResult<()> {
        self.store.delete_chunks(id, actor).await?;
        self.store.delete_document(id, actor).await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: DocumentStatus,
        chunk_count: u32,
        embedding_status: EmbeddingStatus,
        actor: &str,
    ) -> DocumentResult<()> {
        self.store
            .update_status(id, status, chunk_count, embedding_status, actor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn process_markdown_document_produces_chunks() {
        let manager = DocumentManager::builder().build();
        let md = "# Title\n\n## Section 1\nContent here.\n\n## Section 2\nMore content.";
        let doc = manager
            .process_document("test.md", md, DocumentType::Markdown, DocumentSourceKind::Manual, 1024, "tester")
            .await
            .unwrap();
        assert_eq!(doc.status, DocumentStatus::Embedding);
        assert!(doc.chunk_count > 0);
        let chunks = manager.get_chunks(doc.id).await.unwrap();
        assert_eq!(chunks.len() as u32, doc.chunk_count);
    }

    #[tokio::test]
    async fn delete_document_removes_chunks() {
        let manager = DocumentManager::builder().build();
        let md = "# Hello\nWorld";
        let doc = manager
            .process_document("del.md", md, DocumentType::Markdown, DocumentSourceKind::Manual, 1024, "tester")
            .await
            .unwrap();
        manager.delete_document(doc.id, "cleaner").await.unwrap();
        assert!(manager.get_document(doc.id).await.unwrap().is_none());
        assert!(manager.get_chunks(doc.id).await.unwrap().is_empty());
    }
}