//! External capability, provider and Extension lifecycle Runtime.
//!
//! P12 owns extension manifests, capability/provider registration and host
//! isolation boundaries. It intentionally knows nothing about Agent, Workflow
//! or Planning.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{ExtensionError, ExtensionResult};
pub use infrastructure::*;
pub use manager::{ExtensionManager, ExtensionManagerBuilder};
pub use persistence::SqliteExtensionStore;

pub type ExtensionRuntime = ExtensionManager;
