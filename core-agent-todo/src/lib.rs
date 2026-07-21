//! core-agent-todo — User-visible progress tracking.
//!
//! Lightweight todo items that reflect execution progress to the user.
//! Todos are derived from Plan tasks and synced with execution status.

mod domain;
mod runtime;

pub use domain::*;
pub use runtime::*;

pub type TodoRuntime = TodoManager;