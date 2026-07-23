//! Developer Platform — agent creation, testing, and publishing tools.
//!
//! Provides the Developer Portal backend for third-party developers to:
//!
//! - **DeveloperProfile** — register and manage developer identity
//! - **DeveloperProject** — create and manage agent projects
//! - **AgentManifest** — YAML/JSON agent definitions
//! - **AgentTestRun** — test and evaluate agents
//! - **DeveloperManager** — orchestrates the full developer workflow
//! - **PublishRequest** — publish agents to the marketplace

mod domain;
mod error;
mod infrastructure;
mod manager;

pub use domain::*;
pub use error::{DeveloperError, DeveloperResult};
pub use infrastructure::*;
pub use manager::{
    DeveloperManager, InMemoryDeveloperProfileStore, InMemoryDeveloperProjectStore, NoopPublisher,
    NoopTestRunner,
};

pub type DeveloperRuntime = DeveloperManager;