//! AST tools — semantic code search and replace using ast-grep-style patterns.
//!
//! Uses regex-based pattern matching with language-aware file filtering.

pub mod search;
pub mod replace;

pub use search::ast_search_tool;
pub use replace::ast_replace_tool;