//! SQLite Model Catalog and Usage audit.

mod schema;
mod store;

pub use schema::SCHEMA_SQL;
pub use store::SqliteModelStore;
