//! Code Index tools — symbol indexing and querying.
//!
//! Uses regex-based extraction to build a lightweight code index,
//! supporting class, method, and field discovery across languages.

pub mod index;
pub mod query;

pub use index::code_index_index_tool;
pub use query::code_index_query_tool;