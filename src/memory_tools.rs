use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    FunctionTool, MemoryContent, MemoryEvent, MemoryEventKind, MemoryImportance, MemoryManager,
    MemoryQuery, MemoryRecallHit, MemorySourceKind, MemoryType, PermissionDecision, RawToolOutput,
    SqliteMemoryStore, ToolContent, ToolDefinition, ToolError, ToolRegistration,
};

const MAX_MEMORY_TOOL_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_MEMORY_BODY_BYTES: usize = 32 * 1024;
const MAX_MEMORY_TAGS: usize = 32;

pub(crate) fn persistent_manager(data_directory: &Path) -> Result<Arc<MemoryManager>, ToolError> {
    std::fs::create_dir_all(data_directory)
        .map_err(|error| ToolError::execution("memory", error.to_string(), false))?;
    let store = SqliteMemoryStore::new(data_directory.join("memory.db"))
        .map_err(|error| ToolError::execution("memory", error.to_string(), false))?;
    Ok(Arc::new(MemoryManager::new(Arc::new(store))))
}

pub(crate) fn project_namespace(workspace: &Path) -> Result<String, ToolError> {
    let canonical = std::fs::canonicalize(workspace)
        .map_err(|error| ToolError::execution("memory", error.to_string(), false))?;
    let normalized = canonical.to_string_lossy().replace('\\', "/");
    #[cfg(windows)]
    let normalized = normalized.to_ascii_lowercase();
    let digest = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    Ok(format!("project:{}", &digest[..24]))
}

pub(crate) fn registrations(
    manager: Arc<MemoryManager>,
    project_namespace: String,
) -> Vec<ToolRegistration> {
    let mut remember_definition = ToolDefinition::new(
        "memory",
        "remember_memory",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["title", "body"],
            "properties": {
                "title": {"type": "string", "description": "Short user-visible memory title"},
                "body": {"type": "string", "description": "Durable fact, preference, rule or project knowledge; secrets are rejected"},
                "scope": {"type": "string", "enum": ["project", "session"], "description": "Default project"},
                "type": {"type": "string", "enum": ["knowledge", "preference", "fact", "rule", "workspace"]},
                "importance": {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 32}
            },
            "additionalProperties": false
        }),
    );
    remember_definition.description = "Persist an explicit, governed memory for this project or session. Secret-like content is rejected and the write requires permission.".into();
    remember_definition.category = "memory.write".into();
    remember_definition.default_permission = PermissionDecision::Ask;
    let remember_key = remember_definition.key.clone();
    let remember_manager = manager.clone();
    let remember_project = project_namespace.clone();
    let remember_tool = Arc::new(FunctionTool::new(remember_key, move |request, context| {
        let manager = remember_manager.clone();
        let project_namespace = remember_project.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let input: RememberInput = parse_input("remember_memory", &request.parameters)?;
            validate_memory_input(&input)?;
            let namespace = scoped_namespace(
                input.scope.as_deref(),
                &project_namespace,
                request.session_id,
            )?;
            let memory_type = parse_memory_type(input.memory_type.as_deref())?;
            let mut event = MemoryEvent::new(
                namespace,
                MemorySourceKind::User,
                MemoryContent::new(input.title, input.body),
            );
            event.kind = memory_event_kind(memory_type);
            event.suggested_type = Some(memory_type);
            event.suggested_importance = Some(parse_importance(input.importance.as_deref())?);
            event.tags = input.tags.into_iter().collect();
            event.actor = "local-user".into();
            event.source.session_id = request.session_id;
            let remembered = manager
                .remember(event)
                .await
                .map_err(memory_tool_error("remember_memory"))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(match remembered.memory {
                    Some(memory) => json!({
                        "remembered": true,
                        "id": memory.id,
                        "namespace": memory.namespace,
                        "type": memory.memory_type,
                        "importance": memory.importance,
                        "version": memory.version,
                        "reason": remembered.reason
                    }),
                    None => json!({"remembered": false, "reason": remembered.reason}),
                })],
                ..RawToolOutput::default()
            })
        }
    }));

    let mut recall_definition = ToolDefinition::new(
        "memory",
        "recall_memory",
        "1.0.0",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Optional text query"},
                "scope": {"type": "string", "enum": ["project", "session"], "description": "Default project"},
                "limit": {"type": "integer", "minimum": 1, "maximum": 20}
            },
            "additionalProperties": false
        }),
    );
    recall_definition.description = "Recall bounded, relevant durable memories from project or session scope with stable ids and versions.".into();
    recall_definition.category = "memory.read".into();
    recall_definition.default_permission = PermissionDecision::Allow;
    let recall_key = recall_definition.key.clone();
    let recall_manager = manager.clone();
    let recall_project = project_namespace.clone();
    let recall_tool = Arc::new(FunctionTool::new(recall_key, move |request, context| {
        let manager = recall_manager.clone();
        let project_namespace = recall_project.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let input: RecallInput = parse_input("recall_memory", &request.parameters)?;
            let namespace = scoped_namespace(
                input.scope.as_deref(),
                &project_namespace,
                request.session_id,
            )?;
            let mut query = MemoryQuery::new(namespace);
            query.text = input.query.filter(|value| !value.trim().is_empty());
            query.limit = input.limit.unwrap_or(10).clamp(1, 20);
            query.actor = "local-user".into();
            let hits = manager
                .recall(query)
                .await
                .map_err(memory_tool_error("recall_memory"))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(bounded_hits(&hits))],
                ..RawToolOutput::default()
            })
        }
    }));

    let mut forget_definition = ToolDefinition::new(
        "memory",
        "forget_memory",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["id", "expected_version"],
            "properties": {
                "id": {"type": "string", "format": "uuid"},
                "expected_version": {"type": "integer", "minimum": 1}
            },
            "additionalProperties": false
        }),
    );
    forget_definition.description = "Forget one durable memory by id using optimistic version checking. Content and snapshots are removed and a tombstone remains for audit.".into();
    forget_definition.category = "memory.write".into();
    forget_definition.default_permission = PermissionDecision::Ask;
    let forget_key = forget_definition.key.clone();
    let forget_tool = Arc::new(FunctionTool::new(forget_key, move |request, context| {
        let manager = manager.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let input: ForgetInput = parse_input("forget_memory", &request.parameters)?;
            let id = Uuid::parse_str(&input.id).map_err(|error| {
                ToolError::InvalidArgument(format!("invalid memory id: {error}"))
            })?;
            let memory = manager
                .forget(id, input.expected_version, "local-user")
                .await
                .map_err(memory_tool_error("forget_memory"))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(json!({
                    "forgotten": true,
                    "id": memory.id,
                    "version": memory.version,
                    "state": memory.state
                }))],
                ..RawToolOutput::default()
            })
        }
    }));

    vec![
        ToolRegistration::new(remember_definition, remember_tool),
        ToolRegistration::new(recall_definition, recall_tool),
        ToolRegistration::new(forget_definition, forget_tool),
    ]
}

