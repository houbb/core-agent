mod domain;
mod error;
mod runtime_bridge;
mod store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use core_agent::{
    project_storage_key, standard_config_manager, ConfigManager, ConfigModel, ConfigRequest,
    ConfigSourceInfo, ContextCandidateIndex, ContextCandidateSearch, EnterpriseAgent,
    EnterpriseAgentConfig, EnterpriseAgentEvent, EnterpriseApprovalDecision,
    EnterpriseApprovalHandler, EnterpriseApprovalRequest, EnterpriseCommandAction,
    InteractionCommandRegistry, PermissionMode, ResolvedConfig, UserConfigUpdate, UserConfigWriter,
    UserFileConfigProvider,
};
use tauri::{Emitter, Manager};

pub use domain::{
    AddReferenceRequest, AgentMessageRequest, AgentSubmission, ApprovalDecisionRequest, ChangeItem,
    CommandSuggestion, ContextUsage, ConversationMessage, DesktopWorkspaceSnapshot, MemoryItem,
    ModelSetting, PermissionModeRequest, PreferenceKind, ProjectNode, RuntimeRequest,
    SavePreferenceRequest, SaveSettingsRequest, SessionItem, SettingsSnapshot, ToolStatus,
    TraceStep, UiPreference, UsageSnapshot,
};
pub use error::{DesktopError, DesktopResult};
pub use store::DesktopPreferenceStore;

struct DesktopState {
    preferences: Arc<DesktopPreferenceStore>,
    app_data: std::path::PathBuf,
    runtime: tokio::sync::RwLock<DesktopRuntime>,
    runtime_operation: tokio::sync::Mutex<()>,
    approvals: Arc<DesktopApprovalBroker>,
}

#[derive(Clone)]
struct DesktopRuntime {
    agent: Arc<EnterpriseAgent>,
    workspace: std::path::PathBuf,
    config: core_agent::AgentConfig,
    resume_session: bool,
    permission_mode: String,
    config_sources: Vec<ConfigSourceInfo>,
    effective_config: serde_json::Value,
    context_index: Arc<ContextCandidateIndex>,
}

impl DesktopState {
    async fn agent(&self) -> Arc<EnterpriseAgent> {
        self.runtime.read().await.agent.clone()
    }
}

struct DesktopApprovalBroker {
    app: tauri::AppHandle,
    accepting: std::sync::atomic::AtomicBool,
    pending: Mutex<HashMap<uuid::Uuid, tokio::sync::oneshot::Sender<EnterpriseApprovalDecision>>>,
}

impl DesktopApprovalBroker {
    fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
            accepting: std::sync::atomic::AtomicBool::new(true),
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn app_handle(&self) -> tauri::AppHandle {
        self.app.clone()
    }

    fn resolve(
        &self,
        approval_id: uuid::Uuid,
        decision: EnterpriseApprovalDecision,
    ) -> DesktopResult<bool> {
        let sender = self
            .pending
            .lock()
            .map_err(|_| DesktopError::Agent("approval broker lock poisoned".into()))?
            .remove(&approval_id);
        Ok(sender.is_some_and(|sender| sender.send(decision).is_ok()))
    }

    fn deny_all(&self) {
        if let Ok(mut pending) = self.pending.lock() {
            for (_, sender) in pending.drain() {
                let _ = sender.send(EnterpriseApprovalDecision::Deny);
            }
        }
    }

    fn pause(&self) {
        self.accepting
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.deny_all();
    }

    fn resume(&self) {
        self.accepting
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl EnterpriseApprovalHandler for DesktopApprovalBroker {
    async fn decide(&self, request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        {
            let Ok(mut pending) = self.pending.lock() else {
                return EnterpriseApprovalDecision::Deny;
            };
            if !self.accepting.load(std::sync::atomic::Ordering::SeqCst) {
                return EnterpriseApprovalDecision::Deny;
            }
            pending.insert(request.id, sender);
        }
        if self.app.emit("agent-approval-required", request).is_err() {
            if let Ok(mut pending) = self.pending.lock() {
                pending.remove(&request.id);
            }
            return EnterpriseApprovalDecision::Deny;
        }
        let decision = tokio::time::timeout(std::time::Duration::from_secs(300), receiver).await;
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&request.id);
        }
        match decision {
            Ok(Ok(decision)) => decision,
            _ => EnterpriseApprovalDecision::Deny,
        }
    }
}

