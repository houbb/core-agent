use std::sync::Arc;

use core_agent::integrations::PlatformKernelRuntime;
use core_agent::{
    KernelConfig, KernelStatus, PlatformManager, PlatformState, RuntimeKernel, RuntimeStatus,
};

#[tokio::test]
async fn kernel_controls_real_platform_lifecycle_and_configuration() {
    let platform = Arc::new(PlatformManager::builder().build());
    let adapter = Arc::new(PlatformKernelRuntime::new(platform.clone()));
    let kernel = RuntimeKernel::builder().build();
    kernel.register(adapter.clone(), None).await.unwrap();

    assert_eq!(kernel.start().await.unwrap(), KernelStatus::Running);
    assert_eq!(platform.status().unwrap(), PlatformState::Running);
    assert_eq!(
        kernel.runtime_status("platform").unwrap(),
        RuntimeStatus::Running
    );
    let mut config = KernelConfig::new();
    config.insert("environment".into(), serde_json::json!("test"));
    let snapshot = kernel.reload("platform", config).await.unwrap();
    assert_eq!(snapshot.revision, 2);
    assert_eq!(adapter.configuration().unwrap().unwrap(), snapshot);
    assert!(kernel.health().await.unwrap()[0].healthy);

    assert_eq!(kernel.stop().await.unwrap(), KernelStatus::Stopped);
    assert_eq!(platform.status().unwrap(), PlatformState::Stopped);
}
