use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::error::{ModelError, ModelResult};

use super::ModelProvider;

/// Runtime registry of live Provider instances. Catalog entries contain only
/// metadata; credentials remain inside these instances.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn ModelProvider>>>,
}

impl ProviderRegistry {
    pub fn register(&self, provider: Arc<dyn ModelProvider>) -> ModelResult<()> {
        let key = provider.key().trim();
        if key.is_empty() {
            return Err(ModelError::InvalidArgument(
                "provider runtime key must not be empty".into(),
            ));
        }
        self.providers
            .write()
            .map_err(|_| ModelError::Internal("provider registry lock poisoned".into()))?
            .insert(key.to_owned(), provider);
        Ok(())
    }

    pub fn get(&self, key: &str) -> ModelResult<Arc<dyn ModelProvider>> {
        self.providers
            .read()
            .map_err(|_| ModelError::Internal("provider registry lock poisoned".into()))?
            .get(key)
            .cloned()
            .ok_or_else(|| ModelError::ProviderNotFound(key.to_owned()))
    }

    pub fn contains(&self, key: &str) -> ModelResult<bool> {
        Ok(self
            .providers
            .read()
            .map_err(|_| ModelError::Internal("provider registry lock poisoned".into()))?
            .contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ModelProfile, ModelRequest, ModelResponse};
    use async_trait::async_trait;

    struct EmptyProvider;

    #[async_trait]
    impl ModelProvider for EmptyProvider {
        fn key(&self) -> &str {
            "empty"
        }

        async fn invoke(
            &self,
            _request: &ModelRequest,
            _target: &ModelProfile,
        ) -> ModelResult<ModelResponse> {
            unreachable!()
        }
    }

    #[test]
    fn registry_registers_provider() {
        let registry = ProviderRegistry::default();
        registry.register(Arc::new(EmptyProvider)).unwrap();
        assert!(registry.contains("empty").unwrap());
        assert!(registry.get("missing").is_err());
    }
}