pub(crate) async fn recall_for_prompt(
    manager: &MemoryManager,
    project_namespace: &str,
    session_id: Uuid,
    text: &str,
    max_bytes: usize,
) -> Result<String, ToolError> {
    if text.trim().is_empty() || max_bytes == 0 {
        return Ok(String::new());
    }
    let mut all_hits = Vec::new();
    for namespace in [
        project_namespace.to_owned(),
        format!("session:{session_id}"),
    ] {
        let mut hits = recall_query(manager, &namespace, &truncate_chars(text, 4_096)).await?;
        if hits.is_empty() {
            for keyword in recall_keywords(text).into_iter().take(8) {
                hits = recall_query(manager, &namespace, &keyword).await?;
                if !hits.is_empty() {
                    break;
                }
            }
        }
        all_hits.append(&mut hits);
    }
    all_hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });
    all_hits.truncate(8);
    Ok(render_prompt_hits(&all_hits, max_bytes))
}

async fn recall_query(
    manager: &MemoryManager,
    namespace: &str,
    text: &str,
) -> Result<Vec<MemoryRecallHit>, ToolError> {
    let mut query = MemoryQuery::new(namespace);
    query.text = Some(text.into());
    query.limit = 6;
    query.actor = "agent-runtime".into();
    manager
        .recall(query)
        .await
        .map_err(memory_tool_error("memory_recall"))
}

