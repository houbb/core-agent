//! Agent Network — agent discovery and communication.
//!
//! Provides agent registry, capability-based discovery, status tracking,
//! and reputation management. Enables Agent Society formation.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{NetworkError, NetworkResult};
pub use infrastructure::*;
pub use manager::{NetworkManager, NetworkManagerBuilder};
pub use persistence::store::SqliteNetworkStore;

pub type NetworkRuntime = NetworkManager;