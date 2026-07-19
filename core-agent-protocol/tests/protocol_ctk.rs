use std::collections::BTreeSet;

use core_agent_protocol::{
    CapabilitySpec, CompatibilityTestKit, DiscoveryQuery, EventSpec, ProtocolDocument,
    ProtocolError, ProtocolKind, ProtocolRegistry, ProtocolSpec, ProtocolVersion,
    ResourceCoordinate, RuntimeSpec, UiFieldSpec, UiPanelSpec, UiSpec, WorkflowEdge, WorkflowNode,
    WorkflowSpec,
};

fn capability(key: &str) -> ProtocolDocument {
    ProtocolDocument::new(
        key,
        "Capability",
        "1.0.0",
        ProtocolSpec::Capability(CapabilitySpec {
            permissions: BTreeSet::from(["read".into()]),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
        }),
    )
}

fn ui() -> ProtocolDocument {
    ProtocolDocument::new(
        "memory.ui",
        "Memory UI",
        "1.0.0",
        ProtocolSpec::Ui(UiSpec {
            panels: vec![UiPanelSpec {
                key: "memory".into(),
                title: "Memory".into(),
                panel_type: "table".into(),
                data_endpoint: "/api/memory/items".into(),
                fields: vec![UiFieldSpec {
                    key: "content".into(),
                    label: "Content".into(),
                    value_type: "text".into(),
                }],
            }],
        }),
    )
}

#[test]
fn typed_documents_round_trip_and_discover_by_capability() {
    let registry = ProtocolRegistry::default();
    let capability = registry
        .register(capability("memory.read"), "system")
        .unwrap();
    let ui = registry.register(ui(), "system").unwrap();
    let runtime = ProtocolDocument::new(
        "memory",
        "Memory Runtime",
        "1.0.0",
        ProtocolSpec::Runtime(RuntimeSpec {
            lifecycle_endpoint: "/api/protocol/runtimes/memory/lifecycle".into(),
            health_endpoint: "/api/protocol/runtimes/memory/health".into(),
            event_endpoint: "/api/protocol/runtimes/memory/events".into(),
            capabilities: vec![capability.document.coordinate()],
            events: vec![],
            ui: vec![ui.document.coordinate()],
        }),
    );
    registry.register(runtime, "system").unwrap();

    let found = registry
        .discover(&DiscoveryQuery {
            kind: Some(ProtocolKind::Runtime),
            capability: Some("memory.read".into()),
        })
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].document.key, "memory");
    let yaml = serde_yaml::to_string(&found[0].document).unwrap();
    let restored: ProtocolDocument = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(restored, found[0].document);
}

#[test]
fn registry_is_idempotent_but_rejects_same_version_content_drift() {
    let registry = ProtocolRegistry::default();
    let first = registry
        .register(capability("tool.read"), "system")
        .unwrap();
    let replay = registry
        .register(capability("tool.read"), "system")
        .unwrap();
    assert_eq!(first.registry_revision, replay.registry_revision);
    let mut changed = capability("tool.read");
    if let ProtocolSpec::Capability(spec) = &mut changed.spec {
        spec.permissions.insert("write".into());
    }
    assert!(matches!(
        registry.register(changed, "system"),
        Err(ProtocolError::Conflict(_))
    ));
}

#[test]
fn ctk_rejects_future_contract_unsafe_endpoint_and_sensitive_schema() {
    let mut future = ui();
    future.contract_version = ProtocolVersion {
        major: 1,
        minor: 0,
        patch: 0,
    };
    assert!(!CompatibilityTestKit::check(&future).compatible);
    if let ProtocolSpec::Ui(spec) = &mut future.spec {
        spec.panels[0].data_endpoint = "https://evil.invalid/script".into();
    }
    let codes = CompatibilityTestKit::check(&future)
        .issues
        .into_iter()
        .map(|item| item.code)
        .collect::<BTreeSet<_>>();
    assert!(codes.contains("UNSUPPORTED_CONTRACT"));
    assert!(codes.contains("INVALID_SPEC"));

    let mut unsafe_schema = capability("unsafe");
    if let ProtocolSpec::Capability(spec) = &mut unsafe_schema.spec {
        spec.input_schema = serde_json::json!({"api_key": {"type": "string"}});
    }
    assert!(!CompatibilityTestKit::check(&unsafe_schema).compatible);
}

#[test]
fn registry_rejects_missing_exact_reference() {
    let registry = ProtocolRegistry::default();
    let runtime = ProtocolDocument::new(
        "missing-runtime",
        "Missing Runtime",
        "1.0.0",
        ProtocolSpec::Runtime(RuntimeSpec {
            lifecycle_endpoint: "/api/protocol/runtimes/missing/lifecycle".into(),
            health_endpoint: "/api/protocol/runtimes/missing/health".into(),
            event_endpoint: "/api/protocol/runtimes/missing/events".into(),
            capabilities: vec![ResourceCoordinate::new(
                ProtocolKind::Capability,
                "missing",
                "1.0.0",
            )],
            events: vec![],
            ui: vec![],
        }),
    );
    assert!(matches!(
        registry.register(runtime, "system"),
        Err(ProtocolError::NotFound(_))
    ));
}

#[test]
fn workflow_and_event_specs_are_structurally_checked() {
    let event = ProtocolDocument::new(
        "agent.started",
        "Agent Started",
        "1.0.0",
        ProtocolSpec::Event(EventSpec {
            category: "agent.lifecycle".into(),
            payload_schema: serde_json::json!({"type": "object"}),
        }),
    );
    assert!(CompatibilityTestKit::check(&event).compatible);
    let workflow = ProtocolDocument::new(
        "coding",
        "Coding",
        "1.0.0",
        ProtocolSpec::Workflow(WorkflowSpec {
            trigger: "chat".into(),
            nodes: vec![WorkflowNode {
                key: "plan".into(),
                kind: "planner".into(),
            }],
            edges: vec![WorkflowEdge {
                from: "plan".into(),
                to: "unknown".into(),
            }],
        }),
    );
    assert!(!CompatibilityTestKit::check(&workflow).compatible);
}
