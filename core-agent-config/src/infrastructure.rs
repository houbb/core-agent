use async_trait::async_trait;

use crate::{ConfigLayer, ConfigRequest, ConfigResult};

/// A replaceable source of one configuration layer. Core consumers never
/// depend on the provider's storage format or transport.
#[async_trait]
pub trait ConfigProvider: Send + Sync {
    fn key(&self) -> &str;
    fn priority(&self) -> u16;
    async fn load(&self, request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>>;
}

/// Resolves an opaque secret reference such as `env:CORE_AGENT_API_KEY`.
/// Future vault/keychain implementations can replace this without changing
/// the configuration schema or EnterpriseAgent.
#[async_trait]
pub trait SecretResolver: Send + Sync {
    fn key(&self) -> &str;
    fn supports(&self, reference: &str) -> bool;
    async fn resolve(&self, reference: &str) -> ConfigResult<Option<String>>;
}
