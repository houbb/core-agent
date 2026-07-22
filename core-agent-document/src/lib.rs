//! Structured enterprise Document Intelligence Runtime.
//!
//! P6 document parsing pipeline: Upload → Parse → Clean → Split → Store.
//! Produces DocumentAST and DocumentChunk for downstream vector indexing.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::{
    CodeParser, DefaultDocumentCleaner, DefaultDocumentSplitter, HtmlParser, InMemoryDocumentStore,
    MarkdownParser, PdfParser, TxtParser,
};
pub use domain::*;
pub use error::{DocumentError, DocumentResult};
pub use infrastructure::*;
pub use manager::{DocumentManager, DocumentManagerBuilder};
pub use persistence::SqliteDocumentStore;

pub type DocumentRuntime = DocumentManager;