#[tauri::command]
fn list_preferences(state: tauri::State<'_, DesktopState>) -> DesktopResult<Vec<UiPreference>> {
    state.preferences.list()
}

#[tauri::command]
fn save_preference(
    state: tauri::State<'_, DesktopState>,
    request: SavePreferenceRequest,
) -> DesktopResult<UiPreference> {
    state.preferences.save(request, "desktop-user")
}

#[tauri::command]
async fn agent_context_candidates(
    state: tauri::State<'_, DesktopState>,
    query: String,
    limit: Option<usize>,
) -> DesktopResult<ContextCandidateSearch> {
    if query.len() > 1_024 || query.contains('\0') {
        return Err(DesktopError::Validation(
            "context candidate query is invalid".into(),
        ));
    }
    let index = state.runtime.read().await.context_index.clone();
    Ok(index.search(&query, limit.unwrap_or(100).min(500)))
}

#[tauri::command]
async fn agent_load_workspace(
    state: tauri::State<'_, DesktopState>,
    session_id: Option<uuid::Uuid>,
) -> DesktopResult<DesktopWorkspaceSnapshot> {
    let runtime = state.runtime.read().await.clone();
    let workspace = runtime
        .agent
        .workspace_snapshot()
        .await
        .map_err(agent_error)?;
    let project_tree = build_project_tree(&runtime.workspace, &workspace.resources);
    let sessions = runtime
        .agent
        .list_sessions()
        .await
        .map_err(agent_error)?
        .into_iter()
        .map(|session| SessionItem {
            session_id: session.session_id,
            title: session.title,
            state: session.state,
            updated_at: session.updated_at,
        })
        .collect();
    let tools = runtime
        .agent
        .tools()
        .list()
        .await
        .map_err(agent_error)?
        .into_iter()
        .map(|tool| ToolStatus {
            key: tool.key,
            name: tool.name,
            state: if tool.enabled {
                "READY".into()
            } else {
                "DISABLED".into()
            },
        })
        .collect();
    let trace = match session_id {
        Some(session_id) => trace_steps(session_id, runtime.agent.events(session_id).await),
        None => Vec::new(),
    };
    let context_usage = match session_id {
        Some(session_id) => latest_context_usage(runtime.agent.as_ref(), session_id).await,
        None => None,
    };
    let commands = InteractionCommandRegistry::with_builtins()
        .help()
        .into_iter()
        .map(|command| CommandSuggestion {
            name: command.name,
            usage: command.usage,
            summary: command.summary,
        })
        .collect();
    Ok(DesktopWorkspaceSnapshot {
        project_name: workspace.name,
        workspace_path: runtime.workspace.to_string_lossy().into_owned(),
        profile: "Coder".into(),
        model: runtime.agent.model_name().into(),
        project_tree,
        commands,
        changes: Vec::new(),
        trace,
        memory: Vec::new(),
        tools,
        sessions,
        resume_session: runtime.resume_session,
        permission_mode: runtime.permission_mode,
        config_sources: runtime.config_sources,
        effective_config: runtime.effective_config,
        context_usage,
    })
}

fn build_project_tree(
    root: &std::path::Path,
    resources: &[core_agent::Resource],
) -> Vec<ProjectNode> {
    let mut nodes = Vec::new();
    for resource in resources.iter().take(2_000) {
        let Some(path) = url::Url::parse(&resource.uri)
            .ok()
            .and_then(|value| value.to_file_path().ok())
        else {
            continue;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let components = relative
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(str::to_owned))
            .collect::<Vec<_>>();
        if components.is_empty() {
            continue;
        }
        insert_project_node(
            &mut nodes,
            &components,
            "",
            resource.id.to_string(),
            resource.resource_type == core_agent::ResourceType::Directory,
        );
    }
    sort_project_nodes(&mut nodes);
    nodes
}

fn insert_project_node(
    nodes: &mut Vec<ProjectNode>,
    components: &[String],
    prefix: &str,
    resource_id: String,
    final_is_directory: bool,
) {
    let name = &components[0];
    let current_path = if prefix.is_empty() {
        name.clone()
    } else {
        format!("{prefix}/{name}")
    };
    let is_directory = components.len() > 1 || final_is_directory;
    let index = nodes
        .iter()
        .position(|node| node.name == *name)
        .unwrap_or_else(|| {
            nodes.push(ProjectNode {
                id: if components.len() == 1 {
                    resource_id.clone()
                } else {
                    format!("dir:{current_path}")
                },
                name: name.clone(),
                path: current_path.clone(),
                kind: if is_directory { "directory" } else { "file" }.into(),
                children: Vec::new(),
            });
            nodes.len() - 1
        });
    if components.len() == 1 {
        nodes[index].id = resource_id;
        nodes[index].kind = if final_is_directory {
            "directory"
        } else {
            "file"
        }
        .into();
    } else {
        insert_project_node(
            &mut nodes[index].children,
            &components[1..],
            &current_path,
            resource_id,
            final_is_directory,
        );
    }
}

