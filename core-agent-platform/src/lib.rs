//! Enterprise governance layer for all Agent business Runtimes.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{PlatformError, PlatformResult};
pub use infrastructure::*;
pub use manager::{PlatformManager, PlatformManagerBuilder};
pub use persistence::SqlitePlatformStore;

pub type PlatformRuntime = PlatformManager;
