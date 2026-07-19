use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    FunctionTool, ModelManager, ModelMessage, ModelRequest, ModelRole, ModelToolCall,
    ModelToolDefinition, PermissionDecision, RawToolOutput, StaticToolProvider, ToolContent,
    ToolDefinition, ToolError, ToolLifecycleStatus, ToolManager, ToolProviderDefinition,
    ToolProviderKind, ToolRegistration, ToolRequest,
};

const MAX_SUBAGENT_TASK_BYTES: usize = 16 * 1024;
const MAX_SUBAGENT_OUTPUT_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentProfile {
    General,
    Explore,
    Review,
}

impl SubAgentProfile {
    fn parse(value: Option<&str>) -> Result<Self, ToolError> {
        match value.unwrap_or("general") {
            "general" => Ok(Self::General),
            "explore" => Ok(Self::Explore),
            "review" => Ok(Self::Review),
            value => Err(ToolError::InvalidArgument(format!(
                "unsupported sub-agent profile: {value}"
            ))),
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::General => "Solve the delegated task independently. You are isolated from the parent conversation and have read-only workspace tools. Return a concise, evidence-based result; do not claim to modify files.",
            Self::Explore => "Explore the workspace to answer the delegated question. Use find/search/read tools efficiently, cite file paths, and do not modify files or run processes.",
            Self::Review => "Review the requested code or design. Prioritize concrete correctness, security and regression risks, cite file paths, and do not modify files or run processes.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentOutcome {
    pub profile: SubAgentProfile,
    pub response: String,
    pub tool_calls: usize,
    pub turns: usize,
}

pub struct SubAgentRuntime {
    models: Arc<ModelManager>,
    tools: Arc<ToolManager>,
    model_profile: String,
}

impl SubAgentRuntime {
    pub fn new(
        models: Arc<ModelManager>,
        tools: Arc<ToolManager>,
        model_profile: impl Into<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            models,
            tools,
            model_profile: model_profile.into(),
        })
    }

    pub async fn run(
        &self,
        task: &str,
        profile: SubAgentProfile,
        session_id: Option<Uuid>,
        cancellation: CancellationToken,
    ) -> Result<SubAgentOutcome, ToolError> {
        if task.trim().is_empty() || task.len() > MAX_SUBAGENT_TASK_BYTES {
            return Err(ToolError::InvalidArgument(
                "delegated task must contain 1..=16384 bytes".into(),
            ));
        }
        let definitions = self
            .tools
            .list()
            .await?
            .into_iter()
            .filter(subagent_tool_allowed)
            .collect::<Vec<_>>();
        let mut request = ModelRequest::new(vec![
            ModelMessage::text(ModelRole::System, profile.prompt()),
            ModelMessage::text(ModelRole::User, task),
        ])
        .with_profile(&self.model_profile);
        request.metadata.insert("subagent".into(), "true".into());
        if let Some(session_id) = session_id {
            request
                .metadata
                .insert("parent_session_id".into(), session_id.to_string());
        }
        request.tools = definitions
            .iter()
            .map(|definition| ModelToolDefinition {
                name: definition.name.clone(),
                description: definition.description.clone(),
                parameters: definition.input_schema.clone(),
            })
            .collect();
        let mut tool_calls = 0;
        for turn in 0..4 {
            let response = tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err(ToolError::Cancelled("delegate_task".into()));
                }
                response = self.models.generate(request.clone()) => {
                    response.map_err(|error| ToolError::execution("delegate_task", error.to_string(), true))?
                }
            };
            if response.tool_calls.is_empty() {
                let response = response.text();
                if response.trim().is_empty() {
                    return Err(ToolError::execution(
                        "delegate_task",
                        "sub-agent returned an empty response",
                        false,
                    ));
                }
                if response.len() > MAX_SUBAGENT_OUTPUT_BYTES {
                    return Err(ToolError::execution(
                        "delegate_task",
                        "sub-agent response exceeded 128 KiB",
                        false,
                    ));
                }
                return Ok(SubAgentOutcome {
                    profile,
                    response,
                    tool_calls,
                    turns: turn + 1,
                });
            }
            request.messages.push(ModelMessage::assistant_tool_calls(
                response.text(),
                response
                    .tool_calls
                    .iter()
                    .map(|call| ModelToolCall {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    })
                    .collect(),
            ));
            for call in response.tool_calls {
                let definition = definitions
                    .iter()
                    .find(|definition| definition.name == call.name)
                    .ok_or_else(|| {
                        ToolError::ToolNotFound(format!(
                            "sub-agent requested unknown tool {}",
                            call.name
                        ))
                    })?;
                let mut tool_request =
                    ToolRequest::new(definition.key.clone(), call.arguments.clone());
                tool_request.session_id = session_id;
                tool_request.subject = Some("sub-agent".into());
                let result = self.tools.execute(tool_request).await?;
                if result.status != ToolLifecycleStatus::Success {
                    return Err(ToolError::execution(
                        "delegate_task",
                        result
                            .error
                            .map(|error| error.message)
                            .unwrap_or_else(|| "read-only tool failed".into()),
                        false,
                    ));
                }
                request.messages.push(ModelMessage::tool_result(
                    call.id,
                    call.name,
                    serde_json::to_string(&result)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                ));
                tool_calls += 1;
            }
            request.created_at = chrono::Utc::now();
        }
        Err(ToolError::execution(
            "delegate_task",
            "sub-agent exceeded the four-turn read-only tool limit",
            false,
        ))
    }
}

