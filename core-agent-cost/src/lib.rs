//! Cost — token billing and budget control for enterprise AI.
//!
//! Provides cost recording, aggregation, and budget management.
//! Every cost record is immutable once created.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{CostError, CostResult};
pub use infrastructure::*;
pub use manager::{CostManager, CostManagerBuilder};
pub use persistence::SqliteCostStore;

pub type CostRuntime = CostManager;