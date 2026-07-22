use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{
    RagAnswer, RagConfig, RagContext, RagQuery, RetrievalResult,
};
use crate::error::{RagError, RagResult};
use crate::infrastructure::{ContextBuilder, QueryRewriter, Retriever};
use core_agent_vector::VectorManager;

// ── DefaultQueryRewriter ──

pub struct DefaultQueryRewriter;

impl QueryRewriter for DefaultQueryRewriter {
    fn rewrite(&self, query: &RagQuery, _conversation_history: Option<&str>) -> RagResult<String> {
        // MVP: return the original query unchanged
        // Future: use LLM to rewrite/expand the query
        Ok(query.question.clone())
    }
}

// ── DefaultRetriever ──

pub struct DefaultRetriever {
    vector_manager: Arc<VectorManager>,
    config: RagConfig,
}

impl DefaultRetriever {
    pub fn new(vector_manager: Arc<VectorManager>) -> Self {
        Self {
            vector_manager,
            config: RagConfig::default(),
        }
    }

    pub fn with_config(vector_manager: Arc<VectorManager>, config: RagConfig) -> Self {
        Self {
            vector_manager,
            config,
        }
    }
}

#[async_trait]
impl Retriever for DefaultRetriever {
    async fn retrieve(&self, query: &RagQuery) -> RagResult<Vec<RetrievalResult>> {
        let mut vector_query = core_agent_vector::VectorQuery::new(
            Some(query.question.clone()),
            None,
        );
        vector_query.top_k = query.top_k;
        vector_query.min_score = query.min_score;

        let results = self
            .vector_manager
            .search(&vector_query)
            .await
            .map_err(|e| RagError::RetrievalFailed(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|sr| RetrievalResult {
                content: sr.record.content,
                score: sr.score,
                source: sr.record.source,
                document_id: sr.record.document_id,
                chunk_id: sr.record.chunk_id,
                metadata: serde_json::to_value(&sr.record.metadata).unwrap_or_default(),
                matched_by: sr.matched_by,
            })
            .collect())
    }
}

// ── DefaultContextBuilder ──

pub struct DefaultContextBuilder;

impl ContextBuilder for DefaultContextBuilder {
    fn build_context(&self, results: &[RetrievalResult], max_tokens: usize) -> RagResult<RagContext> {
        let mut context_text = String::from("Relevant knowledge from the knowledge base:\n\n");
        let mut total_tokens = 0usize;
        let mut included = Vec::new();

        for result in results {
            let entry = format!(
                "[Source: {} (score: {:.2})]\n{}\n\n",
                result.source, result.score, result.content
            );
            let estimated_tokens = entry.len() / 4; // rough token estimate
            if total_tokens + estimated_tokens > max_tokens && !included.is_empty() {
                break;
            }
            total_tokens += estimated_tokens;
            context_text.push_str(&entry);
            included.push(result.clone());
        }

        context_text.push_str("Answer the user's question based on the above knowledge.\n");

        Ok(RagContext {
            results: included,
            context_text,
            total_tokens,
        })
    }
}

// ── DefaultRagPipeline ──

pub struct DefaultRagPipeline {
    retriever: Arc<dyn Retriever>,
    context_builder: Arc<dyn ContextBuilder>,
    config: RagConfig,
}

impl DefaultRagPipeline {
    pub fn new(
        retriever: Arc<dyn Retriever>,
        context_builder: Arc<dyn ContextBuilder>,
        config: RagConfig,
    ) -> Self {
        Self {
            retriever,
            context_builder,
            config,
        }
    }
}

#[async_trait]
impl crate::infrastructure::RagPipeline for DefaultRagPipeline {
    async fn execute(&self, query: &RagQuery, _history: Option<&str>) -> RagResult<RagAnswer> {
        let start = std::time::Instant::now();

        // Retrieve
        let results = self.retriever.retrieve(query).await?;

        // Build context
        let context = self
            .context_builder
            .build_context(&results, query.max_context_tokens)?;

        // For MVP, the answer is the context (no LLM call yet)
        let answer = if self.config.include_sources {
            format!(
                "Based on the knowledge base, I found the following relevant information:\n\n{}",
                context.context_text
            )
        } else {
            context.context_text.clone()
        };

        let elapsed = start.elapsed().as_millis() as u64;

        Ok(RagAnswer {
            answer,
            sources: context.results,
            confidence: 0.8,
            processing_time_ms: elapsed,
            query: query.question.clone(),
        })
    }
}