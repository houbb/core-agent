use core_agent::integrations::platform_visual_descriptor;
use core_agent::VisualRegistry;

#[test]
fn platform_runtime_visual_contract_auto_composes_studio_panels() {
    let registry = VisualRegistry::default();
    registry
        .register(platform_visual_descriptor(), None)
        .unwrap();
    let catalog = registry.catalog().unwrap();
    assert_eq!(catalog.panels.len(), 2);
    assert_eq!(catalog.panels[0].runtime_id, "platform");
    assert!(catalog
        .panels
        .iter()
        .any(|panel| panel.panel.key == "health"));
}