fn sort_project_nodes(nodes: &mut [ProjectNode]) {
    nodes.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    for node in nodes {
        sort_project_nodes(&mut node.children);
    }
}

async fn latest_context_usage(
    agent: &EnterpriseAgent,
    session_id: uuid::Uuid,
) -> Option<ContextUsage> {
    let snapshots = agent
        .contexts()
        .list_snapshots(&session_id.to_string(), 0, 1)
        .await
        .ok()?;
    let snapshot = snapshots.items.first()?;
    let context = agent
        .contexts()
        .load_context_snapshot(&snapshot.id)
        .await
        .ok()?;
    Some(ContextUsage {
        context_id: context.id.to_string(),
        total_tokens: context.total_tokens,
        max_tokens: agent.max_context_tokens(),
        build_duration_ms: context.build_duration_ms,
        estimated: true,
        distribution: serde_json::to_value(context.token_distribution).ok()?,
    })
}

#[tauri::command]
async fn agent_send_message(
    state: tauri::State<'_, DesktopState>,
    request: AgentMessageRequest,
) -> DesktopResult<AgentSubmission> {
    let _operation = state.runtime_operation.lock().await;
    let agent = state.agent().await;
    if request.message.starts_with('/') {
        if let Some(outcome) = agent
            .execute_command(&request.message, request.session_id)
            .await
            .map_err(agent_error)?
        {
            return Ok(AgentSubmission {
                session_id: outcome.session_id,
                response: Some(outcome.response),
                action: outcome.action,
                request_id: None,
                wall_duration_ms: None,
                active_duration_ms: None,
                telemetry_recorded: None,
            });
        }
    }
    let run = agent
        .run_with_approval(
            request.message,
            request.session_id,
            state.approvals.as_ref(),
        )
        .await
        .map_err(agent_error)?;

    // ── Spawn real-time event emitter for streaming Agent events ──
    let app_handle = state.approvals.app_handle();
    let event_agent = agent.clone();
    let session_id = run.session_id;
    tokio::spawn(async move {
        let mut rx = event_agent.subscribe_events();
        while let Ok(event) = rx.recv().await {
            let step = trace_step_from_event(session_id, &event);
            let _ = app_handle.emit("agent-event", step);
            if event.is_terminal() {
                break;
            }
        }
    });

    Ok(AgentSubmission {
        session_id: Some(run.session_id),
        response: None,
        action: EnterpriseCommandAction::None,
        request_id: Some(run.request_id),
        wall_duration_ms: Some(run.wall_duration_ms),
        active_duration_ms: Some(run.active_duration_ms),
        telemetry_recorded: Some(run.telemetry_recorded),
    })
}

#[tauri::command]
async fn agent_load_session(
    state: tauri::State<'_, DesktopState>,
    session_id: uuid::Uuid,
) -> DesktopResult<Vec<ConversationMessage>> {
    let agent = state.agent().await;
    let conversation = agent
        .sessions()
        .list_conversations(&session_id.to_string())
        .await
        .map_err(agent_error)?
        .into_iter()
        .find(|value| value.conversation_type == "MAIN")
        .ok_or_else(|| DesktopError::NotFound("MAIN conversation".into()))?;
    let total = agent
        .sessions()
        .list_messages(&conversation.id, 0, 1)
        .await
        .map_err(agent_error)?
        .total;
    let messages = agent
        .sessions()
        .list_messages(&conversation.id, total.saturating_sub(500), 500)
        .await
        .map_err(agent_error)?;
    Ok(messages
        .items
        .into_iter()
        .filter(|message| matches!(message.status.as_str(), "DONE" | "COMPLETED"))
        .filter_map(|message| {
            let role = match message.role.as_str() {
                "USER" => "user",
                "ASSISTANT" | "AGENT" => "agent",
                "SYSTEM" => "system",
                _ => return None,
            };
            Some(ConversationMessage {
                id: message.id,
                role: role.into(),
                content: message.content,
                created_at: message.created_at,
            })
        })
        .collect())
}

