use std::sync::Arc;
use std::time::Instant;

use crate::domain::{RagAnswer, RagConfig, RagQuery};
use crate::error::RagResult;
use crate::infrastructure::{ContextBuilder, QueryRewriter, RagPipeline, Retriever};
use core_agent_vector::VectorManager;

pub struct RagManagerBuilder {
    vector_manager: Arc<VectorManager>,
    query_rewriter: Arc<dyn QueryRewriter>,
    retriever: Arc<dyn Retriever>,
    context_builder: Arc<dyn ContextBuilder>,
    pipeline: Option<Arc<dyn RagPipeline>>,
    config: RagConfig,
}

impl Default for RagManagerBuilder {
    fn default() -> Self {
        let vector_manager = Arc::new(VectorManager::builder().build());
        let config = RagConfig::default();
        let retriever = Arc::new(crate::defaults::DefaultRetriever::new(vector_manager.clone()));
        let context_builder = Arc::new(crate::defaults::DefaultContextBuilder);
        Self {
            vector_manager,
            query_rewriter: Arc::new(crate::defaults::DefaultQueryRewriter),
            retriever,
            context_builder,
            pipeline: None,
            config,
        }
    }
}

impl RagManagerBuilder {
    pub fn vector_manager(mut self, value: Arc<VectorManager>) -> Self {
        self.vector_manager = value;
        self
    }

    pub fn query_rewriter(mut self, value: Arc<dyn QueryRewriter>) -> Self {
        self.query_rewriter = value;
        self
    }

    pub fn retriever(mut self, value: Arc<dyn Retriever>) -> Self {
        self.retriever = value;
        self
    }

    pub fn context_builder(mut self, value: Arc<dyn ContextBuilder>) -> Self {
        self.context_builder = value;
        self
    }

    pub fn pipeline(mut self, value: Arc<dyn RagPipeline>) -> Self {
        self.pipeline = Some(value);
        self
    }

    pub fn config(mut self, value: RagConfig) -> Self {
        self.config = value;
        self
    }

    pub fn build(self) -> RagManager {
        let pipeline = self.pipeline.unwrap_or_else(|| {
            Arc::new(crate::defaults::DefaultRagPipeline::new(
                self.retriever.clone(),
                self.context_builder.clone(),
                self.config.clone(),
            ))
        });
        RagManager {
            vector_manager: self.vector_manager,
            pipeline,
            config: self.config,
        }
    }
}

pub struct RagManager {
    vector_manager: Arc<VectorManager>,
    pipeline: Arc<dyn RagPipeline>,
    config: RagConfig,
}

impl RagManager {
    pub fn builder() -> RagManagerBuilder {
        RagManagerBuilder::default()
    }

    pub fn new(vector_manager: Arc<VectorManager>) -> Self {
        Self::builder().vector_manager(vector_manager).build()
    }

    /// Ask a question: full RAG pipeline (retrieve + context + answer)
    pub async fn ask(
        &self,
        question: &str,
        namespace: &str,
        _actor: &str,
    ) -> RagResult<RagAnswer> {
        let mut query = RagQuery::new(question);
        query.namespace = namespace.into();
        query.top_k = self.config.max_results;
        query.min_score = self.config.min_score;
        query.max_context_tokens = self.config.max_context_tokens;

        self.pipeline.execute(&query, None).await
    }

    /// Ask with conversation history
    pub async fn ask_with_history(
        &self,
        question: &str,
        history: &str,
        namespace: &str,
        _actor: &str,
    ) -> RagResult<RagAnswer> {
        let mut query = RagQuery::new(question);
        query.namespace = namespace.into();
        query.top_k = self.config.max_results;
        query.min_score = self.config.min_score;
        query.max_context_tokens = self.config.max_context_tokens;

        self.pipeline.execute(&query, Some(history)).await
    }

    /// Search-only: just retrieve without generating answer
    pub async fn search(
        &self,
        question: &str,
        namespace: &str,
        top_k: usize,
    ) -> RagResult<Vec<crate::domain::RetrievalResult>> {
        let mut query = RagQuery::new(question);
        query.namespace = namespace.into();
        query.top_k = top_k;

        let retriever = crate::defaults::DefaultRetriever::new(self.vector_manager.clone());
        retriever.retrieve(&query).await
    }

    pub fn config(&self) -> &RagConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ask_returns_answer_with_sources() {
        let manager = RagManager::builder().build();
        // Index some test data
        manager
            .vector_manager
            .index_chunk("payment gateway timeout", "doc", None, None, "tester")
            .await
            .unwrap();
        manager
            .vector_manager
            .index_chunk("database connection pool", "doc", None, None, "tester")
            .await
            .unwrap();

        let answer = manager.ask("timeout", "default", "tester").await.unwrap();
        assert!(!answer.answer.is_empty());
        assert!(answer.sources.len() <= 5);
    }

    #[tokio::test]
    async fn search_returns_retrieval_results() {
        let manager = RagManager::builder().build();
        manager
            .vector_manager
            .index_chunk("order service error", "doc", None, None, "tester")
            .await
            .unwrap();

        let results = manager.search("error", "default", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.content.contains("error")));
    }
}