//! Stable configuration contracts and replaceable provider strategies.

mod domain;
mod error;
mod infrastructure;
mod manager;
mod providers;
mod writer;

pub use domain::{
    AgentConfig, AgentConfigPatch, ConfigCompression, ConfigCompressionPatch, ConfigContext,
    ConfigContextPatch, ConfigLayer, ConfigMemory, ConfigMemoryPatch, ConfigModel,
    ConfigModelPatch, ConfigPermissions, ConfigPermissionsPatch, ConfigRequest, ConfigSession,
    ConfigSessionPatch, ConfigSourceInfo, ResolvedConfig, CONFIG_SCHEMA_VERSION,
    DEFAULT_MAX_CONTEXT_TOKENS,
};
pub use error::{ConfigError, ConfigResult};
pub use infrastructure::{ConfigProvider, SecretResolver};
pub use manager::{ConfigManager, ConfigManagerBuilder};
pub use providers::{
    DefaultsConfigProvider, EnvironmentConfigProvider, EnvironmentSecretResolver,
    ProjectFileConfigProvider, UserFileConfigProvider,
};
pub use writer::{UserConfigSnapshot, UserConfigUpdate, UserConfigWriter};

use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

/// Product-default composition. Callers may build a different manager from
/// the stable provider traits without changing any core consumer.
pub fn standard_config_manager() -> ConfigResult<ConfigManager> {
    ConfigManager::builder()
        .provider(Arc::new(DefaultsConfigProvider))
        .provider(Arc::new(UserFileConfigProvider::discover()))
        .provider(Arc::new(ProjectFileConfigProvider))
        .provider(Arc::new(EnvironmentConfigProvider::current()))
        .secret_resolver(Arc::new(EnvironmentSecretResolver::current()))
        .build()
}

/// Stable opaque key for project-scoped application data. It avoids leaking
/// full local paths into storage directory names.
pub fn project_storage_key(workspace: &Path) -> ConfigResult<String> {
    let canonical = std::fs::canonicalize(workspace).map_err(|error| {
        ConfigError::Source(format!(
            "cannot canonicalize {}: {error}",
            workspace.display()
        ))
    })?;
    let normalized = canonical.to_string_lossy().replace('\\', "/");
    Ok(format!("{:x}", Sha256::digest(normalized.as_bytes())))
}
