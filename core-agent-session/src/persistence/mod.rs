//! Persistence 层 — SQLite 实现
//!
//! 五张表：session / conversation / message / attachment / manifest
//! 使用 rusqlite + r2d2 连接池。

mod schema;
mod store;

pub use schema::*;
pub use store::SqliteSessionStore;
