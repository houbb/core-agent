//! Builtin tool implementations — file, shell, git, web, ask, todo, agent, plan, cron, lsp.
//!
//! Every tool is a `FunctionTool` registered via `BuiltinToolProvider`.
//! No modifications to the core `ToolManager` or `ToolRegistry` are needed.

pub mod provider;
pub mod file;
pub mod shell;
pub mod git;
pub mod web;
pub mod ask;
pub mod todo;
pub mod agent;
pub mod plan;
pub mod cron;
pub mod lsp;

pub use provider::BuiltinToolProvider;