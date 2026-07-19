use std::collections::{BTreeMap, BTreeSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, RwLock};

use chrono::Utc;
use tokio::sync::Mutex as AsyncMutex;

use crate::{
    ConfigSnapshot, KernelConfig, KernelError, KernelEvent, KernelEventKind, KernelEventSink,
    KernelHook, KernelResult, KernelStatus, LifecycleContext, LifecycleOperation, ManagedRuntime,
    NoopKernelEventSink, RuntimeContext, RuntimeDescriptor, RuntimeHealth, RuntimeStatus,
    ServiceRegistry,
};

struct RuntimeEntry {
    descriptor: RuntimeDescriptor,
    runtime: Arc<dyn ManagedRuntime>,
    status: Mutex<RuntimeStatus>,
}

pub struct RuntimeKernelBuilder {
    services: Arc<ServiceRegistry>,
    events: Arc<dyn KernelEventSink>,
    hooks: Vec<Arc<dyn KernelHook>>,
}

impl Default for RuntimeKernelBuilder {
    fn default() -> Self {
        Self {
            services: Arc::new(ServiceRegistry::default()),
            events: Arc::new(NoopKernelEventSink),
            hooks: Vec::new(),
        }
    }
}

impl RuntimeKernelBuilder {
    pub fn services(mut self, value: Arc<ServiceRegistry>) -> Self {
        self.services = value;
        self
    }

    pub fn event_sink(mut self, value: Arc<dyn KernelEventSink>) -> Self {
        self.events = value;
        self
    }

    pub fn hook(mut self, value: Arc<dyn KernelHook>) -> Self {
        self.hooks.push(value);
        self
    }

    pub fn build(self) -> RuntimeKernel {
        RuntimeKernel {
            runtimes: RwLock::new(BTreeMap::new()),
            configurations: RwLock::new(BTreeMap::new()),
            services: self.services,
            events: self.events,
            hooks: self.hooks,
            status: Mutex::new(KernelStatus::Created),
            operation: AsyncMutex::new(()),
        }
    }
}

pub struct RuntimeKernel {
    runtimes: RwLock<BTreeMap<String, Arc<RuntimeEntry>>>,
    configurations: RwLock<BTreeMap<String, ConfigSnapshot>>,
    services: Arc<ServiceRegistry>,
    events: Arc<dyn KernelEventSink>,
    hooks: Vec<Arc<dyn KernelHook>>,
    status: Mutex<KernelStatus>,
    operation: AsyncMutex<()>,
}

impl RuntimeKernel {
    pub fn builder() -> RuntimeKernelBuilder {
        RuntimeKernelBuilder::default()
    }

    pub fn services(&self) -> Arc<ServiceRegistry> {
        self.services.clone()
    }

    pub fn status(&self) -> KernelResult<KernelStatus> {
        self.status
            .lock()
            .map(|status| *status)
            .map_err(|_| KernelError::Internal("Kernel status lock poisoned".into()))
    }

    pub async fn register(
        &self,
        runtime: Arc<dyn ManagedRuntime>,
        configuration: Option<KernelConfig>,
    ) -> KernelResult<RuntimeDescriptor> {
        let _operation = self.operation.lock().await;
        if self.status()? == KernelStatus::Running {
            return Err(KernelError::InvalidState(
                "cannot register a Runtime while Kernel is Running".into(),
            ));
        }
        let descriptor = runtime.descriptor();
        descriptor.validate()?;
        let mut snapshot = ConfigSnapshot::empty(descriptor.id.clone());
        if let Some(values) = configuration {
            snapshot.values = values;
        }
        snapshot.validate()?;
        {
            let mut runtimes = self
                .runtimes
                .write()
                .map_err(|_| KernelError::Internal("Runtime registry lock poisoned".into()))?;
            if runtimes.contains_key(&descriptor.id) {
                return Err(KernelError::Duplicate(descriptor.id));
            }
            runtimes.insert(
                descriptor.id.clone(),
                Arc::new(RuntimeEntry {
                    descriptor: descriptor.clone(),
                    runtime,
                    status: Mutex::new(RuntimeStatus::Registered),
                }),
            );
        }
        self.configurations
            .write()
            .map_err(|_| KernelError::Internal("configuration lock poisoned".into()))?
            .insert(descriptor.id.clone(), snapshot);
        self.emit(KernelEvent::new(
            &descriptor.id,
            KernelEventKind::Registered,
            format!("Runtime {} registered", descriptor.version),
        ))
        .await;
        Ok(descriptor)
    }

