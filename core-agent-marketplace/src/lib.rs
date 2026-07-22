//! Agent Marketplace — agent capability marketplace.
//!
//! Provides a registry for Agent, Skill, Plugin, Workflow, Template,
//! Prompt, and MCP assets. Supports publish, discover, install lifecycle.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{MarketplaceError, MarketplaceResult};
pub use infrastructure::*;
pub use manager::{MarketplaceManager, MarketplaceManagerBuilder};
pub use persistence::store::SqliteMarketplaceStore;

pub type MarketplaceRuntime = MarketplaceManager;