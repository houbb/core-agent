//! Persistence 层 — SQLite 实现
//!
//! context_snapshot 表 + SqliteContextSnapshotStore + 4 个内置 Provider + ReferenceStore

pub mod providers;
mod reference_store;
mod schema;
mod store;

pub use reference_store::SqliteContextReferenceStore;
pub use schema::*;
pub use store::SqliteContextSnapshotStore;
