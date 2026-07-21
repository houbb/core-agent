//! Plugin Runtime — Agent platform extension system.
//!
//! Builds on core-agent-extension to provide a user-facing Plugin model:
//! - PluginManifest (YAML-based, similar to VS Code extension manifest)
//! - PluginLifecycle: install → enable → disable → uninstall
//! - PluginManager that delegates to ExtensionManager internally
//! - PluginPackage: .zip format with manifest.json + tools + skills + agents

mod domain;
mod error;
mod manager;

pub use domain::*;
pub use error::{PluginError, PluginResult};
pub use manager::{PluginManager, PluginManagerBuilder};

pub type PluginRuntime = PluginManager;