#[tauri::command]
async fn agent_load_settings(
    state: tauri::State<'_, DesktopState>,
) -> DesktopResult<SettingsSnapshot> {
    let runtime = state.runtime.read().await.clone();
    settings_snapshot(&runtime)
}

#[tauri::command]
async fn agent_save_settings(
    state: tauri::State<'_, DesktopState>,
    request: SaveSettingsRequest,
) -> DesktopResult<SettingsSnapshot> {
    let current = state.runtime.read().await.clone();
    let mut models = Vec::with_capacity(request.models.len());
    for mut input in request.models {
        input.api_key = input.api_key.filter(|value| !value.is_empty());
        input.api_key_ref = input.api_key_ref.filter(|value| !value.is_empty());
        let has_existing_secret = current.config.models.iter().any(|model| {
            model.name == input.name && (model.api_key.is_some() || model.api_key_ref.is_some())
        });
        if input.api_key.is_none() && input.api_key_ref.is_none() && !has_existing_secret {
            return Err(DesktopError::Validation(format!(
                "model {} requires an API key",
                input.name
            )));
        }
        models.push(ConfigModel {
            provider: input.provider,
            endpoint: input.base_url,
            profile: input
                .profile
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| input.name.clone()),
            name: input.name,
            max_context_tokens: input.max_context_tokens,
            api_key: input.api_key,
            api_key_ref: input.api_key_ref,
            stream: true,
        });
    }
    let update = UserConfigUpdate {
        active_model: request.active_model,
        models,
        compression: request.compression,
    };
    state.approvals.pause();
    let result = async {
        let _operation = state.runtime_operation.lock().await;
        UserConfigWriter::discover()
            .map_err(agent_error)?
            .save(&update, request.fingerprint.as_deref())
            .map_err(agent_error)?;
        let runtime = open_desktop_runtime(&state.app_data, &current.workspace).await?;
        let snapshot = settings_snapshot(&runtime)?;
        *state.runtime.write().await = runtime;
        Ok(snapshot)
    }
    .await;
    state.approvals.resume();
    result
}

#[tauri::command]
async fn agent_usage(state: tauri::State<'_, DesktopState>) -> DesktopResult<UsageSnapshot> {
    let agent = state.agent().await;
    Ok(UsageSnapshot {
        buckets: agent.usage_buckets(366).await.map_err(agent_error)?,
        requests: agent.request_metrics(0, 200).await.map_err(agent_error)?,
    })
}

#[tauri::command]
async fn agent_set_permission_mode(
    state: tauri::State<'_, DesktopState>,
    request: PermissionModeRequest,
) -> DesktopResult<String> {
    let mode = PermissionMode::parse(&request.mode).map_err(agent_error)?;
    let _operation = state.runtime_operation.lock().await;
    let agent = state.agent().await;
    agent.set_permission_mode(mode).await.map_err(agent_error)?;
    state.runtime.write().await.permission_mode = request.mode.clone();
    Ok(request.mode)
}

fn settings_snapshot(runtime: &DesktopRuntime) -> DesktopResult<SettingsSnapshot> {
    let file = UserConfigWriter::discover()
        .map_err(agent_error)?
        .snapshot()
        .map_err(agent_error)?;
    Ok(SettingsSnapshot {
        path: file.path.to_string_lossy().into_owned(),
        fingerprint: file.fingerprint,
        active_model: runtime.config.active_model.clone(),
        models: runtime
            .config
            .models
            .iter()
            .map(|model| ModelSetting {
                provider: model.provider.clone(),
                base_url: model.endpoint.clone(),
                name: model.name.clone(),
                profile: model.profile.clone(),
                max_context_tokens: model.max_context_tokens,
                api_key_configured: model.api_key.is_some() || model.api_key_ref.is_some(),
                api_key_ref: model.api_key_ref.clone(),
            })
            .collect(),
        compression: runtime.config.context.compression.clone(),
        sources: runtime.config_sources.clone(),
    })
}

#[tauri::command]
fn agent_decide_approval(
    state: tauri::State<'_, DesktopState>,
    request: ApprovalDecisionRequest,
) -> DesktopResult<bool> {
    let decision = match request.decision.as_str() {
        "ALLOW_ONCE" => EnterpriseApprovalDecision::AllowOnce,
        "DENY" => EnterpriseApprovalDecision::Deny,
        _ => {
            return Err(DesktopError::Validation(
                "approval decision must be ALLOW_ONCE or DENY".into(),
            ))
        }
    };
    state.approvals.resolve(request.approval_id, decision)
}