    pub fn descriptors(&self) -> KernelResult<Vec<RuntimeDescriptor>> {
        Ok(self
            .runtimes
            .read()
            .map_err(|_| KernelError::Internal("Runtime registry lock poisoned".into()))?
            .values()
            .map(|entry| entry.descriptor.clone())
            .collect())
    }

    pub fn runtime_status(&self, runtime_id: &str) -> KernelResult<RuntimeStatus> {
        let entry = self.entry(runtime_id)?;
        let status = *entry
            .status
            .lock()
            .map_err(|_| KernelError::Internal("Runtime status lock poisoned".into()))?;
        Ok(status)
    }

    pub fn configuration(&self, runtime_id: &str) -> KernelResult<ConfigSnapshot> {
        self.configurations
            .read()
            .map_err(|_| KernelError::Internal("configuration lock poisoned".into()))?
            .get(runtime_id)
            .cloned()
            .ok_or_else(|| KernelError::NotFound(runtime_id.into()))
    }

    pub fn startup_order(&self) -> KernelResult<Vec<String>> {
        let descriptors = self
            .runtimes
            .read()
            .map_err(|_| KernelError::Internal("Runtime registry lock poisoned".into()))?
            .iter()
            .map(|(key, entry)| (key.clone(), entry.descriptor.clone()))
            .collect::<BTreeMap<_, _>>();
        validate_dependency_versions(&descriptors)?;
        topological_order(&descriptors)
    }

    pub async fn start(&self) -> KernelResult<KernelStatus> {
        let _operation = self.operation.lock().await;
        if self.status()? == KernelStatus::Running {
            return Ok(KernelStatus::Running);
        }
        let order = self.startup_order()?;
        let mut started = Vec::new();
        for runtime_id in order {
            let entry = self.entry(&runtime_id)?;
            let current = self.entry_status(&entry)?;
            if current == RuntimeStatus::Running {
                continue;
            }
            if current != RuntimeStatus::Initialized {
                if let Err(error) = self
                    .run_operation(&entry, LifecycleOperation::Init, |runtime, context| {
                        Box::pin(runtime.init(context))
                    })
                    .await
                {
                    self.rollback(&runtime_id, &started).await;
                    self.set_kernel_status(KernelStatus::Failed)?;
                    return Err(error);
                }
                self.set_entry_status(&entry, RuntimeStatus::Initialized)?;
                self.emit(KernelEvent::new(
                    &runtime_id,
                    KernelEventKind::Initialized,
                    "Runtime initialized",
                ))
                .await;
            }
            if let Err(error) = self
                .run_operation(&entry, LifecycleOperation::Start, |runtime, _| {
                    Box::pin(runtime.start())
                })
                .await
            {
                self.set_entry_status(&entry, RuntimeStatus::Failed)?;
                self.emit(KernelEvent::new(
                    &runtime_id,
                    KernelEventKind::Failed,
                    error.to_string(),
                ))
                .await;
                self.rollback(&runtime_id, &started).await;
                self.set_kernel_status(KernelStatus::Failed)?;
                return Err(error);
            }
            self.set_entry_status(&entry, RuntimeStatus::Running)?;
            started.push(runtime_id.clone());
            self.emit(KernelEvent::new(
                runtime_id,
                KernelEventKind::Started,
                "Runtime started",
            ))
            .await;
        }
        self.set_kernel_status(KernelStatus::Running)?;
        Ok(KernelStatus::Running)
    }

