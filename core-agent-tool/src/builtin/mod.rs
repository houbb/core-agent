//! Builtin tool implementations — file, shell, git, web, ask, todo, agent, plan, cron, lsp,
//! ast, code_index, dependency, decompiler, project, runtime, enterprise, ai, user.
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
pub mod ast;
pub mod code_index;
pub mod dependency;
pub mod decompiler;
pub mod project;
pub mod runtime;
pub mod enterprise;
pub mod ai;
pub mod user;

pub use provider::BuiltinToolProvider;