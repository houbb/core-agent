//! Durable Multi-Agent Team Runtime.
//!
//! P11 owns organization, team, role, membership, routing and collaboration
//! governance. It delegates single-Agent work through `AgentDispatcher` and
//! never implements Planning or Execution itself.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{MultiAgentError, MultiAgentResult};
pub use infrastructure::*;
pub use manager::{MultiAgentManager, MultiAgentManagerBuilder};
pub use persistence::SqliteMultiAgentStore;

pub type MultiAgentRuntime = MultiAgentManager;