#[tauri::command]
async fn agent_session_events(
    state: tauri::State<'_, DesktopState>,
    session_id: uuid::Uuid,
) -> DesktopResult<Vec<TraceStep>> {
    let agent = state.agent().await;
    Ok(trace_steps(session_id, agent.events(session_id).await))
}

#[tauri::command]
async fn agent_add_reference(
    state: tauri::State<'_, DesktopState>,
    request: AddReferenceRequest,
) -> DesktopResult<serde_json::Value> {
    let agent = state.agent().await;
    let ctx = agent.contexts();
    let req = core_agent::AddReferenceRequest {
        session_id: request.session_id,
        reference_type: request.reference_type,
        locator: serde_json::Value::Null,
        snapshot: request.snapshot,
        metadata: None,
        path: request.path,
        start_line: request.start_line,
        end_line: request.end_line,
        content: request.content,
        message_id: request.message_id,
    };
    let resp = ctx.add_reference(req).await.map_err(agent_error)?;
    Ok(serde_json::to_value(&resp).map_err(agent_error)?)
}

#[tauri::command]
async fn agent_open_file(
    path: String,
    line: Option<usize>,
) -> DesktopResult<()> {
    if path.trim().is_empty() || path.len() > 32_768 || path.contains('\0') {
        return Err(DesktopError::Validation("file path is invalid".into()));
    }
    tauri_plugin_opener::open_path(&path, None::<&str>)
        .map_err(|e| DesktopError::Agent(e.to_string()))?;
    Ok(())
}

#[tauri::command]
async fn agent_open_workspace(
    state: tauri::State<'_, DesktopState>,
    path: String,
) -> DesktopResult<()> {
    if path.trim().is_empty() || path.len() > 32_768 || path.contains('\0') {
        return Err(DesktopError::Validation("workspace path is invalid".into()));
    }
    let workspace = std::fs::canonicalize(&path).map_err(agent_error)?;
    if !workspace.is_dir() {
        return Err(DesktopError::Validation(
            "workspace path must be a directory".into(),
        ));
    }
    let runtime = open_desktop_runtime(&state.app_data, &workspace).await?;
    state.approvals.pause();
    let _operation = state.runtime_operation.lock().await;
    *state.runtime.write().await = runtime;
    state.approvals.resume();
    record_recent_project(state.preferences.as_ref(), &workspace)?;
    Ok(())
}

fn record_recent_project(
    preferences: &DesktopPreferenceStore,
    workspace: &std::path::Path,
) -> DesktopResult<()> {
    let key = format!(
        "recent:{}",
        project_storage_key(workspace).map_err(agent_error)?
    );
    let current = preferences.find(&key)?;
    preferences.save(
        SavePreferenceRequest {
            key,
            kind: PreferenceKind::RecentProject,
            value: serde_json::json!({
                "path": workspace.to_string_lossy(),
                "name": workspace.file_name().and_then(|value| value.to_str()).unwrap_or("Workspace")
            }),
            expected_version: current.map(|value| value.version),
        },
        "desktop-user",
    )?;
    Ok(())
}

fn trace_steps(session_id: uuid::Uuid, events: Vec<EnterpriseAgentEvent>) -> Vec<TraceStep> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| trace_step(session_id, index, &event))
        .collect()
}

fn trace_step(session_id: uuid::Uuid, index: usize, event: &EnterpriseAgentEvent) -> TraceStep {
    let (kind, state) = match event.kind.as_str() {
        "execution_finished" => ("response", "COMPLETED"),
        "execution_failed" => ("error", "FAILED"),
        "cancelled" => ("cancelled", "CANCELLED"),
        _ => (event.kind.as_str(), "COMPLETED"),
    };
    let duration_ms = event
        .data
        .get("wallDurationMs")
        .or_else(|| event.data.pointer("/usage/latency_ms"))
        .and_then(serde_json::Value::as_u64);
    let tokens = event
        .data
        .pointer("/usage/total_tokens")
        .or_else(|| event.data.get("tokens"))
        .and_then(serde_json::Value::as_u64);
    TraceStep {
        id: format!("{session_id}:{index}"),
        kind: kind.into(),
        title: event.kind.replace('_', " "),
        state: state.into(),
        duration_ms,
        tokens,
        summary: matches!(kind, "response" | "error").then_some(event.message.clone()),
    }
}

