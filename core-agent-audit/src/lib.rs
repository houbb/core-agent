//! Audit Log — append-only enterprise audit trail.
//!
//! Provides append-only event recording, querying, and snapshot aggregation.
//! Every audit event is immutable once created.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{AuditError, AuditResult};
pub use infrastructure::*;
pub use manager::{AuditManager, AuditManagerBuilder};
pub use persistence::SqliteAuditStore;

pub type AuditRuntime = AuditManager;