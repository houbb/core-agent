//! SubAgent Runtime — Agent creation, lifecycle management, and registry.
//!
//! P2 owns the ability for one Agent to create, manage, and destroy other Agents.
//! It provides AgentInstance registry, lifecycle state machine, factory, and
//! observer/interceptor hooks.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{SubAgentError, SubAgentResult};
pub use infrastructure::*;
pub use manager::{SubAgentManager, SubAgentManagerBuilder};
pub use persistence::SqliteSubAgentStore;

pub type SubAgentRuntime = SubAgentManager;