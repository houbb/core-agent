//! core-agent-workspace — provider-neutral Workspace Runtime.
//!
//! A Workspace is an Agent operating environment, not a directory. The crate
//! owns lifecycle, projects, resources, environment discovery, graph indexing,
//! snapshots and persistence. It intentionally does not execute Tools, Plans or
//! Models and has no dependency on the other Runtime crates.

pub mod application;
pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod persistence;
pub mod providers;

pub use application::{
    EnvironmentManager, ProjectManager, ResourceManager, WorkspaceManager, WorkspaceManagerBuilder,
};
pub use domain::*;
pub use error::{WorkspaceError, WorkspaceResult};
pub use infrastructure::*;
pub use persistence::SqliteWorkspaceStore;
pub use providers::{
    LocalEnvironmentDetector, LocalProjectScanner, LocalResourceProvider, LocalWorkspaceIndexer,
    LocalWorkspaceProvider, LocalWorkspaceSnapshot, ScanOptions, SnapshotOptions,
};

/// Public Runtime name for callers that prefer phase-oriented naming.
pub type WorkspaceRuntime = WorkspaceManager;
