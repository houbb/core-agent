use std::collections::BTreeSet;
use std::sync::Arc;

use crate::{
    AgentConfig, ConfigError, ConfigProvider, ConfigRequest, ConfigResult, ResolvedConfig,
    SecretResolver,
};

pub struct ConfigManager {
    providers: Vec<Arc<dyn ConfigProvider>>,
    secret_resolvers: Vec<Arc<dyn SecretResolver>>,
}

#[derive(Default)]
pub struct ConfigManagerBuilder {
    providers: Vec<Arc<dyn ConfigProvider>>,
    secret_resolvers: Vec<Arc<dyn SecretResolver>>,
}

impl ConfigManagerBuilder {
    pub fn provider(mut self, provider: Arc<dyn ConfigProvider>) -> Self {
        self.providers.push(provider);
        self
    }

    pub fn secret_resolver(mut self, resolver: Arc<dyn SecretResolver>) -> Self {
        self.secret_resolvers.push(resolver);
        self
    }

    pub fn build(self) -> ConfigResult<ConfigManager> {
        let mut keys = BTreeSet::new();
        let mut priorities = BTreeSet::new();
        for provider in &self.providers {
            if provider.key().trim().is_empty()
                || !keys.insert(provider.key().to_owned())
                || !priorities.insert(provider.priority())
            {
                return Err(ConfigError::Validation(
                    "configuration provider keys and priorities must be unique".into(),
                ));
            }
        }
        let mut resolver_keys = BTreeSet::new();
        if self
            .secret_resolvers
            .iter()
            .any(|resolver| !resolver_keys.insert(resolver.key().to_owned()))
        {
            return Err(ConfigError::Validation(
                "secret resolver keys must be unique".into(),
            ));
        }
        Ok(ConfigManager {
            providers: self.providers,
            secret_resolvers: self.secret_resolvers,
        })
    }
}

impl ConfigManager {
    pub fn builder() -> ConfigManagerBuilder {
        ConfigManagerBuilder::default()
    }

    pub async fn resolve(&self, request: &ConfigRequest) -> ConfigResult<ResolvedConfig> {
        let mut layers = Vec::new();
        for provider in &self.providers {
            if let Some(layer) = provider.load(request).await? {
                if layer.source.provider != provider.key()
                    || layer.source.priority != provider.priority()
                {
                    return Err(ConfigError::Validation(format!(
                        "provider {} returned inconsistent source metadata",
                        provider.key()
                    )));
                }
                layers.push(layer);
            }
        }
        layers.sort_by_key(|layer| layer.source.priority);
        let mut config = AgentConfig::default();
        let mut sources = Vec::with_capacity(layers.len());
        for layer in layers {
            config.apply(layer.patch);
            sources.push(layer.source);
        }
        if let Some(reference) = config.model.api_key_ref.clone() {
            let resolver = self
                .secret_resolvers
                .iter()
                .find(|resolver| resolver.supports(&reference));
            if let Some(resolver) = resolver {
                if let Some(secret) = resolver.resolve(&reference).await? {
                    config.model.api_key = Some(secret);
                } else if config.model.api_key.is_none() {
                    return Err(ConfigError::Secret(format!(
                        "{} did not resolve {reference}",
                        resolver.key()
                    )));
                }
            } else if config.model.api_key.is_none() {
                return Err(ConfigError::Secret(format!(
                    "no resolver supports {reference}"
                )));
            }
        }
        config.validate()?;
        Ok(ResolvedConfig { config, sources })
    }
}
