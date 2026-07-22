//! Agent Learning — experience learning and behavior optimization.
//!
//! Captures agent execution experiences, learns patterns, and optimizes
//! behavior through Skill/Workflow/Prompt learning. Every learning record
//! follows a Candidate → Review → Apply lifecycle.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{LearningError, LearningResult};
pub use infrastructure::*;
pub use manager::{LearningManager, LearningManagerBuilder};
pub use persistence::store::SqliteLearningStore;

pub type LearningRuntime = LearningManager;