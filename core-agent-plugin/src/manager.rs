use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use core_agent_extension::{
    CapabilityManifest, Extension, ExtensionManager, ExtensionManagerBuilder, ExtensionManifest,
    InstallExtensionRequest, ProviderKind, ProviderManifest,
};

use crate::domain::{Plugin, PluginManifest, PluginState};
use crate::error::{PluginError, PluginResult};

pub struct PluginManagerBuilder {
    extension_manager: Option<Arc<ExtensionManager>>,
}

impl Default for PluginManagerBuilder {
    fn default() -> Self {
        Self {
            extension_manager: None,
        }
    }
}

impl PluginManagerBuilder {
    pub fn extension_manager(mut self, value: Arc<ExtensionManager>) -> Self {
        self.extension_manager = Some(value);
        self
    }

    pub fn build(self) -> PluginManager {
        PluginManager {
            extension_manager: self.extension_manager.unwrap_or_else(|| {
                Arc::new(ExtensionManagerBuilder::default().build())
            }),
            plugins: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

pub struct PluginManager {
    extension_manager: Arc<ExtensionManager>,
    plugins: std::sync::RwLock<HashMap<Uuid, Plugin>>,
}

impl PluginManager {
    pub fn builder() -> PluginManagerBuilder {
        PluginManagerBuilder::default()
    }

    /// Install a plugin from a manifest.
    pub async fn install(&self, manifest: PluginManifest, actor: &str) -> PluginResult<Plugin> {
        manifest.validate()?;
        let plugin = Plugin::install(
            manifest.clone(),
            "memory://plugin",
            "0".repeat(64),
            actor,
        )?;

        // Register as an extension in the extension runtime
        let extension_manifest = self.plugin_to_extension(&manifest)?;
        let request = InstallExtensionRequest {
            manifest: extension_manifest,
            source_uri: format!("file://plugin/{}", manifest.name),
            checksum: "0".repeat(64),
            actor: actor.into(),
        };
        self.extension_manager
            .install(request)
            .await
            .map_err(|e| PluginError::Extension(e.to_string()))?;

        let mut plugins = self
            .plugins
            .write()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?;
        if plugins.values().any(|p| p.name == plugin.name) {
            return Err(PluginError::Conflict(format!(
                "plugin {} is already installed",
                plugin.name
            )));
        }
        let id = plugin.id;
        plugins.insert(id, plugin.clone());
        Ok(plugin)
    }

    /// Enable a plugin (load + enable its extension).
    pub async fn enable(&self, id: Uuid, actor: &str) -> PluginResult<Plugin> {
        let plugin = self.required_plugin(id)?;
        if plugin.state != PluginState::Installed {
            return Err(PluginError::InvalidState(
                "plugin must be Installed before enable".into(),
            ));
        }

        let extension = self
            .find_extension_by_key(&plugin.name)
            .await?
            .ok_or_else(|| PluginError::NotFound("extension not found".into()))?;
        self.extension_manager
            .load(extension.id, actor)
            .await
            .map_err(|e| PluginError::Extension(e.to_string()))?;
        self.extension_manager
            .enable(extension.id, actor)
            .await
            .map_err(|e| PluginError::Extension(e.to_string()))?;

        let mut plugins = self
            .plugins
            .write()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?;
        let plugin = plugins
            .get_mut(&id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        plugin.state = PluginState::Enabled;
        plugin.version_count = plugin.version_count.saturating_add(1);
        plugin.actor = actor.into();
        plugin.updated_at = Utc::now();
        Ok(plugin.clone())
    }

    /// Disable a plugin.
    pub async fn disable(&self, id: Uuid, actor: &str) -> PluginResult<Plugin> {
        let plugin = self.required_plugin(id)?;
        if plugin.state != PluginState::Enabled {
            return Err(PluginError::InvalidState(
                "plugin must be Enabled before disable".into(),
            ));
        }

        let extension = self
            .find_extension_by_key(&plugin.name)
            .await?
            .ok_or_else(|| PluginError::NotFound("extension not found".into()))?;
        self.extension_manager
            .disable(extension.id, actor)
            .await
            .map_err(|e| PluginError::Extension(e.to_string()))?;

        let mut plugins = self
            .plugins
            .write()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?;
        let plugin = plugins
            .get_mut(&id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        plugin.state = PluginState::Disabled;
        plugin.version_count = plugin.version_count.saturating_add(1);
        plugin.actor = actor.into();
        plugin.updated_at = Utc::now();
        Ok(plugin.clone())
    }

    /// Uninstall a plugin.
    pub async fn uninstall(&self, id: Uuid, actor: &str) -> PluginResult<Plugin> {
        let plugin = self.required_plugin(id)?;
        if plugin.state != PluginState::Disabled {
            return Err(PluginError::InvalidState(
                "plugin must be Disabled before uninstall".into(),
            ));
        }

        let extension = self.find_extension_by_key(&plugin.name).await?;
        if let Some(ext) = extension {
            self.extension_manager
                .uninstall(ext.id, actor)
                .await
                .map_err(|e| PluginError::Extension(e.to_string()))?;
        }

        let mut plugins = self
            .plugins
            .write()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?;
        let plugin = plugins
            .remove(&id)
            .ok_or_else(|| PluginError::NotFound(id.to_string()))?;
        Ok(plugin)
    }

    /// List all plugins.
    pub fn list(&self) -> PluginResult<Vec<Plugin>> {
        let plugins = self
            .plugins
            .read()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?;
        let mut values: Vec<_> = plugins.values().cloned().collect();
        values.sort_by_key(|p| p.name.clone());
        Ok(values)
    }

    /// Find a plugin by ID.
    pub fn find(&self, id: Uuid) -> PluginResult<Option<Plugin>> {
        Ok(self
            .plugins
            .read()
            .map_err(|_| PluginError::Conflict("plugin lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    async fn find_extension_by_key(&self, key: &str) -> PluginResult<Option<Extension>> {
        let extensions = self
            .extension_manager
            .list()
            .await
            .map_err(|e| PluginError::Extension(e.to_string()))?;
        Ok(extensions.into_iter().find(|ext| ext.key == key))
    }

    fn required_plugin(&self, id: Uuid) -> PluginResult<Plugin> {
        self.find(id)?
            .ok_or_else(|| PluginError::NotFound(id.to_string()))
    }

    fn plugin_to_extension(
        &self,
        manifest: &PluginManifest,
    ) -> PluginResult<ExtensionManifest> {
        let capabilities: Vec<CapabilityManifest> = manifest
            .tools
            .iter()
            .chain(manifest.skills.iter())
            .chain(manifest.agents.iter())
            .map(|key| CapabilityManifest {
                key: key.clone(),
                version: manifest.version.clone(),
                name: format!("{}.{}", manifest.name, key),
                permissions: std::collections::BTreeSet::new(),
                metadata: std::collections::BTreeMap::new(),
            })
            .collect();

        Ok(ExtensionManifest {
            key: manifest.name.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            entrypoint: format!("plugin://{}", manifest.name),
            requested_permissions: std::collections::BTreeSet::new(),
            capabilities,
            providers: vec![ProviderManifest {
                key: format!("plugin-{}", manifest.name),
                kind: ProviderKind::Native,
                capabilities: manifest
                    .tools
                    .iter()
                    .chain(manifest.skills.iter())
                    .chain(manifest.agents.iter())
                    .cloned()
                    .collect(),
                priority: 0,
                config: serde_json::Value::Null,
                metadata: std::collections::BTreeMap::new(),
            }],
            metadata: std::collections::BTreeMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use async_trait::async_trait;
    use uuid::Uuid;

    use core_agent_extension::{
        CapabilityResult, ExtensionLoadHandle, ExtensionLoader, ExtensionHost,
        ExtensionManagerBuilder, ExtensionManifestRecord, ExtensionResult,
        Provider, CapabilityInvocation,
    };

    use super::*;

    /// A no-op loader that does not read from disk.
    struct NoopExtensionLoader;

    #[async_trait]
    impl ExtensionLoader for NoopExtensionLoader {
        async fn load(
            &self,
            manifest: &ExtensionManifestRecord,
        ) -> ExtensionResult<ExtensionLoadHandle> {
            Ok(ExtensionLoadHandle {
                extension_id: manifest.extension_id,
                manifest_id: manifest.id,
                generation: Uuid::new_v4(),
            })
        }

        async fn unload(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
            Ok(())
        }
    }

    /// A no-op host that succeeds on start/stop but fails on execute.
    struct NoopExtensionHost;

    #[async_trait]
    impl ExtensionHost for NoopExtensionHost {
        async fn start(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
            Ok(())
        }

        async fn stop(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
            Ok(())
        }

        async fn execute(
            &self,
            _handle: &ExtensionLoadHandle,
            _provider: &Provider,
            _invocation: &CapabilityInvocation,
        ) -> ExtensionResult<CapabilityResult> {
            Err(core_agent_extension::ExtensionError::Host(
                "not configured".into(),
            ))
        }
    }

    #[tokio::test]
    async fn plugin_install_enable_disable_uninstall_lifecycle() {
        let ext_manager = ExtensionManagerBuilder::default()
            .loader(Arc::new(NoopExtensionLoader))
            .host(Arc::new(NoopExtensionHost))
            .build();
        let manager = PluginManager::builder()
            .extension_manager(Arc::new(ext_manager))
            .build();

        let manifest = PluginManifest {
            name: "test-plugin".into(),
            version: "1.0.0".into(),
            description: "Test plugin".into(),
            author: "tester".into(),
            tools: vec!["test.tool".into()],
            skills: Vec::new(),
            agents: Vec::new(),
            permissions: BTreeSet::new(),
            metadata: BTreeMap::new(),
        };

        let plugin = manager.install(manifest, "tester").await.unwrap();
        assert_eq!(plugin.state, PluginState::Installed);

        let enabled = manager.enable(plugin.id, "tester").await.unwrap();
        assert_eq!(enabled.state, PluginState::Enabled);

        let disabled = manager.disable(plugin.id, "tester").await.unwrap();
        assert_eq!(disabled.state, PluginState::Disabled);

        manager.uninstall(plugin.id, "tester").await.unwrap();
        assert!(manager.find(plugin.id).unwrap().is_none());
    }
}