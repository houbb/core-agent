use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use core_agent_extension::{
    CapabilityInvocation, CapabilityManifest, CapabilityResult, ExtensionError, ExtensionHost,
    ExtensionLoadHandle, ExtensionManager, ExtensionManifest, ExtensionObservation,
    ExtensionObserver, ExtensionPermission, ExtensionResult, ExtensionState,
    InstallExtensionRequest, Provider, ProviderKind, ProviderManifest, SqliteExtensionStore,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::tempdir;

struct ScriptHost {
    starts: AtomicUsize,
    stops: AtomicUsize,
    calls: AtomicUsize,
    failures: Mutex<VecDeque<String>>,
}

impl ScriptHost {
    fn new(failures: Vec<String>) -> Self {
        Self {
            starts: AtomicUsize::new(0),
            stops: AtomicUsize::new(0),
            calls: AtomicUsize::new(0),
            failures: Mutex::new(failures.into()),
        }
    }
}

#[async_trait]
impl ExtensionHost for ScriptHost {
    async fn start(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn execute(
        &self,
        _handle: &ExtensionLoadHandle,
        provider: &Provider,
        invocation: &CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(message) = self.failures.lock().unwrap().pop_front() {
            return Err(ExtensionError::Host(message));
        }
        Ok(CapabilityResult {
            request_id: invocation.request_id,
            provider_id: provider.id,
            summary: "capability completed".into(),
            output: serde_json::json!({"ok": true}),
            completed_at: Utc::now(),
        })
    }
}

fn manifest(version: &str) -> ExtensionManifest {
    ExtensionManifest {
        key: "git".into(),
        name: "Git Extension".into(),
        version: version.into(),
        description: "Local Git capability".into(),
        entrypoint: "git-extension".into(),
        requested_permissions: BTreeSet::new(),
        capabilities: vec![CapabilityManifest {
            key: "git.status".into(),
            version: version.into(),
            name: "Git Status".into(),
            permissions: BTreeSet::new(),
            metadata: BTreeMap::new(),
        }],
        providers: vec![ProviderManifest {
            key: "local-git".into(),
            kind: ProviderKind::Local,
            capabilities: ["git.status".into()].into_iter().collect(),
            priority: 0,
            config: serde_json::Value::Null,
            metadata: BTreeMap::new(),
        }],
        metadata: BTreeMap::new(),
    }
}

fn artifact(directory: &tempfile::TempDir) -> (String, String) {
    let path = directory.path().join("extension.bin");
    std::fs::write(&path, b"verified extension artifact").unwrap();
    let checksum = format!("{:x}", Sha256::digest(b"verified extension artifact"));
    (format!("file:{}", path.display()), checksum)
}

fn install_request(directory: &tempfile::TempDir, version: &str) -> InstallExtensionRequest {
    let (source_uri, checksum) = artifact(directory);
    InstallExtensionRequest {
        manifest: manifest(version),
        source_uri,
        checksum,
        actor: "operator".into(),
    }
}

#[tokio::test]
async fn local_extension_lifecycle_executes_and_upgrades_offline() {
    let directory = tempdir().unwrap();
    let host = Arc::new(ScriptHost::new(Vec::new()));
    let manager = ExtensionManager::builder().host(host.clone()).build();
    let installed = manager
        .install(install_request(&directory, "1.0.0"))
        .await
        .unwrap();
    assert_eq!(installed.state, ExtensionState::Installed);
    manager.load(installed.id, "operator").await.unwrap();
    manager.enable(installed.id, "operator").await.unwrap();
    let result = manager
        .execute(CapabilityInvocation::new(
            "git.status",
            serde_json::json!({"path": "."}),
            "operator",
        ))
        .await
        .unwrap();
    assert_eq!(result.output, serde_json::json!({"ok": true}));
    assert_eq!(host.calls.load(Ordering::SeqCst), 1);
    let disabled = manager.disable(installed.id, "operator").await.unwrap();
    assert_eq!(disabled.state, ExtensionState::Disabled);
    let upgraded = manager
        .upgrade(installed.id, install_request(&directory, "2.0.0"))
        .await
        .unwrap();
    assert_eq!(upgraded.current_version, "2.0.0");
    assert_eq!(manager.list_manifests(installed.id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn unknown_host_outcome_stays_running_and_resumes_same_invocation() {
    let directory = tempdir().unwrap();
    let host = Arc::new(ScriptHost::new(vec!["connection lost".into()]));
    let manager = ExtensionManager::builder().host(host.clone()).build();
    let extension = manager
        .install(install_request(&directory, "1.0.0"))
        .await
        .unwrap();
    manager.load(extension.id, "operator").await.unwrap();
    manager.enable(extension.id, "operator").await.unwrap();
    let invocation =
        CapabilityInvocation::new("git.status", serde_json::json!({"path": "."}), "operator");
    assert!(matches!(
        manager.execute(invocation.clone()).await,
        Err(ExtensionError::OutcomeUnknown(_))
    ));
    assert_eq!(
        manager.find(extension.id).await.unwrap().unwrap().state,
        ExtensionState::Running
    );
    let result = manager.resume(invocation).await.unwrap();
    assert_eq!(result.summary, "capability completed");
    assert_eq!(host.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn default_policy_denies_manifest_permissions_before_persistence() {
    let directory = tempdir().unwrap();
    let manager = ExtensionManager::builder().build();
    let mut request = install_request(&directory, "1.0.0");
    request
        .manifest
        .requested_permissions
        .insert(ExtensionPermission::Network);
    request.manifest.capabilities[0]
        .permissions
        .insert(ExtensionPermission::Network);
    assert!(matches!(
        manager.install(request).await,
        Err(ExtensionError::Denied(_))
    ));
    assert!(manager.list().await.unwrap().is_empty());
}

struct PanickingObserver;
impl ExtensionObserver for PanickingObserver {
    fn on_observation(&self, _observation: &ExtensionObservation) {
        panic!("observer failed")
    }
}

#[tokio::test]
async fn observer_panic_does_not_change_capability_result() {
    let directory = tempdir().unwrap();
    let manager = ExtensionManager::builder()
        .host(Arc::new(ScriptHost::new(Vec::new())))
        .observer(Arc::new(PanickingObserver))
        .build();
    let extension = manager
        .install(install_request(&directory, "1.0.0"))
        .await
        .unwrap();
    manager.load(extension.id, "operator").await.unwrap();
    manager.enable(extension.id, "operator").await.unwrap();
    assert!(manager
        .execute(CapabilityInvocation::new(
            "git.status",
            serde_json::Value::Null,
            "operator",
        ))
        .await
        .is_ok());
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("extension.db");
    let manager = ExtensionManager::builder()
        .store(Arc::new(SqliteExtensionStore::new(&path).unwrap()))
        .host(Arc::new(ScriptHost::new(Vec::new())))
        .build();
    let extension = manager
        .install(install_request(&directory, "1.0.0"))
        .await
        .unwrap();
    manager.load(extension.id, "operator").await.unwrap();
    manager.enable(extension.id, "operator").await.unwrap();
    drop(manager);

    let reopened = SqliteExtensionStore::new(&path).unwrap();
    assert_eq!(
        core_agent_extension::ExtensionStore::find_extension(&reopened, extension.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ExtensionState::Enabled
    );
    let connection = Connection::open(&path).unwrap();
    for table in [
        "extension",
        "extension_manifest",
        "extension_state",
        "capability",
        "provider",
    ] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        for required in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.contains(required), "{table} lacks {required}");
        }
        let foreign_keys: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM pragma_foreign_key_list('{table}')"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(foreign_keys, 0);
    }
    connection
        .execute(
            "UPDATE extension SET state='DISABLED' WHERE id=?1",
            [extension.id.to_string()],
        )
        .unwrap();
    assert!(matches!(
        core_agent_extension::ExtensionStore::find_extension(&reopened, extension.id).await,
        Err(ExtensionError::Validation(_))
    ));
}
