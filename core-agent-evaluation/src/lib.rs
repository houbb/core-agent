//! Agent Evaluation — quality scoring for agent task results.
//!
//! Provides multi-dimension evaluation (Correctness, Quality, Safety, Cost),
//! score aggregation, and feedback collection. Every evaluation is immutable
//! once created.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{EvaluationError, EvaluationResult};
pub use infrastructure::*;
pub use manager::{EvaluationManager, EvaluationManagerBuilder};
pub use persistence::store::SqliteEvaluationStore;

pub type EvaluationRuntime = EvaluationManager;