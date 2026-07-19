//! Infrastructure 层 — 扩展点定义
//!
//! Context Runtime 的扩展点，定义稳定的 trait 接口。
//! 企业版只需要新增实现，不需要修改核心代码。

pub mod cache;
pub mod composer;
pub mod observer;
pub mod provider;
pub mod reducer;
pub mod serializer;
pub mod snapshot_store;

// 重导出核心 trait
pub use cache::ContextCache;
pub use composer::ContextComposer;
pub use observer::{ContextObservation, ContextObserver, ContextStage};
pub use provider::{ContextProvider, ProviderContext};
pub use reducer::{ContextReducer, ReducerConfig};
pub use serializer::{ContextSerializer, JsonContextSerializer};
pub use snapshot_store::{ContextSnapshotMeta, ContextSnapshotStore};
