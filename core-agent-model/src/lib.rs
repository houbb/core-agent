//! core-agent-model — Provider-neutral Model Runtime.
//!
//! The crate owns model selection, capability checks, inference, streaming,
//! retry/timeout/fallback and usage collection. It intentionally has no
//! dependency on Session or Context Runtime.

pub mod application;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod persistence;
pub mod providers;

pub use application::{
    DefaultModelRouter, InferenceEngine, ModelManager, ModelManagerBuilder, StreamEngine,
};
pub use domain::*;
pub use error::{ModelError, ModelResult};
pub use infrastructure::*;
pub use persistence::SqliteModelStore;
pub use providers::OpenAiCompatibleProvider;

/// Public Runtime name for callers that prefer the phase-oriented naming.
pub type ModelRuntime = ModelManager;
