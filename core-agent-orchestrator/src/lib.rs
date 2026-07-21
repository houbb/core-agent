//! Orchestrator Runtime — multi-agent orchestration, strategy execution, and result aggregation.
//!
//! P2 owns the supervisor pattern with strategies (Sequential, Parallel, Supervisor, Debate)
//! and result aggregation.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{OrchestratorError, OrchestratorResult};
pub use infrastructure::*;
pub use manager::{OrchestratorManager, OrchestratorManagerBuilder};
pub use persistence::SqliteOrchestrationStore;

pub type OrchestratorRuntime = OrchestratorManager;