pub(crate) fn provider(runtime: Arc<SubAgentRuntime>) -> StaticToolProvider {
    let provider = ToolProviderDefinition::new(
        "subagent",
        "Embedded Read-only Sub-agent",
        ToolProviderKind::Builtin,
    );
    let mut definition = ToolDefinition::new(
        "subagent",
        "delegate_task",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["task"],
            "properties": {
                "task": {"type": "string", "description": "Self-contained task for an isolated read-only sub-agent"},
                "profile": {"type": "string", "enum": ["general", "explore", "review"]}
            },
            "additionalProperties": false
        }),
    );
    definition.description = "Delegate a self-contained exploration or review to an isolated model context with only read-only workspace tools. The parent receives the final result.".into();
    definition.category = "agent.delegate".into();
    definition.default_permission = PermissionDecision::Ask;
    definition.timeout_ms = 240_000;
    let key = definition.key.clone();
    let tool = Arc::new(FunctionTool::new(key, move |request, context| {
        let runtime = runtime.clone();
        async move {
            let task = request
                .parameters
                .get("task")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ToolError::InvalidArgument("delegate_task task is required".into())
                })?;
            let profile =
                SubAgentProfile::parse(request.parameters.get("profile").and_then(Value::as_str))?;
            let outcome = runtime
                .run(task, profile, request.session_id, context.cancellation)
                .await?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(outcome)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));
    StaticToolProvider::new(provider, vec![ToolRegistration::new(definition, tool)])
}

fn subagent_tool_allowed(definition: &ToolDefinition) -> bool {
    definition.enabled
        && definition.default_permission == PermissionDecision::Allow
        && matches!(
            definition.category.as_str(),
            "filesystem.read" | "guidance.read" | "memory.read" | "process.read"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_distinct_bounded_prompts() {
        assert_ne!(
            SubAgentProfile::Explore.prompt(),
            SubAgentProfile::Review.prompt()
        );
        assert!(SubAgentProfile::parse(Some("unknown")).is_err());
    }

    #[test]
    fn subagent_only_receives_explicitly_read_only_tools() {
        let mut read =
            ToolDefinition::new("workspace", "read_file", "1.0.0", json!({"type":"object"}));
        read.category = "filesystem.read".into();
        read.default_permission = PermissionDecision::Allow;
        assert!(subagent_tool_allowed(&read));
        read.category = "filesystem.write".into();
        assert!(!subagent_tool_allowed(&read));
    }
}
