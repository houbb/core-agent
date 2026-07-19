//! Durable, command-oriented execution of approved Planning Runtime plans.
//!
//! This crate owns execution progress, retries, checkpoints and compensation.
//! It deliberately does not create Plans and does not depend on Tool Runtime.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{ExecutionError, ExecutionResult};
pub use infrastructure::*;
pub use manager::{ExecutionEngine, ExecutionManager, ExecutionManagerBuilder};
pub use persistence::SqliteExecutionStore;

pub type ExecutionRuntime = ExecutionManager;
