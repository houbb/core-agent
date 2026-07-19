//! Durable sequential Workflow Runtime.
//!
//! P10 owns business-readable definition, scheduling, progress and governance.
//! It delegates side effects through `WorkflowEngine` and never executes Tools.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{WorkflowError, WorkflowResult};
pub use infrastructure::*;
pub use manager::{WorkflowManager, WorkflowManagerBuilder};
pub use persistence::SqliteWorkflowStore;

pub type WorkflowRuntime = WorkflowManager;
