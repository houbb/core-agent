use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::error::{ToolError, ToolRuntimeResult};

use super::{Tool, ToolRegistration, ToolRegistry};

#[derive(Default)]
pub struct InMemoryToolRegistry {
    tools: RwLock<BTreeMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry for InMemoryToolRegistry {
    fn register(&self, registration: ToolRegistration) -> ToolRuntimeResult<()> {
        registration.definition.validate()?;
        if registration.tool.key() != registration.definition.key {
            return Err(ToolError::Registry(format!(
                "runtime key {} does not match definition key {}",
                registration.tool.key(),
                registration.definition.key
            )));
        }
        self.tools
            .write()
            .map_err(|_| ToolError::Internal("tool registry lock poisoned".into()))?
            .insert(registration.definition.key, registration.tool);
        Ok(())
    }

    fn remove(&self, key: &str) -> ToolRuntimeResult<Option<Arc<dyn Tool>>> {
        Ok(self
            .tools
            .write()
            .map_err(|_| ToolError::Internal("tool registry lock poisoned".into()))?
            .remove(key))
    }

    fn find(&self, key: &str) -> ToolRuntimeResult<Option<Arc<dyn Tool>>> {
        Ok(self
            .tools
            .read()
            .map_err(|_| ToolError::Internal("tool registry lock poisoned".into()))?
            .get(key)
            .cloned())
    }

    fn list(&self) -> ToolRuntimeResult<Vec<String>> {
        Ok(self
            .tools
            .read()
            .map_err(|_| ToolError::Internal("tool registry lock poisoned".into()))?
            .keys()
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::domain::{RawToolOutput, ToolDefinition, ToolRequest};
    use crate::infrastructure::ToolContext;

    struct EmptyTool(String);

    #[async_trait]
    impl Tool for EmptyTool {
        fn key(&self) -> &str {
            &self.0
        }

        async fn execute(
            &self,
            _request: &ToolRequest,
            _context: &ToolContext,
        ) -> ToolRuntimeResult<RawToolOutput> {
            Ok(RawToolOutput::default())
        }
    }

    #[test]
    fn registry_registers_and_removes_live_tools() {
        let registry = InMemoryToolRegistry::default();
        let definition = ToolDefinition::new(
            "builtin",
            "empty",
            "1",
            serde_json::json!({"type":"object"}),
        );
        registry
            .register(ToolRegistration::new(
                definition.clone(),
                Arc::new(EmptyTool(definition.key.clone())),
            ))
            .unwrap();
        assert!(registry.find(&definition.key).unwrap().is_some());
        assert!(registry.remove(&definition.key).unwrap().is_some());
    }
}
