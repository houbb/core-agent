use chrono::Utc;
use core_agent_visual::{
    ActionMethod, FieldKind, PanelKind, VisualAction, VisualDataSource, VisualDescriptor,
    VisualField, VisualPanelDescriptor, VisualRegistry,
};

fn descriptor(runtime: &str) -> VisualDescriptor {
    VisualDescriptor::new(
        runtime,
        "1.0.0",
        vec![VisualPanelDescriptor {
            key: "overview".into(),
            title: "Overview".into(),
            description: "Runtime overview".into(),
            icon: Some("activity".into()),
            kind: PanelKind::Table,
            data_source: VisualDataSource {
                endpoint: format!("/api/{runtime}/overview"),
                refresh_seconds: Some(10),
            },
            fields: vec![VisualField {
                key: "state".into(),
                label: "State".into(),
                kind: FieldKind::Status,
                sortable: true,
                filterable: true,
            }],
            actions: vec![],
        }],
    )
}

#[test]
fn descriptors_register_update_and_compose_deterministically() {
    let registry = VisualRegistry::default();
    registry.register(descriptor("tool"), None).unwrap();
    registry.register(descriptor("memory"), None).unwrap();
    assert_eq!(
        registry
            .catalog()
            .unwrap()
            .panels
            .into_iter()
            .map(|panel| panel.id)
            .collect::<Vec<_>>(),
        vec!["memory/overview", "tool/overview"]
    );

    let mut updated = descriptor("tool");
    updated.revision = 2;
    updated.updated_at = Utc::now();
    updated.panels[0].title = "Tool Overview".into();
    registry.register(updated.clone(), Some(1)).unwrap();
    assert_eq!(registry.find("tool").unwrap(), Some(updated));
}

#[test]
fn revision_conflict_does_not_replace_current_descriptor() {
    let registry = VisualRegistry::default();
    registry.register(descriptor("tool"), None).unwrap();
    let mut stale = descriptor("tool");
    stale.revision = 2;
    stale.panels[0].title = "Stale".into();
    assert!(registry.register(stale, Some(9)).is_err());
    assert_eq!(registry.find("tool").unwrap().unwrap().revision, 1);
}

#[test]
fn descriptor_rejects_remote_traversal_and_unapproved_dangerous_action() {
    let mut value = descriptor("tool");
    value.panels[0].data_source.endpoint = "https://example.com/data".into();
    assert!(value.validate().is_err());

    let mut value = descriptor("tool");
    value.panels[0].actions.push(VisualAction {
        key: "delete".into(),
        label: "Delete".into(),
        method: ActionMethod::Delete,
        endpoint: "/api/tool/delete".into(),
        dangerous: true,
        requires_approval: false,
    });
    assert!(value.validate().is_err());

    value.panels[0].actions[0].requires_approval = true;
    assert!(value.validate().is_ok());
}
