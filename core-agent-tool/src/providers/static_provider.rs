use async_trait::async_trait;

use crate::domain::ToolProviderDefinition;
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{ToolProvider, ToolRegistration};

/// Process-local Provider suitable for Builtin Tools and embedded applications.
pub struct StaticToolProvider {
    definition: ToolProviderDefinition,
    registrations: Vec<ToolRegistration>,
}

impl StaticToolProvider {
    pub fn new(definition: ToolProviderDefinition, registrations: Vec<ToolRegistration>) -> Self {
        Self {
            definition,
            registrations,
        }
    }
}

#[async_trait]
impl ToolProvider for StaticToolProvider {
    fn definition(&self) -> ToolProviderDefinition {
        self.definition.clone()
    }

    async fn discover(&self) -> ToolRuntimeResult<Vec<ToolRegistration>> {
        Ok(self.registrations.clone())
    }
}
