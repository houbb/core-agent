mod domain;
mod error;
mod runtime_bridge;
mod store;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use core_agent::{
    EnterpriseAgent, EnterpriseAgentConfig, EnterpriseAgentEvent, EnterpriseApprovalDecision,
    EnterpriseApprovalHandler, EnterpriseApprovalRequest, EnterpriseModelConfig, PermissionMode,
};
use tauri::{Emitter, Manager};

pub use domain::{
    AgentMessageRequest, AgentSubmission, ApprovalDecisionRequest, ChangeItem,
    DesktopWorkspaceSnapshot, MemoryItem, PreferenceKind, ProjectNode, RuntimeRequest,
    SavePreferenceRequest, SessionItem, ToolStatus, TraceStep, UiPreference,
};
pub use error::{DesktopError, DesktopResult};
pub use store::DesktopPreferenceStore;

struct DesktopState {
    preferences: Arc<DesktopPreferenceStore>,
    agent: Arc<EnterpriseAgent>,
    approvals: Arc<DesktopApprovalBroker>,
}

struct DesktopApprovalBroker {
    app: tauri::AppHandle,
    pending: Mutex<HashMap<uuid::Uuid, tokio::sync::oneshot::Sender<EnterpriseApprovalDecision>>>,
}

impl DesktopApprovalBroker {
    fn new(app: tauri::AppHandle) -> Self {
        Self {
            app,
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
}

#[async_trait::async_trait]
impl EnterpriseApprovalHandler for DesktopApprovalBroker {
    async fn decide(&self, request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if self
            .pending
            .lock()
            .map(|mut pending| pending.insert(request.id, sender))
            .is_err()
        {
            return EnterpriseApprovalDecision::Deny;
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
async fn agent_load_workspace(
    state: tauri::State<'_, DesktopState>,
    session_id: Option<uuid::Uuid>,
) -> DesktopResult<DesktopWorkspaceSnapshot> {
    let workspace = state
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
    let sessions = state
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
    let tools = state
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
        Some(session_id) => trace_steps(session_id, state.agent.events(session_id).await),
        None => Vec::new(),
    };
    Ok(DesktopWorkspaceSnapshot {
        project_name: workspace.name,
        profile: "Coder".into(),
        model: state.agent.model_name().into(),
        project_tree,
        changes: Vec::new(),
        trace,
        memory: Vec::new(),
        tools,
        sessions,
    })
}

#[tauri::command]
async fn agent_send_message(
    state: tauri::State<'_, DesktopState>,
    request: AgentMessageRequest,
) -> DesktopResult<AgentSubmission> {
    let run = state
        .agent
        .run_with_approval(
            request.message,
            request.session_id,
            state.approvals.as_ref(),
        )
        .await
        .map_err(agent_error)?;
    Ok(AgentSubmission {
        session_id: run.session_id,
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
    Ok(trace_steps(
        session_id,
        state.agent.events(session_id).await,
    ))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let preferences = DesktopPreferenceStore::new(directory.join("desktop.db"))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let workspace = std::env::var_os("CORE_AGENT_WORKSPACE")
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_dir()?);
            let mut config = EnterpriseAgentConfig::new(directory.join("runtime"), workspace);
            config.model = EnterpriseModelConfig {
                provider: std::env::var("CORE_AGENT_MODEL_PROVIDER")
                    .unwrap_or_else(|_| "ollama".into()),
                endpoint: std::env::var("CORE_AGENT_MODEL_ENDPOINT")
                    .unwrap_or_else(|_| "http://127.0.0.1:11434/v1".into()),
                api_key: std::env::var("CORE_AGENT_API_KEY").ok(),
                model: std::env::var("CORE_AGENT_MODEL").unwrap_or_else(|_| "qwen3".into()),
                profile: std::env::var("CORE_AGENT_MODEL_PROFILE")
                    .unwrap_or_else(|_| "default".into()),
            };
            config.permission_mode = PermissionMode::parse(
                &std::env::var("CORE_AGENT_PERMISSION_MODE")
                    .unwrap_or_else(|_| "risk-based".into()),
            )
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            let agent = tauri::async_runtime::block_on(EnterpriseAgent::open(config))
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let approvals = Arc::new(DesktopApprovalBroker::new(app.handle().clone()));
            app.manage(DesktopState {
                preferences: Arc::new(preferences),
                agent: Arc::new(agent),
                approvals,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_preferences,
            save_preference,
            agent_load_workspace,
            agent_send_message,
            agent_decide_approval,
            agent_session_events,
            runtime_bridge::runtime_request
        ])
        .run(tauri::generate_context!())
        .expect("AgentOS Desktop failed to run");
}
