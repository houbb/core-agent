use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RagAnswer, RagConfig, RagContext, RagQuery, RetrievalResult};
use crate::error::RagResult;

// ── QueryRewriter ──

pub trait QueryRewriter: Send + Sync {
    fn rewrite(&self, query: &RagQuery, conversation_history: Option<&str>) -> RagResult<String>;
}

// ── Retriever ──

#[async_trait]
pub trait Retriever: Send + Sync {
    async fn retrieve(&self, query: &RagQuery) -> RagResult<Vec<RetrievalResult>>;
}

// ── ContextBuilder ──

pub trait ContextBuilder: Send + Sync {
    fn build_context(&self, results: &[RetrievalResult], max_tokens: usize) -> RagResult<RagContext>;
}

// ── RagPipeline ──

#[async_trait]
pub trait RagPipeline: Send + Sync {
    async fn execute(&self, query: &RagQuery, history: Option<&str>) -> RagResult<RagAnswer>;
}

pub type SharedRagManager = Arc<crate::manager::RagManager>;