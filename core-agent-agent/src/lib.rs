//! Durable single-Agent lifecycle orchestration.
//!
//! Agent Runtime owns Agent identity, reusable Profile/Policy declarations,
//! lifecycle, snapshots and the coordination boundary over Planning and
//! Execution. It intentionally does not implement Models, Tools or Context.

mod coordinator;
mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use coordinator::*;
pub use defaults::*;
pub use domain::*;
pub use error::{AgentError, AgentResult};
pub use infrastructure::*;
pub use manager::{AgentManager, AgentManagerBuilder};
pub use persistence::SqliteAgentStore;

pub type AgentRuntime = AgentManager;
