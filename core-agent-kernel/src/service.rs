use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::domain::validate_key;
use crate::{KernelError, KernelResult};

#[derive(Default)]
pub struct ServiceRegistry {
    services: RwLock<BTreeMap<String, Arc<dyn Any + Send + Sync>>>,
}

impl ServiceRegistry {
    pub fn register<T>(&self, key: impl Into<String>, service: Arc<T>) -> KernelResult<()>
    where
        T: Any + Send + Sync,
    {
        let key = key.into();
        validate_key("service key", &key)?;
        let mut services = self
            .services
            .write()
            .map_err(|_| KernelError::Internal("service registry lock poisoned".into()))?;
        if services.contains_key(&key) {
            return Err(KernelError::Service(format!(
                "service {key} is already registered"
            )));
        }
        services.insert(key, service);
        Ok(())
    }

    pub fn resolve<T>(&self, key: &str) -> KernelResult<Arc<T>>
    where
        T: Any + Send + Sync,
    {
        let service = self
            .services
            .read()
            .map_err(|_| KernelError::Internal("service registry lock poisoned".into()))?
            .get(key)
            .cloned()
            .ok_or_else(|| KernelError::Service(format!("service {key} is not registered")))?;
        Arc::downcast::<T>(service).map_err(|_| {
            KernelError::Service(format!("service {key} has a different concrete type"))
        })
    }

    pub fn contains(&self, key: &str) -> KernelResult<bool> {
        Ok(self
            .services
            .read()
            .map_err(|_| KernelError::Internal("service registry lock poisoned".into()))?
            .contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_resolution_is_type_safe() {
        let registry = ServiceRegistry::default();
        registry.register("answer", Arc::new(42_u64)).unwrap();
        assert_eq!(*registry.resolve::<u64>("answer").unwrap(), 42);
        assert!(registry.resolve::<String>("answer").is_err());
    }
}
