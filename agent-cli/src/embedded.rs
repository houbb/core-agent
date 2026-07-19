use std::path::Path;
use std::sync::Arc;
use std::{io, io::IsTerminal};

use async_trait::async_trait;
use core_agent::{
    EnterpriseAgent, EnterpriseAgentConfig, EnterpriseApprovalDecision, EnterpriseApprovalHandler,
    EnterpriseApprovalRequest, EnterpriseSessionStatus,
};
use serde_json::json;
use uuid::Uuid;

use crate::config::agent_directory;
use crate::{
    AgentClient, AgentEvent, AgentRequest, CliConfig, CliError, CliResult, EventStream,
    ProfessionalAgentClient, ProfessionalRequest, ProfessionalResponse, ProjectSnapshot,
    SessionStatus, SessionSummary, Submission,
};

/// Same-process Terminal adapter. No background Agent service is required.
pub struct EmbeddedAgentClient {
    runtime: Arc<EnterpriseAgent>,
    workspace: String,
    approval: Arc<dyn EnterpriseApprovalHandler>,
}

impl EmbeddedAgentClient {
    pub async fn open(root: &Path, config: &CliConfig) -> CliResult<Self> {
        Self::open_with_approval(root, config, Arc::new(TerminalApprovalHandler)).await
    }

    pub async fn open_with_approval(
        root: &Path,
        config: &CliConfig,
        approval: Arc<dyn EnterpriseApprovalHandler>,
    ) -> CliResult<Self> {
        let workspace = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut agent_config = core_agent::AgentConfig::default();
        agent_config.model.provider = config.model.provider.clone();
        agent_config.model.endpoint = config.model.endpoint.clone();
        agent_config.model.name = config.model.name.clone();
        agent_config.model.profile = config.model.profile.clone();
        agent_config.model.max_context_tokens = config.model.max_context_tokens;
        agent_config.model.api_key = config.api_key();
        agent_config.permissions.mode = config.permissions.mode.clone();
        agent_config.memory.enabled = config.memory.enabled;
        agent_config.session.resume_last = config.session.resume_last;
        agent_config.context.max_mentions = config.context.max_mentions;
        agent_config.context.max_files = config.context.max_files;
        agent_config.context.max_file_bytes = config.context.max_file_bytes;
        agent_config.context.max_total_bytes = config.context.max_total_bytes;
        agent_config.context.max_directory_depth = config.context.max_directory_depth;
        agent_config.context.compression.strategy = config.context.compression_strategy.clone();
        agent_config.context.compression.trigger_percent =
            config.context.compression_trigger_percent;
        agent_config.context.compression.keep_recent_messages = config.context.keep_recent_messages;
        let mut runtime_config = EnterpriseAgentConfig::from_agent_config(
            agent_directory(root).join("runtime"),
            &workspace,
            &agent_config,
        )
        .map_err(|error| CliError::Configuration(error.to_string()))?;
        runtime_config.entrypoint = "terminal".into();
        runtime_config.telemetry_dir = Some(
            core_agent::UserFileConfigProvider::default_directory()
                .map_err(|error| CliError::Configuration(error.to_string()))?
                .join("runtime"),
        );
        let runtime = EnterpriseAgent::open(runtime_config)
            .await
            .map_err(api_error)?;
        Ok(Self {
            runtime: Arc::new(runtime),
            workspace: workspace.to_string_lossy().into_owned(),
            approval,
        })
    }

    pub fn from_runtime(runtime: Arc<EnterpriseAgent>, workspace: impl Into<String>) -> Self {
        Self {
            runtime,
            workspace: workspace.into(),
            approval: Arc::new(NonInteractiveApprovalHandler),
        }
    }

    pub fn runtime(&self) -> &Arc<EnterpriseAgent> {
        &self.runtime
    }

    async fn event_stream(&self, session_id: Uuid) -> CliResult<EventStream> {
        let events = self.runtime.events(session_id).await;
        let events = events.into_iter().map(|event| {
            let data = serde_json::to_value(&event)?;
            Ok(AgentEvent {
                kind: event.kind,
                message: event.message,
                data,
            })
        });
        Ok(Box::pin(futures_util::stream::iter(events)))
    }
}

#[async_trait]
impl AgentClient for EmbeddedAgentClient {
    async fn send(&self, request: AgentRequest) -> CliResult<Submission> {
        request.validate()?;
        let requested = Path::new(&request.workspace)
            .canonicalize()
            .unwrap_or_else(|_| request.workspace.clone().into());
        if requested.to_string_lossy() != self.workspace {
            return Err(CliError::InvalidArgument(
                "request workspace does not match the embedded Runtime workspace".into(),
            ));
        }
        let run = self
            .runtime
            .run_with_approval(request.message, request.session_id, self.approval.as_ref())
            .await
            .map_err(api_error)?;
        Ok(Submission {
            session_id: run.session_id,
            accepted: true,
        })
    }

