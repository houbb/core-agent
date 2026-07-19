use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{
    elapsed_ms, PermissionDecision, RawToolOutput, ToolDefinition, ToolExecutionRecord,
    ToolLifecycleStatus, ToolRequest, ToolResult, ToolUsage,
};
use crate::error::{ToolError, ToolRuntimeResult};

use super::{
    Tool, ToolContext, ToolExecutor, ToolLifecycle, ToolPermission, ToolPolicy, ToolResultMapper,
    ToolValidator,
};

#[derive(Default)]
pub struct DefaultToolExecutor;

#[async_trait]
impl ToolExecutor for DefaultToolExecutor {
    async fn invoke(
        &self,
        tool: Arc<dyn Tool>,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        tool.execute(request, context).await
    }
}

#[derive(Default)]
pub struct JsonSchemaToolValidator;

impl ToolValidator for JsonSchemaToolValidator {
    fn validate_schema(&self, schema: &serde_json::Value) -> ToolRuntimeResult<()> {
        jsonschema::options()
            .with_pattern_options(jsonschema::PatternOptions::regex())
            .build(schema)
            .map(|_| ())
            .map_err(|error| ToolError::Validation(format!("invalid JSON Schema: {error}")))
    }

    fn validate(
        &self,
        schema: &serde_json::Value,
        parameters: &serde_json::Value,
    ) -> ToolRuntimeResult<()> {
        let validator = jsonschema::options()
            .with_pattern_options(jsonschema::PatternOptions::regex())
            .build(schema)
            .map_err(|error| ToolError::Validation(format!("invalid JSON Schema: {error}")))?;
        validator
            .validate(parameters)
            .map_err(|error| ToolError::Validation(error.to_string()))
    }
}

#[derive(Default)]
pub struct DefaultToolPermission;

#[async_trait]
impl ToolPermission for DefaultToolPermission {
    async fn check(
        &self,
        _request: &ToolRequest,
        tool: &ToolDefinition,
    ) -> ToolRuntimeResult<PermissionDecision> {
        Ok(tool.default_permission)
    }
}

pub struct FixedToolPermission(pub PermissionDecision);

#[async_trait]
impl ToolPermission for FixedToolPermission {
    async fn check(
        &self,
        _request: &ToolRequest,
        _tool: &ToolDefinition,
    ) -> ToolRuntimeResult<PermissionDecision> {
        Ok(self.0)
    }
}

#[derive(Default)]
pub struct DefaultToolResultMapper;

impl ToolResultMapper for DefaultToolResultMapper {
    fn map(
        &self,
        request: &ToolRequest,
        definition: &ToolDefinition,
        started_at: DateTime<Utc>,
        completed_at: DateTime<Utc>,
        output: ToolRuntimeResult<RawToolOutput>,
    ) -> ToolRuntimeResult<ToolResult> {
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                return Ok(ToolResult::failed(
                    request.id,
                    &definition.key,
                    &error,
                    started_at,
                    completed_at,
                ));
            }
        };
        if output
            .metadata
            .keys()
            .any(|key| key.starts_with("core_agent."))
        {
            return Err(ToolError::Mapping(
                "tool output metadata uses reserved core_agent namespace".into(),
            ));
        }
        let output_bytes = serde_json::to_vec(&(&output.content, &output.attachments))
            .map_err(|error| ToolError::Mapping(error.to_string()))?
            .len() as u64;
        Ok(ToolResult {
            request_id: request.id,
            tool_key: definition.key.clone(),
            status: ToolLifecycleStatus::Success,
            content: output.content,
            attachments: output.attachments,
            usage: ToolUsage {
                duration_ms: elapsed_ms(started_at, completed_at),
                output_bytes,
            },
            error: None,
            metadata: output.metadata,
            started_at,
            completed_at,
        })
    }
}

#[derive(Default)]
pub struct NoopToolLifecycle;

#[async_trait]
impl ToolLifecycle for NoopToolLifecycle {
    async fn transition(&self, _record: &ToolExecutionRecord) -> ToolRuntimeResult<()> {
        Ok(())
    }
}

/// Process-local lifecycle store used by the default embedded Runtime.
#[derive(Default)]
pub struct InMemoryToolLifecycle {
    records: RwLock<BTreeMap<uuid::Uuid, ToolExecutionRecord>>,
}

impl InMemoryToolLifecycle {
    pub fn find(&self, request_id: uuid::Uuid) -> ToolRuntimeResult<Option<ToolExecutionRecord>> {
        Ok(self
            .records
            .read()
            .map_err(|_| ToolError::Internal("tool lifecycle lock poisoned".into()))?
            .get(&request_id)
            .cloned())
    }
}

#[async_trait]
impl ToolLifecycle for InMemoryToolLifecycle {
    async fn transition(&self, record: &ToolExecutionRecord) -> ToolRuntimeResult<()> {
        record.validate()?;
        let mut records = self
            .records
            .write()
            .map_err(|_| ToolError::Internal("tool lifecycle lock poisoned".into()))?;
        if record.status == ToolLifecycleStatus::Created {
            if records.contains_key(&record.request_id) {
                return Err(ToolError::Lifecycle(format!(
                    "execution {} already exists",
                    record.request_id
                )));
            }
        } else {
            let previous = records.get(&record.request_id).ok_or_else(|| {
                ToolError::Lifecycle(format!(
                    "execution {} has no CREATED record",
                    record.request_id
                ))
            })?;
            if previous.id != record.id
                || previous.tool_key != record.tool_key
                || previous.provider_key != record.provider_key
                || previous.session_id != record.session_id
                || previous.subject != record.subject
                || !previous.status.can_transition_to(record.status)
            {
                return Err(ToolError::Lifecycle(
                    "invalid or identity-changing lifecycle transition".into(),
                ));
            }
        }
        records.insert(record.request_id, record.clone());
        Ok(())
    }
}

#[derive(Default)]
pub struct AllowAllToolPolicy;

#[async_trait]
impl ToolPolicy for AllowAllToolPolicy {
    async fn evaluate(
        &self,
        _request: &ToolRequest,
        _tool: &ToolDefinition,
    ) -> ToolRuntimeResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_enforces_required_properties() {
        let validator = JsonSchemaToolValidator;
        let schema = serde_json::json!({
            "type":"object",
            "required":["path"],
            "properties":{"path":{"type":"string","minLength":1}},
            "additionalProperties":false
        });
        assert!(validator
            .validate(&schema, &serde_json::json!({"path":"a"}))
            .is_ok());
        assert!(validator
            .validate(&schema, &serde_json::json!({"path":""}))
            .is_err());
        assert!(validator
            .validate(&schema, &serde_json::json!({"other":1}))
            .is_err());
    }

    #[test]
    fn validator_rejects_external_schema_references() {
        let validator = JsonSchemaToolValidator;
        let schema = serde_json::json!({"$ref":"file:///tmp/private.json"});
        assert!(validator.validate_schema(&schema).is_err());
    }

    #[tokio::test]
    async fn in_memory_lifecycle_rejects_duplicate_created_record() {
        let lifecycle = InMemoryToolLifecycle::default();
        let record = ToolExecutionRecord::new(
            uuid::Uuid::new_v4(),
            "builtin/echo@1",
            "builtin",
            None,
            None,
            &BTreeMap::new(),
        );
        lifecycle.transition(&record).await.unwrap();
        assert!(lifecycle.transition(&record).await.is_err());
    }
}
