use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    ConfigSnapshot, KernelEvent, KernelResult, LifecycleContext, RuntimeDescriptor, RuntimeHealth,
    ServiceRegistry,
};

#[derive(Clone)]
pub struct RuntimeContext {
    pub services: Arc<ServiceRegistry>,
    pub configuration: ConfigSnapshot,
}

#[async_trait]
pub trait ManagedRuntime: Send + Sync {
    fn descriptor(&self) -> RuntimeDescriptor;
    async fn init(&self, context: &RuntimeContext) -> KernelResult<()>;
    async fn start(&self) -> KernelResult<()>;
    async fn stop(&self) -> KernelResult<()>;
    async fn reload(&self, context: &RuntimeContext) -> KernelResult<()>;
    async fn health(&self) -> KernelResult<RuntimeHealth>;
}

pub trait KernelHook: Send + Sync {
    fn before(&self, _context: &LifecycleContext) -> KernelResult<()> {
        Ok(())
    }

    fn after(&self, _context: &LifecycleContext) {}
}

#[async_trait]
pub trait KernelEventSink: Send + Sync {
    async fn emit(&self, event: KernelEvent) -> KernelResult<()>;
}

#[derive(Default)]
pub struct NoopKernelEventSink;

#[async_trait]
impl KernelEventSink for NoopKernelEventSink {
    async fn emit(&self, _event: KernelEvent) -> KernelResult<()> {
        Ok(())
    }
}
