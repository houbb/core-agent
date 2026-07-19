//! Persistence 层 — SQLite 实现
//!
//! context_snapshot 表 + SqliteContextSnapshotStore + 4 个内置 Provider

pub mod providers;
mod schema;
mod store;

pub use schema::*;
pub use store::SqliteContextSnapshotStore;
