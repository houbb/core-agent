//! Agent Autonomous — autonomous agent loop and self-improvement.
//!
//! Provides the Observe → Understand → Plan → Act → Evaluate → Learn loop,
//! autonomy levels (L0-L4), goal management, and trigger-based activation.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{AutonomousError, AutonomousResult};
pub use infrastructure::*;
pub use manager::{AutonomousManager, AutonomousManagerBuilder};
pub use persistence::store::SqliteAutonomousStore;

pub type AutonomousRuntime = AutonomousManager;