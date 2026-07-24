//! Extension Root Scanner — unified discovery of agents, tools, skills, and MCP servers.
//!
//! Defines the canonical directory layout and provides a scanner that walks
//! known roots to discover all user-defined extensions.

mod domain;
mod error;
mod scanner;

pub use domain::*;
pub use error::{ScannerError, ScannerResult};
pub use scanner::ExtensionRootScanner;

/// Re-export for convenience.
pub type ExtensionScanner = ExtensionRootScanner;