    async fn stream(&self, session_id: Uuid) -> CliResult<EventStream> {
        self.event_stream(session_id).await
    }

    async fn resume(&self, session_id: Uuid) -> CliResult<EventStream> {
        self.runtime.resume(session_id).await.map_err(api_error)?;
        self.event_stream(session_id).await
    }

    async fn cancel(&self, session_id: Uuid) -> CliResult<bool> {
        self.runtime.cancel(session_id).await.map_err(api_error)
    }

    async fn status(&self, session_id: Uuid) -> CliResult<SessionStatus> {
        status(self.runtime.status(session_id).await.map_err(api_error)?)
    }

    async fn sessions(&self) -> CliResult<Vec<SessionSummary>> {
        self.runtime
            .list_sessions()
            .await
            .map_err(api_error)?
            .into_iter()
            .map(|item| {
                Ok(SessionSummary {
                    session_id: item.session_id,
                    state: item.state,
                    title: Some(item.title),
                })
            })
            .collect()
    }
}

#[async_trait]
impl ProfessionalAgentClient for EmbeddedAgentClient {
    async fn index_project(
        &self,
        project: ProjectSnapshot,
        profile: &str,
    ) -> CliResult<ProfessionalResponse> {
        let data = serde_json::to_value(&project)?;
        let mut items = vec![
            format!("Profile: {profile}"),
            format!("Root: {}", project.root),
        ];
        if !project.languages.is_empty() {
            items.push(format!(
                "Languages: {}",
                project
                    .languages
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !project.modules.is_empty() {
            items.push(format!("Modules: {}", project.modules.join(", ")));
        }
        Ok(ProfessionalResponse {
            summary: format!("Project {} indexed locally", project.name),
            items,
            data,
        })
    }

    async fn execute_professional(
        &self,
        request: ProfessionalRequest,
    ) -> CliResult<ProfessionalResponse> {
        let invocation = core_agent::InteractionCommandInvocation {
            name: request.invocation.name,
            arguments: request.invocation.arguments,
            route: request.invocation.route,
        };
        let line = invocation.to_line();
        if let Some(outcome) = self
            .runtime
            .execute_command(&line, request.session_id)
            .await
            .map_err(api_error)?
        {
            return Ok(ProfessionalResponse {
                summary: outcome.response,
                items: Vec::new(),
                data: outcome.data,
            });
        }
        let run = self
            .runtime
            .run_with_approval(line, request.session_id, self.approval.as_ref())
            .await
            .map_err(api_error)?;
        Ok(ProfessionalResponse {
            summary: run.response,
            items: vec![format!("Session: {}", run.session_id)],
            data: json!({"sessionId": run.session_id}),
        })
    }
}

struct NonInteractiveApprovalHandler;

#[async_trait]
impl EnterpriseApprovalHandler for NonInteractiveApprovalHandler {
    async fn decide(&self, _request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        EnterpriseApprovalDecision::Deny
    }
}

struct TerminalApprovalHandler;

#[async_trait]
impl EnterpriseApprovalHandler for TerminalApprovalHandler {
    async fn decide(&self, request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        if !io::stdin().is_terminal() {
            eprintln!(
                "Approval required for {} ({}) but stdin is not interactive.",
                request.tool, request.risk
            );
            return EnterpriseApprovalDecision::Deny;
        }
        eprintln!("\nApproval required");
        eprintln!("Tool: {}", request.tool);
        eprintln!("Risk: {}", request.risk);
        eprintln!("Reason: {}", request.reason);
        eprintln!(
            "Parameters: {}",
            serde_json::to_string_pretty(&request.parameters)
                .unwrap_or_else(|_| "[unavailable]".into())
        );
        eprint!("Allow once? [y/N] ");
        let _ = io::Write::flush(&mut io::stderr());
        let mut answer = String::new();
        match io::stdin().read_line(&mut answer) {
            Ok(_) if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") => {
                EnterpriseApprovalDecision::AllowOnce
            }
            _ => EnterpriseApprovalDecision::Deny,
        }
    }
}

fn status(value: EnterpriseSessionStatus) -> CliResult<SessionStatus> {
    Ok(SessionStatus {
        session_id: value.session_id,
        state: value.state,
        model: Some(value.model),
        memory_items: None,
    })
}

fn api_error(error: impl std::fmt::Display) -> CliError {
    CliError::Api(error.to_string())
}
