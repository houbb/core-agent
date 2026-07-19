use std::collections::BTreeSet;
use std::sync::Arc;

use core_agent::integrations::EcosystemExtensionInventory;
use core_agent::{ExtensionManager, InstallationPlan, PackageCoordinate};

#[tokio::test]
async fn marketplace_plan_reports_capabilities_missing_from_real_extension_runtime() {
    let inventory = EcosystemExtensionInventory::new(Arc::new(ExtensionManager::builder().build()));
    let plan = InstallationPlan {
        root: PackageCoordinate {
            key: "rca-agent".into(),
            version: "1.0.0".into(),
        },
        packages: vec![PackageCoordinate {
            key: "rca-agent".into(),
            version: "1.0.0".into(),
        }],
        required_capabilities: BTreeSet::from(["metrics.query".into(), "logs.search".into()]),
    };

    assert_eq!(
        inventory.missing_capabilities(&plan).await.unwrap(),
        vec!["logs.search", "metrics.query"]
    );
}
