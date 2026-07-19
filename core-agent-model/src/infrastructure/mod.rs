//! Replaceable Model Runtime extension points and default infrastructure.

mod catalog;
mod defaults;
mod observer;
mod registry;
mod traits;

pub use catalog::InMemoryModelCatalog;
pub use defaults::{
    DefaultCapabilityRegistry, FixedRetryPolicy, NoopRateLimiter, NoopUsageCollector,
};
pub use observer::{ModelObservation, ModelObserver, ModelStage};
pub use registry::ProviderRegistry;
pub use traits::{
    CapabilityRegistry, ModelCatalog, ModelProvider, ModelRouter, ModelStream, RateLimiter,
    RequestInterceptor, ResponseInterceptor, RetryPolicy, UsageCollector,
};