    pub async fn stop(&self) -> KernelResult<KernelStatus> {
        let _operation = self.operation.lock().await;
        let mut order = self.startup_order()?;
        order.reverse();
        let mut first_error = None;
        for runtime_id in order {
            let entry = self.entry(&runtime_id)?;
            if self.entry_status(&entry)? != RuntimeStatus::Running {
                continue;
            }
            match self
                .run_operation(&entry, LifecycleOperation::Stop, |runtime, _| {
                    Box::pin(runtime.stop())
                })
                .await
            {
                Ok(()) => {
                    self.set_entry_status(&entry, RuntimeStatus::Stopped)?;
                    self.emit(KernelEvent::new(
                        runtime_id,
                        KernelEventKind::Stopped,
                        "Runtime stopped",
                    ))
                    .await;
                }
                Err(error) => {
                    self.set_entry_status(&entry, RuntimeStatus::Failed)?;
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if let Some(error) = first_error {
            self.set_kernel_status(KernelStatus::Failed)?;
            Err(error)
        } else {
            self.set_kernel_status(KernelStatus::Stopped)?;
            Ok(KernelStatus::Stopped)
        }
    }

    pub async fn reload(
        &self,
        runtime_id: &str,
        values: KernelConfig,
    ) -> KernelResult<ConfigSnapshot> {
        let _operation = self.operation.lock().await;
        let entry = self.entry(runtime_id)?;
        if self.entry_status(&entry)? != RuntimeStatus::Running {
            return Err(KernelError::InvalidState(format!(
                "Runtime {runtime_id} is not Running"
            )));
        }
        let current = self.configuration(runtime_id)?;
        let proposed = ConfigSnapshot {
            runtime_id: runtime_id.into(),
            revision: current.revision.saturating_add(1),
            values,
        };
        proposed.validate()?;
        self.run_operation_with_context(
            &entry,
            LifecycleOperation::Reload,
            proposed.clone(),
            |runtime, context| Box::pin(runtime.reload(context)),
        )
        .await?;
        self.configurations
            .write()
            .map_err(|_| KernelError::Internal("configuration lock poisoned".into()))?
            .insert(runtime_id.into(), proposed.clone());
        self.emit(KernelEvent::new(
            runtime_id,
            KernelEventKind::Reloaded,
            format!("Configuration revision {} applied", proposed.revision),
        ))
        .await;
        Ok(proposed)
    }

    pub async fn health(&self) -> KernelResult<Vec<RuntimeHealth>> {
        let entries = self
            .runtimes
            .read()
            .map_err(|_| KernelError::Internal("Runtime registry lock poisoned".into()))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut health = Vec::with_capacity(entries.len());
        for entry in entries {
            if self.entry_status(&entry)? != RuntimeStatus::Running {
                health.push(RuntimeHealth {
                    runtime_id: entry.descriptor.id.clone(),
                    healthy: false,
                    message: "Runtime is not Running".into(),
                    checked_at: Utc::now(),
                });
                continue;
            }
            match entry.runtime.health().await {
                Ok(mut item) => {
                    item.runtime_id = entry.descriptor.id.clone();
                    health.push(item);
                }
                Err(error) => health.push(RuntimeHealth {
                    runtime_id: entry.descriptor.id.clone(),
                    healthy: false,
                    message: error.to_string(),
                    checked_at: Utc::now(),
                }),
            }
        }
        Ok(health)
    }

    async fn run_operation<F>(
        &self,
        entry: &Arc<RuntimeEntry>,
        operation: LifecycleOperation,
        action: F,
    ) -> KernelResult<()>
    where
        F: for<'a> FnOnce(
            &'a dyn ManagedRuntime,
            &'a RuntimeContext,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = KernelResult<()>> + Send + 'a>,
        >,
    {
        let configuration = self.configuration(&entry.descriptor.id)?;
        self.run_operation_with_context(entry, operation, configuration, action)
            .await
    }

    async fn run_operation_with_context<F>(
        &self,
        entry: &Arc<RuntimeEntry>,
        operation: LifecycleOperation,
        configuration: ConfigSnapshot,
        action: F,
    ) -> KernelResult<()>
    where
        F: for<'a> FnOnce(
            &'a dyn ManagedRuntime,
            &'a RuntimeContext,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = KernelResult<()>> + Send + 'a>,
        >,
    {
        let lifecycle = LifecycleContext {
            runtime_id: entry.descriptor.id.clone(),
            operation,
            config_revision: configuration.revision,
        };
        self.before(&lifecycle)?;
        let context = RuntimeContext {
            services: self.services.clone(),
            configuration,
        };
        action(entry.runtime.as_ref(), &context)
            .await
            .map_err(|error| KernelError::Lifecycle {
                runtime: entry.descriptor.id.clone(),
                operation: operation.as_str().into(),
                message: error.to_string(),
            })?;
        self.after(&lifecycle);
        Ok(())
    }

    async fn rollback(&self, failing_runtime: &str, started: &[String]) {
        let mut runtimes = started.to_vec();
        runtimes.push(failing_runtime.into());
        runtimes.reverse();
        for runtime_id in runtimes {
            let Ok(entry) = self.entry(&runtime_id) else {
                continue;
            };
            let configuration = self.configuration(&runtime_id).ok();
            if let Some(configuration) = configuration {
                let _ = self
                    .run_operation_with_context(
                        &entry,
                        LifecycleOperation::Stop,
                        configuration,
                        |runtime, _| Box::pin(runtime.stop()),
                    )
                    .await;
            }
            let _ = self.set_entry_status(&entry, RuntimeStatus::Stopped);
        }
    }

    fn before(&self, context: &LifecycleContext) -> KernelResult<()> {
        for hook in &self.hooks {
            catch_unwind(AssertUnwindSafe(|| hook.before(context)))
                .map_err(|_| KernelError::Hook("before hook panicked".into()))??;
        }
        Ok(())
    }

    fn after(&self, context: &LifecycleContext) {
        for hook in &self.hooks {
            let _ = catch_unwind(AssertUnwindSafe(|| hook.after(context)));
        }
    }

    async fn emit(&self, event: KernelEvent) {
        let _ = self.events.emit(event).await;
    }

    fn entry(&self, runtime_id: &str) -> KernelResult<Arc<RuntimeEntry>> {
        self.runtimes
            .read()
            .map_err(|_| KernelError::Internal("Runtime registry lock poisoned".into()))?
            .get(runtime_id)
            .cloned()
            .ok_or_else(|| KernelError::NotFound(runtime_id.into()))
    }

    fn entry_status(&self, entry: &RuntimeEntry) -> KernelResult<RuntimeStatus> {
        entry
            .status
            .lock()
            .map(|status| *status)
            .map_err(|_| KernelError::Internal("Runtime status lock poisoned".into()))
    }

    fn set_entry_status(&self, entry: &RuntimeEntry, value: RuntimeStatus) -> KernelResult<()> {
        *entry
            .status
            .lock()
            .map_err(|_| KernelError::Internal("Runtime status lock poisoned".into()))? = value;
        Ok(())
    }

    fn set_kernel_status(&self, value: KernelStatus) -> KernelResult<()> {
        *self
            .status
            .lock()
            .map_err(|_| KernelError::Internal("Kernel status lock poisoned".into()))? = value;
        Ok(())
    }
}

fn validate_dependency_versions(
    descriptors: &BTreeMap<String, RuntimeDescriptor>,
) -> KernelResult<()> {
    for descriptor in descriptors.values() {
        for dependency in &descriptor.dependencies {
            let Some(actual) = descriptors.get(&dependency.runtime_id) else {
                if dependency.optional {
                    continue;
                }
                return Err(KernelError::Dependency(format!(
                    "Runtime {} requires missing {}",
                    descriptor.id, dependency.runtime_id
                )));
            };
            if !actual.version.satisfies(dependency.minimum_version) {
                return Err(KernelError::Version(format!(
                    "Runtime {} requires {} >= {}, found {}",
                    descriptor.id,
                    dependency.runtime_id,
                    dependency.minimum_version,
                    actual.version
                )));
            }
        }
    }
    Ok(())
}

fn topological_order(
    descriptors: &BTreeMap<String, RuntimeDescriptor>,
) -> KernelResult<Vec<String>> {
    fn visit(
        runtime_id: &str,
        descriptors: &BTreeMap<String, RuntimeDescriptor>,
        visiting: &mut BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        order: &mut Vec<String>,
    ) -> KernelResult<()> {
        if visited.contains(runtime_id) {
            return Ok(());
        }
        if !visiting.insert(runtime_id.into()) {
            return Err(KernelError::Dependency(format!(
                "dependency cycle includes {runtime_id}"
            )));
        }
        let descriptor = descriptors
            .get(runtime_id)
            .ok_or_else(|| KernelError::NotFound(runtime_id.into()))?;
        for dependency in &descriptor.dependencies {
            if descriptors.contains_key(&dependency.runtime_id) {
                visit(
                    &dependency.runtime_id,
                    descriptors,
                    visiting,
                    visited,
                    order,
                )?;
            }
        }
        visiting.remove(runtime_id);
        visited.insert(runtime_id.into());
        order.push(runtime_id.into());
        Ok(())
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut order = Vec::with_capacity(descriptors.len());
    for runtime_id in descriptors.keys() {
        visit(
            runtime_id,
            descriptors,
            &mut visiting,
            &mut visited,
            &mut order,
        )?;
    }
    Ok(order)
}
