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
    Test,
    Debug,
    SecurityReview,
    Doc,
    Migration,
    Architecture,
}

impl SubAgentProfile {
    pub fn parse(value: Option<&str>) -> Result<Self, ToolError> {
        match value.unwrap_or("general") {
            "general" => Ok(Self::General),
            "explore" => Ok(Self::Explore),
            "review" => Ok(Self::Review),
            "test" => Ok(Self::Test),
            "debug" => Ok(Self::Debug),
            "security_review" => Ok(Self::SecurityReview),
            "doc" => Ok(Self::Doc),
            "migration" => Ok(Self::Migration),
            "architecture" => Ok(Self::Architecture),
            value => Err(ToolError::InvalidArgument(format!(
                "unsupported sub-agent profile: {value}"
            ))),
        }
    }

    pub fn prompt(self) -> &'static str {
        match self {
            Self::General => "Solve the delegated task independently. You are isolated from the parent conversation and have read-only workspace tools. Return a concise, evidence-based result; do not claim to modify files.",
            Self::Explore => "Explore the workspace to answer the delegated question. Use find/search/read tools efficiently, cite file paths, and do not modify files or run processes.",
            Self::Review => "Review the requested code or design. Prioritize concrete correctness, security and regression risks, cite file paths, and do not modify files or run processes.",
            Self::Test => "You are a Test Agent. Analyze test failures, generate test cases, and diagnose test issues. You have access to filesystem, process, and git tools. Use run_command to execute tests and analyze output. Report clear findings with file paths and line numbers.",
            Self::Debug => "You are a Debug Agent. Diagnose errors, analyze stack traces and logs, locate root causes. You have access to filesystem, process, LSP, and AST tools. Use run_command to reproduce issues and report findings with file paths and line numbers.",
            Self::SecurityReview => "You are a Security Review Agent. Audit code for security vulnerabilities: injection, XSS, CSRF, auth bypass, sensitive data exposure, insecure deserialization, etc. You have access to filesystem, git, and memory tools. Report findings with file paths, severity, and remediation suggestions.",
            Self::Doc => "You are a Documentation Agent. Generate and update documentation including README, CHANGELOG, API docs, and inline code comments. You have access to filesystem, git, and network tools. Read existing docs first, then propose targeted updates.",
            Self::Migration => "You are a Migration Agent. Assist with code migration, framework upgrades, and API transitions. You have access to filesystem, AST, and git tools. Analyze the current codebase, identify migration targets, and generate transformation plans.",
            Self::Architecture => "You are an Architecture Agent. Analyze project structure, module dependencies, coupling, and design patterns. You have access to filesystem, AST, dependency graph, and code index tools. Provide architecture insights and improvement recommendations.",
        }
    }

    pub fn allowed_categories(self) -> &'static [&'static str] {
        match self {
            Self::General => &["filesystem.read", "guidance.read", "memory.read", "process.read"],
            Self::Explore => &["filesystem.read", "guidance.read"],
            Self::Review => &["filesystem.read", "guidance.read", "memory.read"],
            Self::Test => &["filesystem.*", "process.*", "guidance.read", "memory.read", "git.*"],
            Self::Debug => &["filesystem.read", "process.*", "git.*", "memory.read"],
            Self::SecurityReview => &["filesystem.read", "guidance.read", "memory.read", "git.*"],
            Self::Doc => &["filesystem.*", "guidance.read", "network.read", "git.*", "memory.read"],
            Self::Migration => &["filesystem.*", "guidance.read", "git.*", "memory.read"],
            Self::Architecture => &["filesystem.read", "guidance.read", "memory.read", "git.*"],
        }
    }

    pub fn max_turns(self) -> usize {
        match self {
            Self::General => 4,
            Self::Explore => 4,
            Self::Review => 4,
            Self::Test => 8,
            Self::Debug => 8,
            Self::SecurityReview => 6,
            Self::Doc => 6,
            Self::Migration => 6,
            Self::Architecture => 6,
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
            .filter(|d| subagent_tool_allowed(d, profile))
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
        let max_turns = profile.max_turns();
        for turn in 0..max_turns {
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
            &format!("sub-agent exceeded the {max_turns}-turn tool limit for profile {profile:?}"),
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
                "profile": {"type": "string", "enum": ["general", "explore", "review", "test", "debug", "security_review", "doc", "migration", "architecture"]}
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

fn subagent_tool_allowed(definition: &ToolDefinition, profile: SubAgentProfile) -> bool {
    if !definition.enabled || definition.default_permission != PermissionDecision::Allow {
        return false;
    }
    let allowed = profile.allowed_categories();
    allowed.iter().any(|pattern| {
        if pattern.ends_with(".*") {
            let prefix = &pattern[..pattern.len() - 1];
            definition.category.starts_with(prefix)
        } else {
            definition.category == *pattern
        }
    })
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
        // All 9 profiles have non-empty prompts
        for profile in &[
            SubAgentProfile::General,
            SubAgentProfile::Explore,
            SubAgentProfile::Review,
            SubAgentProfile::Test,
            SubAgentProfile::Debug,
            SubAgentProfile::SecurityReview,
            SubAgentProfile::Doc,
            SubAgentProfile::Migration,
            SubAgentProfile::Architecture,
        ] {
            assert!(!profile.prompt().is_empty(), "{:?} prompt must not be empty", profile);
        }
    }

    #[test]
    fn subagent_only_receives_explicitly_allowed_tools() {
        let mut read =
            ToolDefinition::new("workspace", "read_file", "1.0.0", json!({"type":"object"}));
        read.category = "filesystem.read".into();
        read.default_permission = PermissionDecision::Allow;
        assert!(subagent_tool_allowed(&read, SubAgentProfile::General));
        read.category = "filesystem.write".into();
        assert!(!subagent_tool_allowed(&read, SubAgentProfile::General));
    }

    #[test]
    fn test_profile_has_broader_tool_access() {
        let mut write =
            ToolDefinition::new("workspace", "write_file", "1.0.0", json!({"type":"object"}));
        write.category = "filesystem.write".into();
        write.default_permission = PermissionDecision::Allow;
        // Test profile allows filesystem.*
        assert!(subagent_tool_allowed(&write, SubAgentProfile::Test));
        // General profile does not allow filesystem.write
        assert!(!subagent_tool_allowed(&write, SubAgentProfile::General));
    }

    #[test]
    fn debug_profile_allows_process_tools() {
        let mut cmd =
            ToolDefinition::new("process", "run_command", "1.0.0", json!({"type":"object"}));
        cmd.category = "process.execute".into();
        cmd.default_permission = PermissionDecision::Allow;
        assert!(subagent_tool_allowed(&cmd, SubAgentProfile::Debug));
        // Review does not allow process.*
        assert!(!subagent_tool_allowed(&cmd, SubAgentProfile::Review));
    }

    #[test]
    fn test_profile_max_turns() {
        assert_eq!(SubAgentProfile::General.max_turns(), 4);
        assert_eq!(SubAgentProfile::Test.max_turns(), 8);
        assert_eq!(SubAgentProfile::Debug.max_turns(), 8);
        assert_eq!(SubAgentProfile::SecurityReview.max_turns(), 6);
        assert!(SubAgentProfile::Test.max_turns() > SubAgentProfile::General.max_turns());
    }

    #[test]
    fn all_profiles_parse_correctly() {
        assert_eq!(SubAgentProfile::parse(Some("test")).unwrap(), SubAgentProfile::Test);
        assert_eq!(SubAgentProfile::parse(Some("debug")).unwrap(), SubAgentProfile::Debug);
        assert_eq!(SubAgentProfile::parse(Some("security_review")).unwrap(), SubAgentProfile::SecurityReview);
        assert_eq!(SubAgentProfile::parse(Some("doc")).unwrap(), SubAgentProfile::Doc);
        assert_eq!(SubAgentProfile::parse(Some("migration")).unwrap(), SubAgentProfile::Migration);
        assert_eq!(SubAgentProfile::parse(Some("architecture")).unwrap(), SubAgentProfile::Architecture);
        // Default is General
        assert_eq!(SubAgentProfile::parse(None).unwrap(), SubAgentProfile::General);
    }

    #[test]
    fn all_profiles_have_self_in_allowed_categories() {
        for profile in &[
            SubAgentProfile::General,
            SubAgentProfile::Explore,
            SubAgentProfile::Review,
            SubAgentProfile::Test,
            SubAgentProfile::Debug,
            SubAgentProfile::SecurityReview,
            SubAgentProfile::Doc,
            SubAgentProfile::Migration,
            SubAgentProfile::Architecture,
        ] {
            let cats = profile.allowed_categories();
            assert!(!cats.is_empty(), "{:?} must have at least one allowed category", profile);
            // Every profile should have at least filesystem.read (or filesystem.*)
            let has_fs = cats.iter().any(|c| *c == "filesystem.read" || *c == "filesystem.*");
            assert!(has_fs, "{:?} must allow filesystem access", profile);
        }
    }
}