fn recall_keywords(text: &str) -> Vec<String> {
    let stop_words = [
        "about", "after", "before", "from", "have", "into", "that", "the", "this", "what", "when",
        "where", "which", "with", "your",
    ];
    let mut keywords = text
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .map(str::to_lowercase)
        .filter(|word| word.chars().count() >= 4 && !stop_words.contains(&word.as_str()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    keywords.sort_by_key(|right| std::cmp::Reverse(right.chars().count()));
    keywords
}

fn render_prompt_hits(hits: &[MemoryRecallHit], max_bytes: usize) -> String {
    let mut output = String::new();
    for hit in hits {
        let item = format!(
            "- [{} v{}] {}: {}",
            hit.memory.id,
            hit.memory.version,
            hit.memory.content.title.trim(),
            hit.memory.content.body.trim()
        );
        if output.len() + item.len() + usize::from(!output.is_empty()) > max_bytes {
            break;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&item);
    }
    output
}

#[derive(Debug, Deserialize)]
struct RememberInput {
    title: String,
    body: String,
    scope: Option<String>,
    #[serde(rename = "type")]
    memory_type: Option<String>,
    importance: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RecallInput {
    query: Option<String>,
    scope: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ForgetInput {
    id: String,
    expected_version: u64,
}

fn validate_memory_input(input: &RememberInput) -> Result<(), ToolError> {
    if input.title.trim().is_empty() || input.title.len() > 512 {
        return Err(ToolError::InvalidArgument(
            "memory title must contain 1..=512 bytes".into(),
        ));
    }
    if input.body.trim().is_empty() || input.body.len() > MAX_MEMORY_BODY_BYTES {
        return Err(ToolError::InvalidArgument(
            "memory body must contain 1..=32768 bytes".into(),
        ));
    }
    if input.tags.len() > MAX_MEMORY_TAGS
        || input.tags.iter().any(|tag| {
            tag.trim().is_empty()
                || tag.len() > 64
                || tag.chars().any(|character| character.is_control())
        })
    {
        return Err(ToolError::InvalidArgument(
            "memory tags must contain at most 32 safe values of 1..=64 bytes".into(),
        ));
    }
    if looks_sensitive(&format!("{}\n{}", input.title, input.body)) {
        return Err(ToolError::PermissionDenied(
            "memory content looks like a credential or private key; durable secret storage is disabled"
                .into(),
        ));
    }
    Ok(())
}

fn scoped_namespace(
    scope: Option<&str>,
    project_namespace: &str,
    session_id: Option<Uuid>,
) -> Result<String, ToolError> {
    match scope.unwrap_or("project") {
        "project" => Ok(project_namespace.into()),
        "session" => session_id.map(|id| format!("session:{id}")).ok_or_else(|| {
            ToolError::InvalidArgument("session scope requires an active session".into())
        }),
        _ => Err(ToolError::InvalidArgument(
            "memory scope must be project or session".into(),
        )),
    }
}

fn parse_memory_type(value: Option<&str>) -> Result<MemoryType, ToolError> {
    match value.unwrap_or("knowledge") {
        "knowledge" => Ok(MemoryType::Knowledge),
        "preference" => Ok(MemoryType::Preference),
        "fact" => Ok(MemoryType::Fact),
        "rule" => Ok(MemoryType::Rule),
        "workspace" => Ok(MemoryType::Workspace),
        _ => Err(ToolError::InvalidArgument("unsupported memory type".into())),
    }
}

fn parse_importance(value: Option<&str>) -> Result<MemoryImportance, ToolError> {
    match value.unwrap_or("medium") {
        "low" => Ok(MemoryImportance::Low),
        "medium" => Ok(MemoryImportance::Medium),
        "high" => Ok(MemoryImportance::High),
        "critical" => Ok(MemoryImportance::Critical),
        _ => Err(ToolError::InvalidArgument(
            "unsupported memory importance".into(),
        )),
    }
}

fn memory_event_kind(memory_type: MemoryType) -> MemoryEventKind {
    match memory_type {
        MemoryType::Preference => MemoryEventKind::Preference,
        MemoryType::Fact => MemoryEventKind::Fact,
        _ => MemoryEventKind::Knowledge,
    }
}

fn bounded_hits(hits: &[MemoryRecallHit]) -> Value {
    let mut items = Vec::new();
    let mut used = 0_usize;
    let mut truncated = false;
    for hit in hits {
        let body = truncate_chars(&hit.memory.content.body, 8_192);
        let value = json!({
            "id": hit.memory.id,
            "version": hit.memory.version,
            "namespace": hit.memory.namespace,
            "type": hit.memory.memory_type,
            "importance": hit.memory.importance,
            "title": hit.memory.content.title,
            "body": body,
            "tags": hit.memory.tags,
            "score": hit.score,
            "matchedBy": hit.matched_by
        });
        let bytes = serde_json::to_vec(&value)
            .map(|value| value.len())
            .unwrap_or(usize::MAX);
        if used.saturating_add(bytes) > MAX_MEMORY_TOOL_OUTPUT_BYTES {
            truncated = true;
            break;
        }
        used += bytes;
        items.push(value);
    }
    json!({"memories": items, "count": items.len(), "truncated": truncated})
}

fn looks_sensitive(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("-----begin ") && normalized.contains("private key-----") {
        return true;
    }
    if value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|part| part.len() == 20 && part.starts_with("AKIA"))
    {
        return true;
    }
    if value
        .split(|character: char| character.is_whitespace() || matches!(character, '"' | '\''))
        .any(|part| {
            (part.starts_with("ghp_") || part.starts_with("github_pat_") || part.starts_with("sk-"))
                && part.len() >= 20
                || (part.starts_with("eyJ") && part.matches('.').count() == 2 && part.len() >= 32)
        })
    {
        return true;
    }
    for line in value.lines() {
        let normalized_line = line.to_ascii_lowercase();
        if [
            "api_key",
            "apikey",
            "access_token",
            "refresh_token",
            "client_secret",
            "password",
            "bearer",
        ]
        .iter()
        .any(|marker| normalized_line.contains(marker))
            && line
                .split(['=', ':', ' '])
                .any(|part| part.trim_matches(['\'', '"']).len() >= 16)
        {
            return true;
        }
    }
    false
}

fn parse_input<T>(tool: &str, parameters: &Value) -> Result<T, ToolError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(parameters.clone())
        .map_err(|error| ToolError::InvalidArgument(format!("{tool}: {error}")))
}

fn truncate_chars(value: &str, max: usize) -> String {
    let mut truncated = value.chars().take(max).collect::<String>();
    if value.chars().count() > max {
        truncated.push('…');
    }
    truncated
}

fn memory_tool_error(tool: &'static str) -> impl FnOnce(crate::MemoryError) -> ToolError + Copy {
    move |error| ToolError::execution(tool, error.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ToolExecutionContext, ToolRequest};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn secret_like_memory_is_rejected() {
        let input = RememberInput {
            title: "credential".into(),
            body: "OPENAI_API_KEY=abcdefghijklmnopqrstuvwxyz".into(),
            scope: None,
            memory_type: None,
            importance: None,
            tags: vec![],
        };
        assert!(validate_memory_input(&input).is_err());
        assert!(looks_sensitive(
            "token ghp_abcdefghijklmnopqrstuvwxyz0123456789"
        ));
    }

    #[test]
    fn recall_keywords_remove_noise_and_prioritize_specific_terms() {
        let keywords = recall_keywords("What is the Rust formatting rule?");
        assert_eq!(keywords.first().map(String::as_str), Some("formatting"));
        assert!(keywords.contains(&"rust".into()));
        assert!(!keywords.contains(&"what".into()));
    }

    #[tokio::test]
    async fn memory_tools_persist_recall_and_forget_across_managers() {
        let directory = tempfile::tempdir().unwrap();
        let manager = persistent_manager(directory.path()).unwrap();
        let tools = registrations(manager.clone(), "project:test".into());
        let remember = tools
            .iter()
            .find(|tool| tool.definition.name == "remember_memory")
            .unwrap();
        let mut request = ToolRequest::new(
            remember.definition.key.clone(),
            json!({
                "title": "Formatting",
                "body": "Use rustfmt before review",
                "type": "rule",
                "tags": ["rust"]
            }),
        );
        request.session_id = Some(Uuid::new_v4());
        let context = ToolExecutionContext {
            request_id: request.id,
            cancellation: CancellationToken::new(),
        };
        remember.tool.execute(&request, &context).await.unwrap();
        drop(tools);
        drop(manager);

        let reopened = persistent_manager(directory.path()).unwrap();
        let mut query = MemoryQuery::new("project:test");
        query.text = Some("rustfmt".into());
        query.actor = "test".into();
        let hits = reopened.recall(query).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].memory.content.title, "Formatting");
        let forgotten = reopened
            .forget(hits[0].memory.id, hits[0].memory.version, "test")
            .await
            .unwrap();
        assert_eq!(forgotten.content, MemoryContent::forgotten());
    }
}
