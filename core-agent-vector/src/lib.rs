//! Vector storage and search runtime.
//!
//! P6 vector search: hybrid search combining vector similarity (cosine),
//! keyword search (FTS5), and metadata filtering.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::{
    InMemoryVectorStore, SimpleEmbeddingModel,
};
pub use domain::*;
pub use error::{VectorError, VectorResult};
pub use infrastructure::*;
pub use manager::{VectorManager, VectorManagerBuilder};
pub use persistence::SqliteVectorStore;

pub type VectorRuntime = VectorManager;