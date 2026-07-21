//! Message Runtime — inter-agent communication bus and mailbox.
//!
//! P2 owns the structured message bus for agent-to-agent communication
//! with Request/Response, Broadcast, Event messaging patterns.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{MessageError, MessageResult};
pub use infrastructure::*;
pub use manager::{MessageManager, MessageManagerBuilder};
pub use persistence::SqliteMessageStore;

pub type MessageRuntime = MessageManager;