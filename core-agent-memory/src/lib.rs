//! Structured long-term Memory Runtime.
//!
//! P8 deliberately uses deterministic structured indexing and retrieval. It
//! does not depend on Context, Agent, Planning, Execution or Tool runtimes.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::{
    DefaultMemoryClassifier, DefaultMemoryIndexer, DefaultMemoryLifecycle, EmbeddedMemoryPolicy,
    InMemoryMemoryStore, StructuredMemoryRetriever,
};
pub use domain::*;
pub use error::{MemoryError, MemoryResult};
pub use infrastructure::*;
pub use manager::{MemoryManager, MemoryManagerBuilder};
pub use persistence::SqliteMemoryStore;

pub type MemoryRuntime = MemoryManager;
