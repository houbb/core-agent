//! Process-local control plane for Agent OS Runtimes.

mod config;
mod domain;
mod error;
mod infrastructure;
mod kernel;
mod service;

pub use config::{ConfigSnapshot, KernelConfig};
pub use domain::*;
pub use error::{KernelError, KernelResult};
pub use infrastructure::*;
pub use kernel::{RuntimeKernel, RuntimeKernelBuilder};
pub use service::ServiceRegistry;
