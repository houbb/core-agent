use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    Document, DocumentAST, DocumentChunk, DocumentStatus, DocumentType,
};
use crate::error::DocumentResult;

// ── DocumentParser ──

pub trait DocumentParser: Send + Sync {
    fn parse(&self, content: &str, doc_type: DocumentType) -> DocumentResult<DocumentAST>;
    fn supported_types(&self) -> Vec<DocumentType>;
}

// ── DocumentCleaner ──

pub trait DocumentCleaner: Send + Sync {
    fn clean(&self, ast: DocumentAST) -> DocumentResult<DocumentAST>;
}

// ── DocumentSplitter ──

pub trait DocumentSplitter: Send + Sync {
    fn split(&self, ast: &DocumentAST, max_chunk_size: usize) -> DocumentResult<Vec<String>>;
}

// ── DocumentStore ──

#[async_trait]
pub trait DocumentStore: Send + Sync {
    async fn save_document(&self, document: &Document, actor: &str) -> DocumentResult<()>;
    async fn find_document(&self, id: Uuid) -> DocumentResult<Option<Document>>;
    async fn list_documents(&self, namespace: &str) -> DocumentResult<Vec<Document>>;
    async fn update_status(
        &self,
        id: Uuid,
        status: DocumentStatus,
        chunk_count: u32,
        embedding_status: crate::domain::EmbeddingStatus,
        actor: &str,
    ) -> DocumentResult<()>;
    async fn delete_document(&self, id: Uuid, actor: &str) -> DocumentResult<()>;

    async fn save_chunks(&self, chunks: &[DocumentChunk], actor: &str) -> DocumentResult<()>;
    async fn find_chunks(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentChunk>>;
    async fn delete_chunks(&self, document_id: Uuid, actor: &str) -> DocumentResult<()>;
}

pub type SharedDocumentStore = Arc<dyn DocumentStore>;