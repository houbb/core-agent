//! core-agent-question — Human-in-the-loop collaboration for Agent.
//!
//! Supports CHOICE, CONFIRM, INPUT, APPROVAL, REVIEW question types.
//! Questions are created, answered, and can be awaited via async channels.

mod domain;
mod runtime;

pub use domain::*;
pub use runtime::*;

pub type QuestionRuntime = QuestionManager;