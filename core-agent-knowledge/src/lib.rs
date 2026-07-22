//! Unified Knowledge Management Runtime.
//!
//! P6 Knowledge Brain: unified entry point for all knowledge types.
//! Lifecycle: Create → Review (auto-approve) → Publish → Update → Archive.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{KnowledgeError, KnowledgeResult};
pub use infrastructure::*;
pub use manager::{KnowledgeManager, KnowledgeManagerBuilder};
pub use persistence::SqliteKnowledgeStore;

pub type KnowledgeRuntime = KnowledgeManager;
