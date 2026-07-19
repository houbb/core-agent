use std::collections::{BTreeMap, BTreeSet};

use core_agent::integrations::{
    kernel_runtime_protocol, marketplace_package_protocol, visual_descriptor_protocol,
};
use core_agent::{
    CapabilitySpec, MarketplacePackage, PackageKind, PanelKind, ProtocolDocument, ProtocolKind,
    ProtocolRegistry, ProtocolSpec, ResourceCoordinate, RuntimeDescriptor, RuntimeVersion,
    VisualDataSource, VisualDescriptor, VisualField, VisualFieldKind, VisualPanelDescriptor,
};

#[test]
fn real_kernel_visual_and_marketplace_descriptors_converge_in_discovery() {
    let registry = ProtocolRegistry::default();
    let capability = ProtocolDocument::new(
        "memory.read",
        "Memory Read",
        "1.0.0",
        ProtocolSpec::Capability(CapabilitySpec {
            permissions: BTreeSet::from(["read".into()]),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
        }),
    );
    registry.register(capability, "system").unwrap();

    let visual = VisualDescriptor::new(
        "memory",
        "1.0.0",
        vec![VisualPanelDescriptor {
            key: "memory".into(),
            title: "Memory".into(),
            description: "Memory facts".into(),
            icon: None,
            kind: PanelKind::Table,
            data_source: VisualDataSource {
                endpoint: "/api/memory/items".into(),
                refresh_seconds: None,
            },
            fields: vec![VisualField {
                key: "content".into(),
                label: "Content".into(),
                kind: VisualFieldKind::Text,
                sortable: false,
                filterable: true,
            }],
            actions: vec![],
        }],
    );
    let ui = visual_descriptor_protocol(&visual);
    registry.register(ui.clone(), "system").unwrap();

    let runtime = kernel_runtime_protocol(
        &RuntimeDescriptor::new("memory", "Memory Runtime", RuntimeVersion::new(1, 0, 0)),
        vec![ResourceCoordinate::new(
            ProtocolKind::Capability,
            "memory.read",
            "1.0.0",
        )],
        vec![],
        vec![ui.coordinate()],
    );
    registry.register(runtime, "system").unwrap();

    let mut package = MarketplacePackage::new(
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        PackageKind::Agent,
        "knowledge-agent",
        "Knowledge Agent",
        "1.0.0",
        "publisher",
    );
    package.required_capabilities.insert("memory.read".into());
    let marketplace = marketplace_package_protocol(
        &package,
        &BTreeMap::from([("memory.read".into(), "1.0.0".into())]),
    )
    .unwrap();
    registry.register(marketplace, "system").unwrap();

    assert_eq!(registry.revision().unwrap(), 4);
    assert_eq!(
        registry
            .discover(&core_agent::DiscoveryQuery {
                kind: None,
                capability: Some("memory.read".into())
            })
            .unwrap()
            .len(),
        3
    );
}
