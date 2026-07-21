//! Approval — human-in-the-loop approval for enterprise AI.
//!
//! Provides approval request lifecycle, risk engine, and decision tracking.
//! Every approval action is auditable and time-boxed.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{ApprovalError, ApprovalResult};
pub use infrastructure::*;
pub use manager::{ApprovalManager, ApprovalManagerBuilder};
pub use persistence::SqliteApprovalStore;

pub type ApprovalRuntime = ApprovalManager;