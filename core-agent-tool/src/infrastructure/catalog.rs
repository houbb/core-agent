use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::{ToolCapability, ToolDefinition, ToolProviderDefinition};
use crate::error::ToolRuntimeResult;

use super::ToolCatalog;

#[derive(Default)]
pub struct InMemoryToolCatalog {
    providers: RwLock<BTreeMap<String, ToolProviderDefinition>>,
    tools: RwLock<BTreeMap<String, ToolDefinition>>,
}

#[async_trait]
impl ToolCatalog for InMemoryToolCatalog {
    async fn upsert_provider(&self, provider: &ToolProviderDefinition) -> ToolRuntimeResult<()> {
        provider.validate()?;
        let mut providers = self.providers.write().await;
        let mut stored = provider.clone();
        if let Some(existing) = providers.get(&provider.key) {
            stored.id = existing.id;
            stored.created_at = existing.created_at;
        }
        providers.insert(provider.key.clone(), stored);
        Ok(())
    }

    async fn find_provider(&self, key: &str) -> ToolRuntimeResult<Option<ToolProviderDefinition>> {
        Ok(self.providers.read().await.get(key).cloned())
    }

    async fn list_providers(&self) -> ToolRuntimeResult<Vec<ToolProviderDefinition>> {
        Ok(self.providers.read().await.values().cloned().collect())
    }

    async fn remove_provider(&self, key: &str) -> ToolRuntimeResult<bool> {
        Ok(self.providers.write().await.remove(key).is_some())
    }

    async fn upsert_tool(&self, tool: &ToolDefinition) -> ToolRuntimeResult<()> {
        tool.validate()?;
        let mut tools = self.tools.write().await;
        let mut stored = tool.clone();
        if let Some(existing) = tools.get(&tool.key) {
            stored.id = existing.id;
            stored.created_at = existing.created_at;
        }
        tools.insert(tool.key.clone(), stored);
        Ok(())
    }

    async fn find_tool(&self, key: &str) -> ToolRuntimeResult<Option<ToolDefinition>> {
        Ok(self.tools.read().await.get(key).cloned())
    }

    async fn list_tools(&self) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        Ok(self.tools.read().await.values().cloned().collect())
    }

    async fn remove_tool(&self, key: &str) -> ToolRuntimeResult<bool> {
        Ok(self.tools.write().await.remove(key).is_some())
    }

    async fn find_by_capability(
        &self,
        capability: &ToolCapability,
        include_descendants: bool,
    ) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        Ok(self
            .tools
            .read()
            .await
            .values()
            .filter(|tool| {
                tool.enabled
                    && tool.capabilities.iter().any(|candidate| {
                        candidate == capability
                            || (include_descendants
                                && candidate.is_same_or_descendant_of(capability))
                    })
            })
            .cloned()
            .collect())
    }

    async fn categories(&self) -> ToolRuntimeResult<Vec<String>> {
        Ok(self
            .tools
            .read()
            .await
            .values()
            .filter(|tool| tool.enabled)
            .map(|tool| tool.category.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn catalog_discovers_capability_descendants() {
        let catalog = InMemoryToolCatalog::default();
        let mut tool =
            ToolDefinition::new("builtin", "read", "1", serde_json::json!({"type":"object"}));
        tool.capabilities
            .insert(ToolCapability::new("filesystem.read").unwrap());
        catalog.upsert_tool(&tool).await.unwrap();
        assert_eq!(
            catalog
                .find_by_capability(&ToolCapability::new("filesystem").unwrap(), true)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(catalog
            .find_by_capability(&ToolCapability::new("filesystem").unwrap(), false)
            .await
            .unwrap()
            .is_empty());
    }
}
