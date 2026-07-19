//! Application 层 — 用例编排
//!
//! 协调 domain / infrastructure / persistence 层，实现 Context Runtime 的全部用例。
//! 核心组件：ContextPipeline、SummaryReducer、DefaultComposer、ContextApplicationService

pub mod composer;
pub mod pipeline;
pub mod reducer;
pub mod service;

// 重导出核心类型
pub use composer::DefaultComposer;
pub use pipeline::ContextPipeline;
pub use reducer::SummaryReducer;
pub use service::ContextApplicationService;
