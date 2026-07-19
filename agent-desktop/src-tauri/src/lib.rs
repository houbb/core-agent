mod domain;
mod error;
mod runtime_bridge;
mod store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use core_agent::{
    project_storage_key, standard_config_manager, ConfigManager, ConfigRequest, ConfigSourceInfo,
    ContextCandidateIndex, ContextCandidateSearch, EnterpriseAgent, EnterpriseAgentConfig,
    EnterpriseAgentEvent, EnterpriseApprovalDecision, EnterpriseApprovalHandler,
    EnterpriseApprovalRequest, EnterpriseCommandAction, InteractionCommandRegistry, ResolvedConfig,
};
use tauri::{Emitter, Manager};

pub use domain::{
    AgentMessageRequest, AgentSubmission, ApprovalDecisionRequest, ChangeItem, CommandSuggestion,
    DesktopWorkspaceSnapshot, MemoryItem, PreferenceKind, ProjectNode, RuntimeRequest,
    SavePreferenceRequest, SessionItem, ToolStatus, TraceStep, UiPreference,
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
    let project_tree = workspace
        .resources
        .iter()
        .take(2_000)
        .map(|resource| ProjectNode {
            id: resource.id.to_string(),
            name: resource.name.clone(),
            path: resource.uri.clone(),
            kind: if resource.resource_type == core_agent::ResourceType::Directory {
                "directory".into()
            } else {
                "file".into()
            },
        })
        .collect();
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
    Ok(AgentSubmission {
        session_id: Some(run.session_id),
        response: None,
        action: EnterpriseCommandAction::None,
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
    Ok(())
}

fn trace_steps(session_id: uuid::Uuid, events: Vec<EnterpriseAgentEvent>) -> Vec<TraceStep> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let (kind, state) = match event.kind.as_str() {
                "execution_finished" => ("response", "COMPLETED"),
                "execution_failed" => ("error", "FAILED"),
                "cancelled" => ("cancelled", "CANCELLED"),
                _ => (event.kind.as_str(), "COMPLETED"),
            };
            let duration_ms = event
                .data
                .pointer("/usage/latency_ms")
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
                summary: matches!(kind, "response" | "error").then_some(event.message),
            }
        })
        .collect()
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
    let runtime = EnterpriseAgentConfig::from_agent_config(
        app_data.join("projects").join(project_key).join("runtime"),
        workspace,
        &resolved.config,
    )
    .map_err(agent_error)?;
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
            agent_open_workspace,
            agent_load_workspace,
            agent_send_message,
            agent_decide_approval,
            agent_session_events,
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
