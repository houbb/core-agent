//! Semantic Understanding Runtime.
//!
//! P6 Knowledge Graph: entity extraction + relation storage + graph query.
//! MVP: regex-based extraction, BFS traversal query.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{SemanticError, SemanticResult};
pub use infrastructure::*;
pub use manager::{SemanticManager, SemanticManagerBuilder};
pub use persistence::SqliteGraphStore;

pub type SemanticRuntime = SemanticManager;
