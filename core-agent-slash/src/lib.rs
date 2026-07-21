//! Slash Command Runtime — user-facing quick-action command entry system.
//!
//! Provides a unified slash command system that can be embedded in any
//! entry point (CLI, TUI, Desktop, Web, API).
//!
//! Architecture:
//! ```text
//! SlashCommandRegistry
//!     ├── builtin commands (hardcoded in code)
//!     └── plugin commands (registered at runtime)
//!
//! Each command: metadata() -> validate() -> execute()
//! Observers: on_command_start -> on_command_success / on_command_failure
//! ```

mod domain;
mod error;
mod registry;

pub use domain::*;
pub use error::{SlashError, SlashResult};
pub use registry::SlashCommandRegistry;