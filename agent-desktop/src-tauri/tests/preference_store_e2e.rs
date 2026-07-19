use agent_desktop_lib::{DesktopPreferenceStore, PreferenceKind, SavePreferenceRequest};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn preference_create_update_reopen_and_conflict_are_consistent() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("desktop.db");
    let store = DesktopPreferenceStore::new(&path).unwrap();
    let created = store
        .save(
            SavePreferenceRequest {
                key: "layout.main".into(),
                kind: PreferenceKind::Layout,
                value: serde_json::json!({"sidebar": true}),
                expected_version: None,
            },
            "desktop-user",
        )
        .unwrap();
    let updated = store
        .save(
            SavePreferenceRequest {
                key: "layout.main".into(),
                kind: PreferenceKind::Layout,
                value: serde_json::json!({"sidebar": false}),
                expected_version: Some(created.version),
            },
            "desktop-user",
        )
        .unwrap();
    assert_eq!(updated.version, 2);
    assert!(store
        .save(
            SavePreferenceRequest {
                key: "layout.main".into(),
                kind: PreferenceKind::Layout,
                value: serde_json::json!({}),
                expected_version: Some(created.version),
            },
            "desktop-user",
        )
        .is_err());
    drop(store);

    assert_eq!(
        DesktopPreferenceStore::new(&path)
            .unwrap()
            .find("layout.main")
            .unwrap()
            .unwrap(),
        updated
    );
}

#[test]
fn sqlite_schema_has_audit_fields_no_foreign_keys_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("desktop.db");
    let store = DesktopPreferenceStore::new(&path).unwrap();
    store
        .save(
            SavePreferenceRequest {
                key: "theme.current".into(),
                kind: PreferenceKind::Theme,
                value: serde_json::json!({"name":"obsidian-gold"}),
                expected_version: None,
            },
            "desktop-user",
        )
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    let columns = connection
        .prepare("PRAGMA table_info(ui_preference)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for required in [
        "id",
        "create_time",
        "update_time",
        "create_user",
        "update_user",
    ] {
        assert!(columns.iter().any(|column| column == required));
    }
    let foreign_keys = connection
        .prepare("PRAGMA foreign_key_list(ui_preference)")
        .unwrap()
        .query_map([], |_| Ok(()))
        .unwrap()
        .count();
    assert_eq!(foreign_keys, 0);
    connection
        .execute(
            "UPDATE ui_preference SET kind='WINDOW' WHERE preference_key='theme.current'",
            [],
        )
        .unwrap();
    drop(connection);
    assert!(store.find("theme.current").is_err());
}

#[test]
fn preference_rejects_secret_material() {
    let store = DesktopPreferenceStore::open_in_memory().unwrap();
    assert!(store
        .save(
            SavePreferenceRequest {
                key: "settings.server".into(),
                kind: PreferenceKind::Layout,
                value: serde_json::json!({"access_token":"secret"}),
                expected_version: None,
            },
            "desktop-user",
        )
        .is_err());
}
