//! LSP tools — definition, references, hover, completion, diagnostics, symbols.
//! NOTE: These are stubs that require an LSP client integration.

pub mod definition;
pub mod references;
pub mod hover;
pub mod completion;
pub mod diagnostics;
pub mod symbols;

pub use definition::lsp_definition_tool;
pub use references::lsp_references_tool;
pub use hover::lsp_hover_tool;
pub use completion::lsp_completion_tool;
pub use diagnostics::lsp_diagnostics_tool;
pub use symbols::lsp_symbols_tool;