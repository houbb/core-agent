use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use core_agent_kernel::{
    KernelConfig, KernelError, KernelEvent, KernelEventSink, KernelResult, KernelStatus,
    ManagedRuntime, RuntimeContext, RuntimeDependency, RuntimeDescriptor, RuntimeHealth,
    RuntimeKernel, RuntimeStatus, RuntimeVersion, ServiceRegistry,
};

struct RecordingRuntime {
    descriptor: RuntimeDescriptor,
    calls: Arc<Mutex<Vec<String>>>,
    fail_start: bool,
    revision: Mutex<u64>,
}

impl RecordingRuntime {
    fn new(id: &str, calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            descriptor: RuntimeDescriptor::new(id, id, RuntimeVersion::new(1, 0, 0)),
            calls,
            fail_start: false,
            revision: Mutex::new(0),
        }
    }

    fn depends_on(mut self, id: &str) -> Self {
        self.descriptor
            .dependencies
            .push(RuntimeDependency::required(
                id,
                RuntimeVersion::new(1, 0, 0),
            ));
        self
    }
}

#[async_trait]
impl ManagedRuntime for RecordingRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        self.descriptor.clone()
    }

    async fn init(&self, context: &RuntimeContext) -> KernelResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:init", self.descriptor.id));
        *self.revision.lock().unwrap() = context.configuration.revision;
        Ok(())
    }

    async fn start(&self) -> KernelResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:start", self.descriptor.id));
        if self.fail_start {
            Err(KernelError::Internal("injected start failure".into()))
        } else {
            Ok(())
        }
    }

    async fn stop(&self) -> KernelResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:stop", self.descriptor.id));
        Ok(())
    }

    async fn reload(&self, context: &RuntimeContext) -> KernelResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{}:reload", self.descriptor.id));
        *self.revision.lock().unwrap() = context.configuration.revision;
        Ok(())
    }

    async fn health(&self) -> KernelResult<RuntimeHealth> {
        Ok(RuntimeHealth::healthy(&self.descriptor.id))
    }
}

#[derive(Default)]
struct RecordingEvents(Mutex<Vec<KernelEvent>>);

#[async_trait]
impl KernelEventSink for RecordingEvents {
    async fn emit(&self, event: KernelEvent) -> KernelResult<()> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

#[tokio::test]
async fn dependencies_start_in_order_reload_and_stop_in_reverse() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let events = Arc::new(RecordingEvents::default());
    let kernel = RuntimeKernel::builder().event_sink(events.clone()).build();
    let base = Arc::new(RecordingRuntime::new("base", calls.clone()));
    let application =
        Arc::new(RecordingRuntime::new("application", calls.clone()).depends_on("base"));
    kernel.register(application, None).await.unwrap();
    kernel.register(base, None).await.unwrap();

    assert_eq!(kernel.startup_order().unwrap(), vec!["base", "application"]);
    assert_eq!(kernel.start().await.unwrap(), KernelStatus::Running);
    let mut config = KernelConfig::new();
    config.insert("mode".into(), serde_json::json!("safe"));
    assert_eq!(
        kernel.reload("application", config).await.unwrap().revision,
        2
    );
    assert!(kernel
        .health()
        .await
        .unwrap()
        .iter()
        .all(|item| item.healthy));
    assert_eq!(kernel.stop().await.unwrap(), KernelStatus::Stopped);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "base:init",
            "base:start",
            "application:init",
            "application:start",
            "application:reload",
            "application:stop",
            "base:stop",
        ]
    );
    assert!(events.0.lock().unwrap().len() >= 8);
}

#[tokio::test]
async fn failed_start_rolls_back_failing_and_started_runtimes() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let kernel = RuntimeKernel::builder().build();
    let base = Arc::new(RecordingRuntime::new("base", calls.clone()));
    let mut application = RecordingRuntime::new("application", calls.clone()).depends_on("base");
    application.fail_start = true;
    kernel.register(base, None).await.unwrap();
    kernel.register(Arc::new(application), None).await.unwrap();

    assert!(kernel.start().await.is_err());
    assert_eq!(kernel.status().unwrap(), KernelStatus::Failed);
    assert_eq!(
        kernel.runtime_status("base").unwrap(),
        RuntimeStatus::Stopped
    );
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            "base:init",
            "base:start",
            "application:init",
            "application:start",
            "application:stop",
            "base:stop",
        ]
    );
}

#[tokio::test]
async fn missing_dependency_and_cycle_fail_before_lifecycle_side_effects() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let missing = RuntimeKernel::builder().build();
    missing
        .register(
            Arc::new(RecordingRuntime::new("application", calls.clone()).depends_on("base")),
            None,
        )
        .await
        .unwrap();
    assert!(missing.start().await.is_err());
    assert!(calls.lock().unwrap().is_empty());

    let cycle = RuntimeKernel::builder().build();
    cycle
        .register(
            Arc::new(RecordingRuntime::new("a", calls.clone()).depends_on("b")),
            None,
        )
        .await
        .unwrap();
    cycle
        .register(
            Arc::new(RecordingRuntime::new("b", calls.clone()).depends_on("a")),
            None,
        )
        .await
        .unwrap();
    assert!(cycle.start().await.is_err());
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn incompatible_version_fails_before_init() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let kernel = RuntimeKernel::builder().build();
    let base = RecordingRuntime::new("base", calls.clone());
    let mut application = RecordingRuntime::new("application", calls.clone());
    application
        .descriptor
        .dependencies
        .push(RuntimeDependency::required(
            "base",
            RuntimeVersion::new(2, 0, 0),
        ));
    kernel.register(Arc::new(base), None).await.unwrap();
    kernel.register(Arc::new(application), None).await.unwrap();
    assert!(matches!(kernel.start().await, Err(KernelError::Version(_))));
    assert!(calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn services_are_shared_and_sensitive_configuration_is_rejected() {
    let services = Arc::new(ServiceRegistry::default());
    services
        .register("platform.name", Arc::new(String::from("core-agent")))
        .unwrap();
    let kernel = RuntimeKernel::builder().services(services.clone()).build();
    assert_eq!(
        services
            .resolve::<String>("platform.name")
            .unwrap()
            .as_str(),
        "core-agent"
    );
    let mut config = BTreeMap::new();
    config.insert("nested".into(), serde_json::json!({"password":"secret"}));
    assert!(kernel
        .register(
            Arc::new(RecordingRuntime::new(
                "base",
                Arc::new(Mutex::new(Vec::new())),
            )),
            Some(config),
        )
        .await
        .is_err());
    assert!(kernel.descriptors().unwrap().is_empty());
}
