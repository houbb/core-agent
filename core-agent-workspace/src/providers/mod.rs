mod local;

pub use local::{
    LocalEnvironmentDetector, LocalProjectScanner, LocalResourceProvider, LocalWorkspaceIndexer,
    LocalWorkspaceProvider, LocalWorkspaceSnapshot, ScanOptions, SnapshotOptions,
};
