//! core-agent-tool — Provider-neutral Tool Runtime.
//!
//! The crate owns Tool discovery, validation, permission checks, execution,
//! cancellation, result mapping and lifecycle audit. It intentionally has no
//! dependency on Session, Context or Model Runtime.

pub mod application;
pub mod builtin;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod persistence;
pub mod providers;

pub use application::{ToolManager, ToolManagerBuilder};
pub use builtin::BuiltinToolProvider;
pub use domain::*;
pub use error::{ToolError, ToolRuntimeResult};
pub use infrastructure::*;
pub use persistence::SqliteToolStore;
pub use providers::{FunctionTool, StaticToolProvider};

/// Public Runtime name for callers that prefer phase-oriented naming.
pub type ToolRuntime = ToolManager;