fn trace_step_from_event(session_id: uuid::Uuid, event: &EnterpriseAgentEvent) -> TraceStep {
    trace_step(session_id, 0, event)
}

fn agent_error(error: impl std::fmt::Display) -> DesktopError {
    DesktopError::Agent(error.to_string())
}

async fn resolve_desktop_runtime_config(
    app_data: &std::path::Path,
    workspace: &std::path::Path,
    manager: &ConfigManager,
) -> DesktopResult<(EnterpriseAgentConfig, ResolvedConfig)> {
    let workspace = std::fs::canonicalize(workspace).map_err(agent_error)?;
    let resolved = manager
        .resolve(&ConfigRequest::new(&workspace))
        .await
        .map_err(agent_error)?;
    let project_key = project_storage_key(&workspace).map_err(agent_error)?;
    let mut runtime = EnterpriseAgentConfig::from_agent_config(
        app_data.join("projects").join(project_key).join("runtime"),
        workspace,
        &resolved.config,
    )
    .map_err(agent_error)?;
    runtime.entrypoint = "desktop".into();
    runtime.telemetry_dir = Some(
        UserFileConfigProvider::default_directory()
            .map_err(agent_error)?
            .join("runtime"),
    );
    Ok((runtime, resolved))
}

async fn open_desktop_runtime(
    app_data: &std::path::Path,
    workspace: &std::path::Path,
) -> DesktopResult<DesktopRuntime> {
    let manager = standard_config_manager().map_err(agent_error)?;
    let (config, resolved) = resolve_desktop_runtime_config(app_data, workspace, &manager).await?;
    let context_index = ContextCandidateIndex::build(workspace, 20_000).map_err(agent_error)?;
    let agent = EnterpriseAgent::open(config).await.map_err(agent_error)?;
    Ok(DesktopRuntime {
        agent: Arc::new(agent),
        workspace: workspace.to_path_buf(),
        config: resolved.config.clone(),
        resume_session: resolved.config.session.resume_last,
        permission_mode: resolved.config.permissions.mode.clone(),
        config_sources: resolved.sources.clone(),
        effective_config: resolved.redacted(),
        context_index: Arc::new(context_index),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let preferences = DesktopPreferenceStore::new(directory.join("desktop.db"))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let workspace = std::env::var_os("CORE_AGENT_WORKSPACE")
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            let runtime =
                tauri::async_runtime::block_on(open_desktop_runtime(&directory, &workspace))
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            record_recent_project(&preferences, &workspace)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let approvals = Arc::new(DesktopApprovalBroker::new(app.handle().clone()));
            app.manage(DesktopState {
                preferences: Arc::new(preferences),
                app_data: directory,
                runtime: tokio::sync::RwLock::new(runtime),
                runtime_operation: tokio::sync::Mutex::new(()),
                approvals,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_preferences,
            save_preference,
            agent_context_candidates,
            agent_open_file,
            agent_open_workspace,
            agent_load_workspace,
            agent_load_session,
            agent_send_message,
            agent_add_reference,
            agent_decide_approval,
            agent_session_events,
            agent_load_settings,
            agent_save_settings,
            agent_usage,
            agent_set_permission_mode,
            runtime_bridge::runtime_request
        ])
        .run(tauri::generate_context!())
        .expect("AgentOS Desktop failed to run");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn desktop_uses_shared_configuration_and_project_scoped_runtime_data() {
        let app_data = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(
            user.path().join("core-agent-config.yaml"),
            "model:\n  apiKey: desktop-secret\npermissions:\n  mode: strict\nsession:\n  resumeLast: true\n",
        )
        .unwrap();
        let manager = ConfigManager::builder()
            .provider(Arc::new(core_agent::UserFileConfigProvider::new(
                user.path(),
            )))
            .build()
            .unwrap();

        let (first_config, first_resolved) =
            resolve_desktop_runtime_config(app_data.path(), first.path(), &manager)
                .await
                .unwrap();
        let (second_config, _) =
            resolve_desktop_runtime_config(app_data.path(), second.path(), &manager)
                .await
                .unwrap();

        assert_ne!(first_config.data_dir, second_config.data_dir);
        assert_eq!(
            first_config.permission_mode,
            core_agent::PermissionMode::Strict
        );
        assert!(first_resolved.config.session.resume_last);
        assert!(!first_resolved
            .redacted()
            .to_string()
            .contains("desktop-secret"));
        assert!(first_config
            .data_dir
            .starts_with(app_data.path().join("projects")));
    }
}
