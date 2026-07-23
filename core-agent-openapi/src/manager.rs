use std::sync::Arc;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    ApiKey, ApiKeyScope, ApiKeyState, RateLimitStatus,
};
use crate::error::{OpenApiError, OpenApiResult};
use crate::infrastructure::{ApiKeyStore, Gateway, RateLimiter};

/// OpenAPI Gateway Manager — orchestrates API key management and gateway routing.
pub struct OpenApiManager {
    key_store: Arc<dyn ApiKeyStore>,
    rate_limiter: Arc<dyn RateLimiter>,
    #[allow(dead_code)]
    gateway: Arc<dyn Gateway>,
}

impl OpenApiManager {
    pub fn new(
        key_store: Arc<dyn ApiKeyStore>,
        rate_limiter: Arc<dyn RateLimiter>,
        gateway: Arc<dyn Gateway>,
    ) -> Self {
        Self {
            key_store,
            rate_limiter,
            gateway,
        }
    }

    /// Create a new API key for a tenant.
    pub async fn create_api_key(
        &self,
        tenant_id: Uuid,
        name: &str,
        scopes: Vec<ApiKeyScope>,
        actor: &str,
    ) -> OpenApiResult<(ApiKey, String)> {
        let raw_key = format!("sk-agent-{}-{}", Uuid::new_v4(), Uuid::new_v4());
        let key_hash = hex_hash(&raw_key);
        let key_prefix = raw_key[..16].to_string();
        let key = ApiKey::new(tenant_id, key_prefix, &key_hash, name, scopes, actor);
        key.validate()?;
        self.key_store.store(&key).await?;
        Ok((key, raw_key))
    }

    /// Authenticate and authorize a request.
    pub async fn authenticate(
        &self,
        api_key: &str,
        required_scope: ApiKeyScope,
    ) -> OpenApiResult<ApiKey> {
        let key_hash = hex_hash(api_key);
        let key = self
            .key_store
            .find_by_hash(&key_hash)
            .await?
            .ok_or_else(|| OpenApiError::Authentication("invalid API key".into()))?;

        if !key.is_active() {
            return Err(OpenApiError::Authentication("API key is not active".into()));
        }

        if !key.has_scope(required_scope) {
            return Err(OpenApiError::Authorization(
                "API key lacks required scope".into(),
            ));
        }

        let status = self.rate_limiter.check(key.id, required_scope).await?;
        if !status.allowed {
            return Err(OpenApiError::RateLimit("rate limit exceeded".into()));
        }

        Ok(key)
    }

    /// Revoke an API key.
    pub async fn revoke_api_key(&self, key_id: Uuid, actor: &str) -> OpenApiResult<()> {
        self.key_store.revoke(key_id, actor).await
    }

    /// List API keys for a tenant.
    pub async fn list_api_keys(&self, tenant_id: Uuid) -> OpenApiResult<Vec<ApiKey>> {
        self.key_store.list_by_tenant(tenant_id).await
    }
}

fn hex_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

// ── Default In-Memory Implementations ─────────────────────────────────────

#[derive(Default)]
pub struct InMemoryApiKeyStore {
    keys: std::sync::RwLock<Vec<ApiKey>>,
}

#[async_trait::async_trait]
impl ApiKeyStore for InMemoryApiKeyStore {
    async fn store(&self, key: &ApiKey) -> OpenApiResult<()> {
        let mut keys = self.keys.write().map_err(|_| {
            OpenApiError::Internal("api key store lock poisoned".into())
        })?;
        if keys.iter().any(|k| k.id == key.id) {
            return Err(OpenApiError::Validation("key already exists".into()));
        }
        keys.push(key.clone());
        Ok(())
    }

    async fn find_by_hash(&self, key_hash: &str) -> OpenApiResult<Option<ApiKey>> {
        let keys = self.keys.read().map_err(|_| {
            OpenApiError::Internal("api key store lock poisoned".into())
        })?;
        Ok(keys.iter().find(|k| k.key_hash == key_hash).cloned())
    }

