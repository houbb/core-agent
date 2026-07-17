//! Persistence 层 — SQLite 实现
//!
//! context_snapshot 表 + SqliteContextSnapshotStore + 4 个内置 Provider

mod schema;
mod store;
pub mod providers;

pub use schema::*;
pub use store::SqliteContextSnapshotStore;