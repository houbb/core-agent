use std::sync::Arc;

use uuid::Uuid;

use crate::domain::{SearchResult, VectorQuery, VectorRecord};
use crate::error::VectorResult;
use crate::infrastructure::{EmbeddingModel, SharedVectorStore};

pub struct VectorManagerBuilder {
    store: SharedVectorStore,
    embedding_model: Arc<dyn EmbeddingModel>,
}

impl Default for VectorManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(crate::defaults::InMemoryVectorStore::default()),
            embedding_model: Arc::new(crate::defaults::SimpleEmbeddingModel),
        }
    }
}

impl VectorManagerBuilder {
    pub fn store(mut self, value: SharedVectorStore) -> Self {
        self.store = value;
        self
    }

    pub fn embedding_model(mut self, value: Arc<dyn EmbeddingModel>) -> Self {
        self.embedding_model = value;
        self
    }

    pub fn build(self) -> VectorManager {
        VectorManager {
            store: self.store,
            embedding_model: self.embedding_model,
        }
    }
}

pub struct VectorManager {
    store: SharedVectorStore,
    embedding_model: Arc<dyn EmbeddingModel>,
}

impl VectorManager {
    pub fn builder() -> VectorManagerBuilder {
        VectorManagerBuilder::default()
    }

    pub fn new(store: SharedVectorStore) -> Self {
        Self::builder().store(store).build()
    }

    /// Index a text chunk: generate embedding and store the vector record
    pub async fn index_chunk(
        &self,
        content: &str,
        source: &str,
        document_id: Option<Uuid>,
        chunk_id: Option<Uuid>,
        actor: &str,
    ) -> VectorResult<VectorRecord> {
        let embedding = self.embedding_model.embed(content)?;
        let mut record = VectorRecord::new(content, embedding, source, actor);
        record.document_id = document_id;
        record.chunk_id = chunk_id;
        self.store.insert(&record, actor).await?;
        Ok(record)
    }

    /// Batch index multiple chunks
    pub async fn batch_index(
        &self,
        chunks: &[(String, String, Option<Uuid>, Option<Uuid>)],
        actor: &str,
    ) -> VectorResult<Vec<VectorRecord>> {
        let mut records = Vec::new();
        for (content, source, doc_id, chunk_id) in chunks {
            let record = self
                .index_chunk(content, source, *doc_id, *chunk_id, actor)
                .await?;
            records.push(record);
        }
        Ok(records)
    }

    /// Search using hybrid (vector + keyword) approach
    pub async fn search(&self, query: &VectorQuery) -> VectorResult<Vec<SearchResult>> {
        self.store.hybrid_search(query).await
    }

    /// Search similar to a given text
    pub async fn search_similar(
        &self,
        text: &str,
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>> {
        let embedding = self.embedding_model.embed(text)?;
        let mut query = VectorQuery::new(Some(text.to_string()), Some(embedding));
        query.top_k = top_k;
        self.store.hybrid_search(&query).await
    }

    /// Delete all vectors for a document
    pub async fn delete_document_vectors(&self, document_id: Uuid, actor: &str) -> VectorResult<()> {
        let records = self.store.list_by_document(document_id).await?;
        for record in records {
            self.store.delete(record.id, actor).await?;
        }
        Ok(())
    }

    pub fn embedding_model_ref(&self) -> &Arc<dyn EmbeddingModel> {
        &self.embedding_model
    }

    /// Get reference to store (for testing)
    pub fn store_ref(&self) -> SharedVectorStore {
        self.store.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::VectorQuery;

    #[tokio::test]
    async fn index_and_search_roundtrip() {
        let manager = VectorManager::builder().build();
        manager
            .index_chunk("payment gateway timeout", "doc", None, None, "tester")
            .await
            .unwrap();
        let results = manager.search_similar("timeout", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].score > 0.0);
    }

    #[tokio::test]
    async fn hybrid_search_combines_vector_and_keyword() {
        let manager = VectorManager::builder().build();
        manager
            .index_chunk("database connection pool exhausted", "doc1", None, None, "tester")
            .await
            .unwrap();
        manager
            .index_chunk("order service timeout", "doc2", None, None, "tester")
            .await
            .unwrap();
        let results = manager.search_similar("connection failed", 5).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn delete_document_vectors_works() {
        let doc_id = Uuid::new_v4();
        let manager = VectorManager::builder().build();
        manager
            .index_chunk("test content", "doc", Some(doc_id), None, "tester")
            .await
            .unwrap();
        manager
            .delete_document_vectors(doc_id, "cleaner")
            .await
            .unwrap();
        assert!(manager
            .store
            .list_by_document(doc_id)
            .await
            .unwrap()
            .is_empty());
    }
}