//! Retrieval Augmented Generation Runtime.
//!
//! P6 RAG pipeline: Question → Query Rewrite → Retriever → Context Builder → Answer.
//! Direct concatenation (no compression+reranking for MVP).

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;

pub use defaults::{
    DefaultContextBuilder, DefaultQueryRewriter, DefaultRagPipeline, DefaultRetriever,
};
pub use domain::*;
pub use error::{RagError, RagResult};
pub use infrastructure::*;
pub use manager::{RagManager, RagManagerBuilder};

pub type RagRuntime = RagManager;
