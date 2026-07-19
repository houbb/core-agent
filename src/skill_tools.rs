use std::sync::Arc;

use serde_json::{json, Value};

use crate::{
    FunctionTool, PermissionDecision, RawToolOutput, SkillCatalog, ToolContent, ToolDefinition,
    ToolError, ToolRegistration, DEFAULT_SKILL_FILE_LIMIT_BYTES,
};

pub(crate) fn registrations(catalog: Arc<SkillCatalog>) -> Vec<ToolRegistration> {
    let mut definition = ToolDefinition::new(
        "guidance",
        "load_skill",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string", "description": "Exact skill name from the available-skills catalog"}
            },
            "additionalProperties": false
        }),
    );
    definition.description = "Load the full, bounded SKILL.md instructions for one discovered skill. Skill metadata is advertised separately so full files are loaded only when relevant.".into();
    definition.category = "guidance.read".into();
    definition.default_permission = PermissionDecision::Allow;
    let key = definition.key.clone();
    let tool = Arc::new(FunctionTool::new(key, move |request, context| {
        let catalog = catalog.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let name = request
                .parameters
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::InvalidArgument("skill name is required".into()))?;
            if name.len() > 128 {
                return Err(ToolError::InvalidArgument(
                    "skill name exceeds 128 bytes".into(),
                ));
            }
            let loaded = catalog
                .load(name, DEFAULT_SKILL_FILE_LIMIT_BYTES)
                .map_err(|error| ToolError::execution("load_skill", error.to_string(), false))?;
            Ok(RawToolOutput {
                content: vec![
                    ToolContent::Json(json!({
                        "name": loaded.descriptor.name,
                        "description": loaded.descriptor.description,
                        "scope": loaded.descriptor.scope,
                        "sha256": loaded.descriptor.content_sha256,
                        "bytes": loaded.descriptor.bytes
                    })),
                    ToolContent::Text(loaded.content),
                ],
                ..RawToolOutput::default()
            })
        }
    }));
    vec![ToolRegistration::new(definition, tool)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GuidanceScope, SkillRoot};

    #[tokio::test]
    async fn load_skill_registration_returns_full_content_lazily() {
        let directory = tempfile::tempdir().unwrap();
        let skill = directory.path().join("review");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: review\ndescription: Review code\n---\nRun the review.",
        )
        .unwrap();
        let catalog = SkillCatalog::discover(
            &[SkillRoot::new(
                GuidanceScope::Project,
                directory.path(),
                100,
            )],
            16,
        )
        .unwrap();
        let registration = registrations(Arc::new(catalog)).remove(0);
        let request = crate::ToolRequest::new(
            registration.definition.key.clone(),
            json!({"name": "review"}),
        );
        let context = crate::ToolExecutionContext {
            request_id: request.id,
            cancellation: tokio_util::sync::CancellationToken::new(),
        };
        let output = registration.tool.execute(&request, &context).await.unwrap();
        assert_eq!(output.content.len(), 2);
        assert!(matches!(
            &output.content[1],
            ToolContent::Text(content) if content.contains("Run the review")
        ));
    }
}