    async fn list_by_tenant(&self, tenant_id: Uuid) -> OpenApiResult<Vec<ApiKey>> {
        let keys = self.keys.read().map_err(|_| {
            OpenApiError::Internal("api key store lock poisoned".into())
        })?;
        Ok(keys
            .iter()
            .filter(|k| k.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn revoke(&self, key_id: Uuid, actor: &str) -> OpenApiResult<()> {
        let mut keys = self.keys.write().map_err(|_| {
            OpenApiError::Internal("api key store lock poisoned".into())
        })?;
        let key = keys
            .iter_mut()
            .find(|k| k.id == key_id)
            .ok_or_else(|| OpenApiError::NotFound(key_id.to_string()))?;
        key.state = ApiKeyState::Revoked;
        key.actor = actor.into();
        key.updated_at = chrono::Utc::now();
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopRateLimiter;

#[async_trait::async_trait]
impl RateLimiter for NoopRateLimiter {
    async fn check(
        &self,
        _api_key_id: Uuid,
        _scope: ApiKeyScope,
    ) -> OpenApiResult<RateLimitStatus> {
        Ok(RateLimitStatus {
            allowed: true,
            remaining: u64::MAX,
            reset_at: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::Gateway;

    struct MockGateway;

    #[async_trait::async_trait]
    impl Gateway for MockGateway {
        async fn authenticate(&self, _key: &str) -> OpenApiResult<ApiKey> {
            Err(OpenApiError::Authentication("mock".into()))
        }
        async fn authorize(&self, _key: &ApiKey, _scope: ApiKeyScope) -> OpenApiResult<()> {
            Ok(())
        }
        async fn chat(
            &self,
            _request: crate::domain::AgentChatApiRequest,
            _key: &ApiKey,
        ) -> OpenApiResult<crate::domain::AgentChatApiResponse> {
            Err(OpenApiError::Internal("not implemented".into()))
        }
        async fn execute_task(
            &self,
            _request: crate::domain::TaskApiRequest,
            _key: &ApiKey,
        ) -> OpenApiResult<crate::domain::TaskApiResponse> {
            Err(OpenApiError::Internal("not implemented".into()))
        }
        async fn run_workflow(
            &self,
            _request: crate::domain::WorkflowRunApiRequest,
            _key: &ApiKey,
        ) -> OpenApiResult<crate::domain::WorkflowRunApiResponse> {
            Err(OpenApiError::Internal("not implemented".into()))
        }
        async fn search_knowledge(
            &self,
            _request: crate::domain::KnowledgeSearchApiRequest,
            _key: &ApiKey,
        ) -> OpenApiResult<crate::domain::KnowledgeSearchApiResponse> {
            Err(OpenApiError::Internal("not implemented".into()))
        }
    }

    #[tokio::test]
    async fn create_and_authenticate_api_key() {
        let store = Arc::new(InMemoryApiKeyStore::default());
        let limiter = Arc::new(NoopRateLimiter);
        let gateway = Arc::new(MockGateway);
        let manager = OpenApiManager::new(store, limiter, gateway);

        let tenant_id = Uuid::new_v4();
        let (key, raw_key) = manager
            .create_api_key(tenant_id, "Test Key", vec![ApiKeyScope::AgentChat], "admin")
            .await
            .unwrap();
        assert_eq!(key.name, "Test Key");
        assert!(key.is_active());
        assert!(raw_key.starts_with("sk-agent-"));

        let auth_key = manager.authenticate(&raw_key, ApiKeyScope::AgentChat).await.unwrap();
        assert_eq!(auth_key.id, key.id);
    }

    #[tokio::test]
    async fn authenticate_with_wrong_scope_fails() {
        let store = Arc::new(InMemoryApiKeyStore::default());
        let limiter = Arc::new(NoopRateLimiter);
        let gateway = Arc::new(MockGateway);
        let manager = OpenApiManager::new(store, limiter, gateway);

        let tenant_id = Uuid::new_v4();
        let (_, raw_key) = manager
            .create_api_key(tenant_id, "Test", vec![ApiKeyScope::AgentChat], "admin")
            .await
            .unwrap();

        let result = manager.authenticate(&raw_key, ApiKeyScope::WorkflowRun).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), "AUTHORIZATION");
    }

    #[tokio::test]
    async fn revoke_api_key() {
        let store = Arc::new(InMemoryApiKeyStore::default());
        let limiter = Arc::new(NoopRateLimiter);
        let gateway = Arc::new(MockGateway);
        let manager = OpenApiManager::new(store, limiter, gateway);

        let tenant_id = Uuid::new_v4();
        let (key, _) = manager
            .create_api_key(tenant_id, "Test", vec![ApiKeyScope::AgentChat], "admin")
            .await
            .unwrap();

        manager.revoke_api_key(key.id, "admin").await.unwrap();
        let keys = manager.list_api_keys(tenant_id).await.unwrap();
        let revoked = keys.iter().find(|k| k.id == key.id).unwrap();
        assert_eq!(revoked.state, ApiKeyState::Revoked);
    }
}