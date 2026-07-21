//! core-agent-reflection — Agent self-evaluation after execution.
//!
//! Evaluates execution results against criteria, produces scores and suggestions.

mod domain;
mod runtime;

pub use domain::*;
pub use runtime::*;

pub type ReflectionRuntime = ReflectionManager;