use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use core_agent_agent::{AgentManager, RuntimeAgentCoordinator};
use core_agent_collaboration::{CollaborationPlatformManager, TeamProject};
use core_agent_config::AgentConfig;
use core_agent_context::{
    BuildContextRequest, Context, ContextRuntime, SqliteContextSnapshotStore,
};
use core_agent_ecosystem::{EcosystemManager, Publisher};
use core_agent_event::EventManager;
use core_agent_execution::ExecutionManager;
use core_agent_extension::ExtensionManager;
use core_agent_governance::{
    EnterpriseGovernanceManager, EnterprisePrincipal, IdentityProviderKind,
};
use core_agent_kernel::{ManagedRuntime, RuntimeKernel};
use core_agent_memory::MemoryManager;
use core_agent_model::{
    ModelCapability, ModelCatalog, ModelManager, ModelManagerBuilder, ModelMessage, ModelProfile,
    ModelProvider, ModelRequest, ModelRole, ModelToolCall, ModelToolDefinition,
    OpenAiCompatibleProvider, ProviderDefinition, SqliteModelStore, UsageCollector,
};
use core_agent_multi::MultiAgentManager;
use core_agent_plan::PlanningManager;
use core_agent_platform::{
    PlatformManager, PlatformOrganization, PlatformPolicy, PolicyEffect, PolicyRule, Tenant,
};
use core_agent_protocol::ProtocolRegistry;
use core_agent_session::{
    AppendMessageRequest, CreateSessionRequest, EventBus, MessageStatus, SessionResponse,
    SessionRuntime, SqliteSessionStore,
};
use core_agent_tool::{
    FunctionTool, PermissionDecision, RawToolOutput, StaticToolProvider, ToolDefinition, ToolError,
    ToolLifecycleStatus, ToolManager, ToolPermission, ToolProviderDefinition, ToolProviderKind,
    ToolRegistration, ToolRequest,
};
use core_agent_visual::VisualRegistry;
use core_agent_workflow::WorkflowManager;
use core_agent_workspace::{
    SqliteWorkspaceStore, Workspace, WorkspaceManager, WorkspaceOpenRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::{
    checkpoint::CheckpointStore, ContextMentionLimits, ContextMentionResolver,
    InteractionCommandRegistry, InteractionCommandRoute, InteractionEntryAction,
};

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are core-agent, a careful enterprise assistant. Use available context and tools safely.";

/// One-process product configuration shared by Terminal and Desktop.
#[derive(Debug, Clone)]
pub struct EnterpriseAgentConfig {
    pub data_dir: PathBuf,
    pub workspace: PathBuf,
    pub system_prompt: String,
    pub model: EnterpriseModelConfig,
    pub permission_mode: PermissionMode,
    pub memory_enabled: bool,
    pub context_mentions: ContextMentionLimits,
}

impl EnterpriseAgentConfig {
    pub fn new(data_dir: impl Into<PathBuf>, workspace: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            workspace: workspace.into(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            model: EnterpriseModelConfig::default(),
            permission_mode: PermissionMode::RiskBased,
            memory_enabled: true,
            context_mentions: ContextMentionLimits::default(),
        }
    }

    pub fn from_agent_config(
        data_dir: impl Into<PathBuf>,
        workspace: impl Into<PathBuf>,
        config: &AgentConfig,
    ) -> EnterpriseAgentResult<Self> {
        let mut runtime = Self::new(data_dir, workspace);
        runtime.model = EnterpriseModelConfig {
            provider: config.model.provider.clone(),
            endpoint: config.model.endpoint.clone(),
            api_key: config.model.api_key.clone(),
            model: config.model.name.clone(),
            profile: config.model.profile.clone(),
        };
        runtime.permission_mode = PermissionMode::parse(&config.permissions.mode)?;
        runtime.memory_enabled = config.memory.enabled;
        runtime.context_mentions = ContextMentionLimits {
            max_mentions: config.context.max_mentions,
            max_files: config.context.max_files,
            max_file_bytes: config.context.max_file_bytes,
            max_total_bytes: config.context.max_total_bytes,
            max_directory_depth: config.context.max_directory_depth,
        };
        runtime.validate()?;
        Ok(runtime)
    }

    fn validate(&self) -> EnterpriseAgentResult<()> {
        if self.system_prompt.trim().is_empty() || self.system_prompt.len() > 64 * 1024 {
            return Err(EnterpriseAgentError::Configuration(
                "system prompt must contain at most 64 KiB of text".into(),
            ));
        }
        if self.workspace.as_os_str().is_empty() || self.data_dir.as_os_str().is_empty() {
            return Err(EnterpriseAgentError::Configuration(
                "workspace and data directory must not be empty".into(),
            ));
        }
        self.model.validate()?;
        ContextMentionResolver::new(self.context_mentions.clone())
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionMode {
    Strict,
    RiskBased,
    Auto,
}

impl PermissionMode {
    pub fn parse(value: &str) -> EnterpriseAgentResult<Self> {
        match value {
            "strict" => Ok(Self::Strict),
            "risk-based" => Ok(Self::RiskBased),
            "auto" => Ok(Self::Auto),
            _ => Err(EnterpriseAgentError::Configuration(format!(
                "unsupported permission mode: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseApprovalRequest {
    pub id: Uuid,
    pub session_id: Uuid,
    pub tool: String,
    pub risk: String,
    pub reason: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnterpriseApprovalDecision {
    AllowOnce,
    Deny,
}

#[async_trait::async_trait]
pub trait EnterpriseApprovalHandler: Send + Sync {
    async fn decide(&self, request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision;
}

struct RejectApprovalHandler;

#[async_trait::async_trait]
impl EnterpriseApprovalHandler for RejectApprovalHandler {
    async fn decide(&self, _request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        EnterpriseApprovalDecision::Deny
    }
}

#[derive(Clone)]
pub struct EnterpriseModelConfig {
    pub provider: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub profile: String,
}

impl std::fmt::Debug for EnterpriseModelConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnterpriseModelConfig")
            .field("provider", &self.provider)
            .field("endpoint", &self.endpoint)
            .field("api_key_configured", &self.api_key.is_some())
            .field("model", &self.model)
            .field("profile", &self.profile)
            .finish()
    }
}

impl Default for EnterpriseModelConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            endpoint: "http://127.0.0.1:11434/v1".into(),
            api_key: None,
            model: "qwen3".into(),
            profile: "default".into(),
        }
    }
}

impl EnterpriseModelConfig {
    fn validate(&self) -> EnterpriseAgentResult<()> {
        if [
            self.provider.as_str(),
            self.endpoint.as_str(),
            self.model.as_str(),
            self.profile.as_str(),
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err(EnterpriseAgentError::Configuration(
                "model provider, endpoint, model and profile must not be empty".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseAgentEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

impl EnterpriseAgentEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "execution_finished" | "execution_failed" | "cancelled"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseRun {
    pub session_id: Uuid,
    pub response: String,
    pub events: Vec<EnterpriseAgentEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EnterpriseCommandAction {
    None,
    NewSession,
    ClearView,
    Exit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseCommandOutcome {
    pub response: String,
    pub action: EnterpriseCommandAction,
    pub session_id: Option<Uuid>,
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnterpriseSessionStatus {
    pub session_id: Uuid,
    pub state: String,
    pub title: String,
    pub model: String,
    pub updated_at: String,
}

/// Internal modules composed once per Terminal/Desktop process.
pub struct EnterpriseRuntimes {
    pub tenant_id: Uuid,
    pub organization_id: Uuid,
    pub collaboration_project_id: Uuid,
    pub planning: Arc<PlanningManager>,
    pub execution: Arc<ExecutionManager>,
    pub agents: Arc<AgentManager>,
    pub memory: Arc<MemoryManager>,
    pub events: Arc<EventManager>,
    pub workflows: Arc<WorkflowManager>,
    pub multi_agent: Arc<MultiAgentManager>,
    pub extensions: Arc<ExtensionManager>,
    pub platform: Arc<PlatformManager>,
    pub kernel: Arc<RuntimeKernel>,
    pub visual: Arc<VisualRegistry>,
    pub collaboration: Arc<CollaborationPlatformManager>,
    pub governance: Arc<EnterpriseGovernanceManager>,
    pub ecosystem: Arc<EcosystemManager>,
    pub protocols: Arc<ProtocolRegistry>,
}

/// The single application composition root. Runtime crates remain modules and
/// are never started as user-facing child processes.
pub struct EnterpriseAgent {
    config: EnterpriseAgentConfig,
    sessions: Arc<SessionRuntime<SqliteSessionStore>>,
    contexts: Arc<ContextRuntime<SqliteSessionStore>>,
    models: Arc<ModelManager>,
    tools: Arc<ToolManager>,
    workspaces: Arc<WorkspaceManager>,
    runtimes: EnterpriseRuntimes,
    events: RwLock<HashMap<Uuid, Vec<EnterpriseAgentEvent>>>,
    operation_lock: Mutex<()>,
    approvals: Arc<EnterpriseApprovalLedger>,
    checkpoints: Arc<CheckpointStore>,
}

#[derive(Default)]
struct EnterpriseApprovalLedger {
    approved: std::sync::Mutex<HashSet<Uuid>>,
}

impl EnterpriseApprovalLedger {
    fn approve(&self, request_id: Uuid) -> EnterpriseAgentResult<()> {
        self.approved
            .lock()
            .map_err(|_| EnterpriseAgentError::Runtime("approval ledger lock poisoned".into()))?
            .insert(request_id);
        Ok(())
    }
}

#[async_trait::async_trait]
impl ToolPermission for EnterpriseApprovalLedger {
    async fn check(
        &self,
        request: &ToolRequest,
        tool: &ToolDefinition,
    ) -> Result<PermissionDecision, ToolError> {
        match tool.default_permission {
            PermissionDecision::Allow | PermissionDecision::Deny => Ok(tool.default_permission),
            PermissionDecision::Ask => {
                let approved = self
                    .approved
                    .lock()
                    .map_err(|_| ToolError::Internal("approval ledger lock poisoned".into()))?
                    .remove(&request.id);
                Ok(if approved {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Ask
                })
            }
        }
    }
}

impl EnterpriseAgent {
    /// Opens the complete embedded Runtime with a real OpenAI-compatible model adapter.
    pub async fn open(config: EnterpriseAgentConfig) -> EnterpriseAgentResult<Self> {
        config.validate()?;
        std::fs::create_dir_all(&config.data_dir)?;

        let model_store = Arc::new(
            SqliteModelStore::new(&database_path(&config.data_dir, "model.db")?)
                .map_err(model_error)?,
        );
        let catalog: Arc<dyn ModelCatalog> = model_store.clone();
        let usage: Arc<dyn UsageCollector> = model_store;
        let provider: Arc<dyn ModelProvider> = Arc::new(
            OpenAiCompatibleProvider::new(
                config.model.provider.clone(),
                config.model.endpoint.clone(),
                config.model.api_key.clone(),
            )
            .map_err(model_error)?,
        );
        let models = Arc::new(
            ModelManagerBuilder::new(catalog)
                .add_provider(provider)
                .with_usage_collector(usage)
                .build()
                .map_err(model_error)?,
        );

        let mut provider =
            ProviderDefinition::new(config.model.provider.clone(), config.model.provider.clone());
        provider.endpoint = Some(config.model.endpoint.clone());
        models
            .upsert_provider(&provider)
            .await
            .map_err(model_error)?;
        let profile = ModelProfile::new(
            config.model.profile.clone(),
            config.model.provider.clone(),
            config.model.model.clone(),
        )
        .with_capability(ModelCapability::Chat);
        models.upsert_profile(&profile).await.map_err(model_error)?;

        Self::with_model(config, models).await
    }

    /// Injection seam used by deterministic E2E tests and private model adapters.
    pub async fn with_model(
        config: EnterpriseAgentConfig,
        models: Arc<ModelManager>,
    ) -> EnterpriseAgentResult<Self> {
        config.validate()?;
        std::fs::create_dir_all(&config.data_dir)?;
        let session_store = Arc::new(
            SqliteSessionStore::new(&database_path(&config.data_dir, "session.db")?)
                .map_err(session_error)?,
        );
        let snapshot_store = Arc::new(
            SqliteContextSnapshotStore::new(&database_path(&config.data_dir, "context.db")?)
                .map_err(context_error)?,
        );
        let sessions = Arc::new(SessionRuntime::new(
            session_store.clone(),
            Arc::new(EventBus::new(256)),
        ));
        let contexts = Arc::new(ContextRuntime::new(session_store, Some(snapshot_store)));
        let workspace_store = Arc::new(
            SqliteWorkspaceStore::new(&database_path(&config.data_dir, "workspace.db")?)
                .map_err(workspace_error)?,
        );
        let approvals = Arc::new(EnterpriseApprovalLedger::default());
        let tools = Arc::new(ToolManager::builder().permission(approvals.clone()).build());
        let checkpoints = Arc::new(
            CheckpointStore::new(&config.workspace, config.data_dir.join("checkpoints"))
                .map_err(checkpoint_error)?,
        );
        register_workspace_tools(&tools, &config.workspace, checkpoints.clone()).await?;
        let planning = Arc::new(PlanningManager::builder().build());
        let execution = Arc::new(
            ExecutionManager::builder()
                .executor(Arc::new(crate::integrations::ToolActionExecutor::new(
                    tools.clone(),
                )))
                .build(),
        );
        let agents = Arc::new(
            AgentManager::builder()
                .coordinator(Arc::new(RuntimeAgentCoordinator::new(
                    planning.clone(),
                    execution.clone(),
                )))
                .build(),
        );
        let platform = Arc::new(PlatformManager::builder().build());
        let kernel = Arc::new(RuntimeKernel::builder().build());
        let platform_adapter = Arc::new(crate::integrations::PlatformKernelRuntime::new(
            platform.clone(),
        ));
        let platform_descriptor = platform_adapter.descriptor();
        kernel
            .register(platform_adapter, None)
            .await
            .map_err(runtime_error)?;
        kernel.start().await.map_err(runtime_error)?;

        let tenant = platform
            .create_tenant(Tenant::new("local", "Local Workspace", "desktop-user"))
            .await
            .map_err(runtime_error)?;
        let organization = platform
            .create_organization(PlatformOrganization::new(
                tenant.id,
                "local",
                "Local Organization",
                "desktop-user",
            ))
            .await
            .map_err(runtime_error)?;
        let mut policy = PlatformPolicy::new(
            tenant.id,
            "local-desktop",
            "Local Desktop Policy",
            "desktop-user",
        );
        policy.rules.push(PolicyRule {
            id: Uuid::new_v4(),
            subjects: BTreeSet::from(["desktop-user".into()]),
            actions: BTreeSet::from(["*".into()]),
            resources: BTreeSet::from(["*".into()]),
            attributes: Default::default(),
            effect: PolicyEffect::Allow,
            priority: 100,
        });
        platform
            .create_policy(policy)
            .await
            .map_err(runtime_error)?;

        let visual = Arc::new(VisualRegistry::default());
        let visual_descriptor = visual
            .register(crate::integrations::platform_visual_descriptor(), None)
            .map_err(runtime_error)?;
        let protocols = Arc::new(ProtocolRegistry::default());
        protocols
            .register(
                crate::integrations::kernel_runtime_protocol(
                    &platform_descriptor,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ),
                "system",
            )
            .map_err(runtime_error)?;
        protocols
            .register(
                crate::integrations::visual_descriptor_protocol(&visual_descriptor),
                "system",
            )
            .map_err(runtime_error)?;
        let collaboration = Arc::new(CollaborationPlatformManager::default());
        let collaboration_project = collaboration
            .create_project(TeamProject::new("local", "Local Workspace", "desktop-user"))
            .map_err(runtime_error)?;
        let governance = Arc::new(EnterpriseGovernanceManager::new(platform.clone()));
        governance
            .bind_principal(EnterprisePrincipal::new(
                tenant.id,
                "desktop-user",
                IdentityProviderKind::LocalAdapter,
                "Desktop User",
                "desktop-user",
            ))
            .await
            .map_err(runtime_error)?;
        let ecosystem = Arc::new(EcosystemManager::new(platform.clone()));
        ecosystem
            .register_publisher(Publisher::new(
                tenant.id,
                "local",
                "Local Publisher",
                "desktop-user",
                "desktop-user",
            ))
            .await
            .map_err(runtime_error)?;
        let runtimes = EnterpriseRuntimes {
            tenant_id: tenant.id,
            organization_id: organization.id,
            collaboration_project_id: collaboration_project.id,
            planning,
            execution,
            agents,
            memory: Arc::new(MemoryManager::builder().build()),
            events: Arc::new(EventManager::builder().build()),
            workflows: Arc::new(WorkflowManager::builder().build()),
            multi_agent: Arc::new(MultiAgentManager::builder().build()),
            extensions: Arc::new(ExtensionManager::builder().build()),
            platform,
            kernel,
            visual,
            collaboration,
            governance,
            ecosystem,
            protocols,
        };
        Ok(Self {
            config,
            sessions,
            contexts,
            models,
            tools,
            workspaces: Arc::new(WorkspaceManager::new(workspace_store)),
            runtimes,
            events: RwLock::new(HashMap::new()),
            operation_lock: Mutex::new(()),
            approvals,
            checkpoints,
        })
    }

    pub fn sessions(&self) -> &SessionRuntime<SqliteSessionStore> {
        &self.sessions
    }

    pub fn contexts(&self) -> &ContextRuntime<SqliteSessionStore> {
        &self.contexts
    }

    pub fn models(&self) -> &ModelManager {
        &self.models
    }

    pub fn tools(&self) -> Arc<ToolManager> {
        self.tools.clone()
    }

    pub fn runtimes(&self) -> &EnterpriseRuntimes {
        &self.runtimes
    }

    pub fn model_name(&self) -> &str {
        &self.config.model.model
    }

    /// Executes all zero-model built-ins from the same registry used by every
    /// product entry. Agent-routed commands return `None` and are handled by
    /// `run_with_approval`, which applies the shared prompt expansion.
    pub async fn execute_command(
        &self,
        line: &str,
        session_id: Option<Uuid>,
    ) -> EnterpriseAgentResult<Option<EnterpriseCommandOutcome>> {
        let registry = InteractionCommandRegistry::with_builtins();
        let invocation = registry
            .parse(line)
            .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?;
        if invocation.route == InteractionCommandRoute::Agent {
            return Ok(None);
        }
        let mut outcome = EnterpriseCommandOutcome {
            response: String::new(),
            action: EnterpriseCommandAction::None,
            session_id,
            data: json!({}),
        };
        if let Some(entry) = registry
            .execute_entry(&invocation)
            .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?
        {
            outcome.response = entry.response;
            match entry.action {
                InteractionEntryAction::None => {}
                InteractionEntryAction::NewSession => {
                    outcome.action = EnterpriseCommandAction::NewSession;
                    outcome.session_id = None;
                }
                InteractionEntryAction::ClearView => {
                    outcome.action = EnterpriseCommandAction::ClearView;
                }
                InteractionEntryAction::Exit => {
                    outcome.action = EnterpriseCommandAction::Exit;
                }
                InteractionEntryAction::Profile(requested) => {
                    outcome.response = if let Some(requested) = requested {
                        format!(
                            "Profile changes are configuration-backed; current profile is {} (requested {}).",
                            self.config.model.profile, requested
                        )
                    } else {
                        format!("Profile: {}", self.config.model.profile)
                    };
                }
            }
            return Ok(Some(outcome));
        }
        match invocation.name.as_str() {
            "project" => {
                let workspace = self.workspace_snapshot().await?;
                outcome.response = format!(
                    "Project {}: {} indexed resources",
                    workspace.name,
                    workspace.resources.len()
                );
                outcome.data = json!({
                    "name": workspace.name,
                    "uri": workspace.uri,
                    "resources": workspace.resources.len(),
                    "projects": workspace.projects.len(),
                });
            }
            "tasks" | "sessions" | "history" => {
                let mut sessions = self.list_sessions().await?;
                if let Some(query) = invocation.arguments.first() {
                    let query = query.to_ascii_lowercase();
                    sessions.retain(|session| {
                        session.title.to_ascii_lowercase().contains(&query)
                            || session.session_id.to_string().contains(&query)
                    });
                }
                outcome.response = if sessions.is_empty() {
                    "No Agent sessions found.".into()
                } else {
                    sessions
                        .iter()
                        .map(|session| {
                            format!(
                                "{}  {}  {}",
                                session.session_id, session.state, session.title
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                outcome.data = serde_json::to_value(sessions)?;
            }
            "status" => {
                if let Some(session_id) = session_id {
                    let status = self.status(session_id).await?;
                    outcome.response = format!(
                        "Session {} is {} on {}",
                        status.session_id, status.state, status.model
                    );
                    outcome.data = serde_json::to_value(status)?;
                } else {
                    outcome.response = "No active session. Send a message or use /new.".into();
                }
            }
            "tools" => {
                let tools = self.tools.list().await.map_err(tool_error)?;
                outcome.response = tools
                    .iter()
                    .map(|tool| format!("{} — {}", tool.key, tool.description))
                    .collect::<Vec<_>>()
                    .join("\n");
                outcome.data = json!({"count": tools.len()});
            }
            "memory" => {
                outcome.response = if self.config.memory_enabled {
                    "Project memory is enabled in the embedded Runtime."
                } else {
                    "Project memory is disabled by configuration."
                }
                .into();
                outcome.data = json!({"enabled": self.config.memory_enabled});
            }
            "config" => {
                outcome.data = json!({
                    "model": {
                        "provider": self.config.model.provider,
                        "endpoint": self.config.model.endpoint,
                        "name": self.config.model.model,
                        "profile": self.config.model.profile,
                        "apiKeyConfigured": self.config.model.api_key.is_some(),
                    },
                    "permissionMode": permission_mode_name(self.config.permission_mode),
                    "memory": {"enabled": self.config.memory_enabled},
                    "context": {
                        "maxMentions": self.config.context_mentions.max_mentions,
                        "maxFiles": self.config.context_mentions.max_files,
                        "maxFileBytes": self.config.context_mentions.max_file_bytes,
                        "maxTotalBytes": self.config.context_mentions.max_total_bytes,
                        "maxDirectoryDepth": self.config.context_mentions.max_directory_depth,
                    }
                });
                outcome.response = serde_json::to_string_pretty(&outcome.data)?;
            }
            "undo" | "redo" => {
                let Some(session_id) = session_id else {
                    outcome.response = format!(
                        "No active session. /{} only applies to Agent file checkpoints.",
                        invocation.name
                    );
                    return Ok(Some(outcome));
                };
                let _operation = self.operation_lock.lock().await;
                let checkpoint = if invocation.name == "undo" {
                    self.checkpoints.undo(session_id)
                } else {
                    self.checkpoints.redo(session_id)
                }
                .map_err(checkpoint_error)?;
                if let Some(checkpoint) = checkpoint {
                    outcome.response = format!(
                        "{} checkpoint {} across {} file(s).",
                        if invocation.name == "undo" {
                            "Undid"
                        } else {
                            "Redid"
                        },
                        checkpoint.checkpoint_id,
                        checkpoint.files
                    );
                    outcome.data = json!({
                        "checkpointId": checkpoint.checkpoint_id,
                        "files": checkpoint.files,
                        "operation": invocation.name,
                    });
                    self.events
                        .write()
                        .await
                        .entry(session_id)
                        .or_default()
                        .push(EnterpriseAgentEvent {
                            kind: if invocation.name == "undo" {
                                "checkpoint_undone"
                            } else {
                                "checkpoint_redone"
                            }
                            .into(),
                            message: outcome.response.clone(),
                            data: outcome.data.clone(),
                        });
                } else {
                    outcome.response =
                        format!("No file checkpoint is available to {}.", invocation.name);
                }
            }
            _ => {
                return Err(EnterpriseAgentError::InvalidArgument(format!(
                    "unsupported zero-model command /{}",
                    invocation.name
                )))
            }
        }
        Ok(Some(outcome))
    }

    pub async fn workspace_snapshot(&self) -> EnterpriseAgentResult<Workspace> {
        let name = self
            .config
            .workspace
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("workspace");
        let request = WorkspaceOpenRequest::local(name, &self.config.workspace)
            .map_err(workspace_error)?
            .actor("local-user");
        self.workspaces.open(request).await.map_err(workspace_error)
    }

    pub async fn run(
        &self,
        message: impl Into<String>,
        session_id: Option<Uuid>,
    ) -> EnterpriseAgentResult<EnterpriseRun> {
        self.run_with_approval(message, session_id, &RejectApprovalHandler)
            .await
    }

    pub async fn run_with_approval(
        &self,
        message: impl Into<String>,
        session_id: Option<Uuid>,
        approval_handler: &dyn EnterpriseApprovalHandler,
    ) -> EnterpriseAgentResult<EnterpriseRun> {
        let message = message.into();
        validate_message(&message)?;
        let (command_context, read_only) = if message.starts_with('/') {
            let invocation = InteractionCommandRegistry::with_builtins()
                .parse(&message)
                .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?;
            if invocation.route != InteractionCommandRoute::Agent {
                return Err(EnterpriseAgentError::InvalidArgument(format!(
                    "/{} must be executed through execute_command",
                    invocation.name
                )));
            }
            (
                Some(
                    invocation
                        .model_prompt(&self.config.workspace)
                        .map_err(|error| {
                            EnterpriseAgentError::InvalidArgument(error.to_string())
                        })?,
                ),
                invocation.is_read_only(),
            )
        } else {
            (None, false)
        };
        let mentions = ContextMentionResolver::new(self.config.context_mentions.clone())
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?
            .resolve(&self.config.workspace, &message)
            .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?;
        let mention_context = mentions
            .context_text()
            .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?;
        let explicit_context = match (command_context, mention_context) {
            (Some(command), Some(mentions)) => Some(format!(
                "Built-in command expansion:\n{command}\n\nExplicit @ context:\n{mentions}"
            )),
            (Some(command), None) => Some(command),
            (None, Some(mentions)) => Some(mentions),
            (None, None) => None,
        };
        let _operation = self.operation_lock.lock().await;
        let session = self.ensure_session(session_id, &message).await?;
        let session_id = parse_uuid(&session.id, "session")?;
        self.checkpoints
            .begin_turn(session_id)
            .map_err(checkpoint_error)?;
        let conversation = self
            .sessions
            .list_conversations(&session.id)
            .await
            .map_err(session_error)?
            .into_iter()
            .find(|item| item.conversation_type == "MAIN")
            .ok_or_else(|| {
                EnterpriseAgentError::Session(format!(
                    "session {} does not have a MAIN conversation",
                    session.id
                ))
            })?;

        self.append_completed_message(&conversation.id, "USER", &message)
            .await?;
        let mut events = vec![EnterpriseAgentEvent {
            kind: "execution_started".into(),
            message: "Enterprise Agent accepted the request".into(),
            data: json!({"sessionId": session_id, "readOnly": read_only}),
        }];
        if !mentions.is_empty() {
            events.push(EnterpriseAgentEvent {
                kind: "context_mentions_resolved".into(),
                message: format!("Resolved {} explicit context file(s)", mentions.files.len()),
                data: json!({
                    "mentions": mentions.explicit_mentions,
                    "files": mentions.files,
                    "totalBytes": mentions.total_bytes,
                }),
            });
        }

        let context = match self
            .contexts
            .build(BuildContextRequest {
                session_id: session.id.clone(),
                conversation_id: Some(conversation.id.clone()),
                system_prompt: Some(self.config.system_prompt.clone()),
                user_input: explicit_context,
                max_messages: Some(100),
                max_tokens: Some(128_000),
                working_directory: Some(self.config.workspace.to_string_lossy().into_owned()),
            })
            .await
        {
            Ok(context) => context,
            Err(error) => {
                let error = context_error(error);
                self.record_failure(session_id, &mut events, &error).await;
                return Err(error);
            }
        };
        events.push(EnterpriseAgentEvent {
            kind: "context_built".into(),
            message: "Session context assembled".into(),
            data: json!({"contextId": context.id, "tokens": context.total_tokens, "hash": context.hash}),
        });

        let definitions = self.tools.list().await.map_err(tool_error)?;
        let mut request = context_model_request(&context, &self.config.model.profile, session_id)?;
        let exposed_definitions = definitions
            .iter()
            .filter(|definition| !read_only || definition.category != "filesystem.write")
            .cloned()
            .collect::<Vec<_>>();
        request.tools = model_tool_definitions(&exposed_definitions)?;
        let mut response_text = None;
        let mut tool_call_count = 0_usize;
        for turn in 0..8_u8 {
            let response = match self.models.generate(request.clone()).await {
                Ok(response) => response,
                Err(error) => {
                    let error = model_error(error);
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
            };
            events.push(EnterpriseAgentEvent {
                kind: "model_completed".into(),
                message: "Model inference completed".into(),
                data: json!({
                    "turn": turn + 1,
                    "provider": response.provider,
                    "model": response.model,
                    "profile": response.profile,
                    "usage": response.usage,
                    "toolCalls": response.tool_calls.len(),
                }),
            });
            if response.tool_calls.is_empty() {
                response_text = Some(response.text());
                break;
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
            for call in &response.tool_calls {
                let definition = resolve_tool_definition(&definitions, &call.name)?;
                if read_only
                    && (definition.category == "filesystem.write"
                        || (definition.category == "process.execute"
                            && !safe_command(&call.arguments)))
                {
                    let error = EnterpriseAgentError::Tool(format!(
                        "tool {} is denied by the read-only command boundary",
                        definition.name
                    ));
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
                let mut tool_request =
                    ToolRequest::new(definition.key.clone(), call.arguments.clone());
                tool_request.session_id = Some(session_id);
                tool_request.subject = Some("local-user".into());
                let permission = tool_permission_requirement(
                    self.config.permission_mode,
                    definition,
                    &call.arguments,
                );
                if permission == PermissionDecision::Deny {
                    let error = EnterpriseAgentError::Tool(format!(
                        "tool {} is denied by the active permission policy",
                        definition.name
                    ));
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
                if permission == PermissionDecision::Ask {
                    let approval = EnterpriseApprovalRequest {
                        id: tool_request.id,
                        session_id,
                        tool: definition.name.clone(),
                        risk: tool_risk(definition, &call.arguments).into(),
                        reason: format!(
                            "{} mode requires approval for {}",
                            permission_mode_name(self.config.permission_mode),
                            definition.category
                        ),
                        parameters: call.arguments.clone(),
                    };
                    events.push(EnterpriseAgentEvent {
                        kind: "approval_required".into(),
                        message: format!("Approval required for {}", definition.name),
                        data: serde_json::to_value(&approval)?,
                    });
                    let decision = approval_handler.decide(&approval).await;
                    events.push(EnterpriseAgentEvent {
                        kind: "approval_decided".into(),
                        message: format!("Approval {:?} for {}", decision, definition.name),
                        data: json!({"approvalId": approval.id, "decision": decision}),
                    });
                    if decision != EnterpriseApprovalDecision::AllowOnce {
                        let error = EnterpriseAgentError::Tool(format!(
                            "approval denied for {}",
                            definition.name
                        ));
                        self.record_failure(session_id, &mut events, &error).await;
                        return Err(error);
                    }
                }
                if definition.default_permission == PermissionDecision::Ask {
                    self.approvals.approve(tool_request.id)?;
                }
                let result = match self.tools.execute(tool_request).await {
                    Ok(result) => result,
                    Err(error) => {
                        let error = tool_error(error);
                        self.record_failure(session_id, &mut events, &error).await;
                        return Err(error);
                    }
                };
                if result.status != ToolLifecycleStatus::Success {
                    let message = result
                        .error
                        .as_ref()
                        .map(|error| format!("{}: {}", error.kind, error.message))
                        .unwrap_or_else(|| "tool execution did not succeed".into());
                    let error = EnterpriseAgentError::Tool(format!(
                        "{} ended in {}: {message}",
                        result.tool_key,
                        result.status.as_str()
                    ));
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
                let content = serde_json::to_string(&result)?;
                if let Err(error) = self
                    .append_completed_message(&conversation.id, "TOOL", &content)
                    .await
                {
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
                events.push(EnterpriseAgentEvent {
                    kind: "tool_completed".into(),
                    message: format!("Tool {} completed", call.name),
                    data: serde_json::to_value(&result)?,
                });
                request.messages.push(ModelMessage::tool_result(
                    call.id.clone(),
                    call.name.clone(),
                    content,
                ));
                tool_call_count += 1;
            }
            request.id = Uuid::new_v4();
            request.created_at = chrono::Utc::now();
        }

        let response_text = response_text.ok_or_else(|| {
            EnterpriseAgentError::Model("model exceeded the 8-turn tool-call limit".into())
        })?;
        if response_text.trim().is_empty() {
            let error = EnterpriseAgentError::Model("model returned an empty response".into());
            self.record_failure(session_id, &mut events, &error).await;
            return Err(error);
        }
        if let Err(error) = self
            .append_completed_message(&conversation.id, "ASSISTANT", &response_text)
            .await
        {
            self.record_failure(session_id, &mut events, &error).await;
            return Err(error);
        }
        if let Some(checkpoint) = self
            .checkpoints
            .finish_turn(session_id)
            .map_err(checkpoint_error)?
        {
            events.push(EnterpriseAgentEvent {
                kind: "checkpoint_created".into(),
                message: format!("Captured {} Agent file change(s)", checkpoint.files),
                data: json!({
                    "sessionId": session_id,
                    "checkpointId": checkpoint.checkpoint_id,
                    "files": checkpoint.files,
                }),
            });
        }
        events.push(EnterpriseAgentEvent {
            kind: "execution_finished".into(),
            message: response_text.clone(),
            data: json!({"sessionId": session_id, "toolCalls": tool_call_count}),
        });
        self.events.write().await.insert(session_id, events.clone());
        Ok(EnterpriseRun {
            session_id,
            response: response_text,
            events,
        })
    }

    pub async fn events(&self, session_id: Uuid) -> Vec<EnterpriseAgentEvent> {
        self.events
            .read()
            .await
            .get(&session_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn status(&self, session_id: Uuid) -> EnterpriseAgentResult<EnterpriseSessionStatus> {
        let session = self
            .sessions
            .get_session(&session_id.to_string())
            .await
            .map_err(session_error)?;
        session_status(session, &self.config.model.model)
    }

    pub async fn list_sessions(&self) -> EnterpriseAgentResult<Vec<EnterpriseSessionStatus>> {
        self.sessions
            .list_sessions(0, 1_000)
            .await
            .map_err(session_error)?
            .items
            .into_iter()
            .map(|session| session_status(session, &self.config.model.model))
            .collect()
    }

    pub async fn cancel(&self, session_id: Uuid) -> EnterpriseAgentResult<bool> {
        let status = self.status(session_id).await?;
        if status.state != "RUNNING" {
            return Ok(false);
        }
        self.sessions
            .pause_session(&session_id.to_string())
            .await
            .map_err(session_error)?;
        self.events.write().await.insert(
            session_id,
            vec![EnterpriseAgentEvent {
                kind: "cancelled".into(),
                message: "Session paused at a Runtime boundary".into(),
                data: json!({"sessionId": session_id}),
            }],
        );
        Ok(true)
    }

    pub async fn resume(
        &self,
        session_id: Uuid,
    ) -> EnterpriseAgentResult<Vec<EnterpriseAgentEvent>> {
        let status = self.status(session_id).await?;
        if status.state == "PAUSED" {
            self.sessions
                .resume_session(&session_id.to_string())
                .await
                .map_err(session_error)?;
        } else if status.state != "RUNNING" {
            return Err(EnterpriseAgentError::Session(format!(
                "session {session_id} cannot be resumed from {}",
                status.state
            )));
        }
        let events = vec![EnterpriseAgentEvent {
            kind: "execution_finished".into(),
            message: "Session resumed; send the next message to continue".into(),
            data: json!({"sessionId": session_id}),
        }];
        self.events.write().await.insert(session_id, events.clone());
        Ok(events)
    }

    async fn ensure_session(
        &self,
        session_id: Option<Uuid>,
        message: &str,
    ) -> EnterpriseAgentResult<SessionResponse> {
        let session = match session_id {
            Some(id) => self
                .sessions
                .get_session(&id.to_string())
                .await
                .map_err(session_error)?,
            None => self
                .sessions
                .create_session(CreateSessionRequest {
                    title: title(message),
                    description: None,
                    owner: Some("local-user".into()),
                    workspace_id: Some(self.config.workspace.to_string_lossy().into_owned()),
                })
                .await
                .map_err(session_error)?,
        };
        match session.state.as_str() {
            "READY" => self
                .sessions
                .start_session(&session.id)
                .await
                .map_err(session_error),
            "PAUSED" => self
                .sessions
                .resume_session(&session.id)
                .await
                .map_err(session_error),
            "RUNNING" => Ok(session),
            state => Err(EnterpriseAgentError::Session(format!(
                "session {} cannot accept messages in {state}",
                session.id
            ))),
        }
    }

    async fn append_completed_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
    ) -> EnterpriseAgentResult<()> {
        let message = self
            .sessions
            .append_message(AppendMessageRequest {
                conversation_id: conversation_id.into(),
                role: role.into(),
                content: content.into(),
            })
            .await
            .map_err(session_error)?;
        self.sessions
            .update_message_status(&message.id, MessageStatus::Done)
            .await
            .map_err(session_error)?;
        Ok(())
    }

    async fn record_failure(
        &self,
        session_id: Uuid,
        events: &mut Vec<EnterpriseAgentEvent>,
        error: &EnterpriseAgentError,
    ) {
        events.push(EnterpriseAgentEvent {
            kind: "execution_failed".into(),
            message: error.to_string(),
            data: json!({"sessionId": session_id}),
        });
        self.events.write().await.insert(session_id, events.clone());
    }
}

async fn register_workspace_tools(
    manager: &Arc<ToolManager>,
    workspace: &Path,
    checkpoints: Arc<CheckpointStore>,
) -> EnterpriseAgentResult<()> {
    let root = std::fs::canonicalize(workspace).map_err(|error| {
        EnterpriseAgentError::Workspace(format!(
            "cannot open workspace {}: {error}",
            workspace.display()
        ))
    })?;
    let provider =
        ToolProviderDefinition::new("workspace", "Embedded Workspace", ToolProviderKind::Builtin);

    let mut list_definition = ToolDefinition::new(
        "workspace",
        "list_files",
        "1.0.0",
        json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Relative directory, default ."},
                "depth": {"type": "integer", "minimum": 1, "maximum": 8}
            },
            "additionalProperties": false
        }),
    );
    list_definition.description =
        "List files under the opened workspace. Paths must stay inside the workspace.".into();
    list_definition.category = "filesystem.read".into();
    list_definition.default_permission = PermissionDecision::Allow;
    let list_key = list_definition.key.clone();
    let list_root = root.clone();
    let list_tool = Arc::new(FunctionTool::new(list_key, move |request, _| {
        let root = list_root.clone();
        async move { list_workspace_files(&root, &request.parameters) }
    }));

    let mut read_definition = ToolDefinition::new(
        "workspace",
        "read_file",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": {"type": "string", "description": "Relative UTF-8 text file path"}
            },
            "additionalProperties": false
        }),
    );
    read_definition.description =
        "Read a bounded UTF-8 text file inside the opened workspace.".into();
    read_definition.category = "filesystem.read".into();
    read_definition.default_permission = PermissionDecision::Allow;
    let read_key = read_definition.key.clone();
    let read_root = root.clone();
    let read_tool = Arc::new(FunctionTool::new(read_key, move |request, _| {
        let root = read_root.clone();
        async move { read_workspace_file(&root, &request.parameters) }
    }));

    let mut write_definition = ToolDefinition::new(
        "workspace",
        "write_file",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["path", "content"],
            "properties": {
                "path": {"type": "string", "description": "Relative UTF-8 text file path"},
                "content": {"type": "string", "description": "Complete replacement content"},
                "expected_sha256": {"type": "string", "description": "Required current hash when replacing a file"}
            },
            "additionalProperties": false
        }),
    );
    write_definition.description = "Create or replace a bounded UTF-8 file inside the workspace. Replacing an existing file requires the SHA-256 returned by read_file.".into();
    write_definition.category = "filesystem.write".into();
    write_definition.default_permission = PermissionDecision::Ask;
    let write_key = write_definition.key.clone();
    let write_root = root.clone();
    let write_tool = Arc::new(FunctionTool::new(write_key, move |request, _| {
        let root = write_root.clone();
        let checkpoints = checkpoints.clone();
        async move {
            let session_id = request.session_id.ok_or_else(|| {
                ToolError::InvalidArgument("write_file requires an active session".into())
            })?;
            let relative = request
                .parameters
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::InvalidArgument("write_file path is required".into()))?;
            let content = request
                .parameters
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ToolError::InvalidArgument("write_file content is required".into())
                })?;
            let prepared = checkpoints
                .prepare_write(session_id, relative, content)
                .map_err(checkpoint_tool_error)?;
            match write_workspace_file(&root, &request.parameters) {
                Ok(output) => {
                    checkpoints
                        .commit_write(prepared)
                        .map_err(checkpoint_tool_error)?;
                    Ok(output)
                }
                Err(error) => {
                    checkpoints
                        .abort_write(prepared)
                        .map_err(checkpoint_tool_error)?;
                    Err(error)
                }
            }
        }
    }));

    let mut command_definition = ToolDefinition::new(
        "workspace",
        "run_command",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string", "description": "Command executed with the workspace as current directory"},
                "timeout_ms": {"type": "integer", "minimum": 1000, "maximum": 120000}
            },
            "additionalProperties": false
        }),
    );
    command_definition.description = "Run a bounded command in the opened workspace. Secrets are removed from the child environment and destructive system commands are denied.".into();
    command_definition.category = "process.execute".into();
    command_definition.default_permission = PermissionDecision::Ask;
    command_definition.timeout_ms = 120_000;
    let command_key = command_definition.key.clone();
    let command_root = root;
    let command_tool = Arc::new(FunctionTool::new(command_key, move |request, _| {
        let root = command_root.clone();
        async move { run_workspace_command(&root, &request.parameters).await }
    }));

    manager
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![
                ToolRegistration::new(list_definition, list_tool),
                ToolRegistration::new(read_definition, read_tool),
                ToolRegistration::new(write_definition, write_tool),
                ToolRegistration::new(command_definition, command_tool),
            ],
        ))
        .await
        .map_err(tool_error)?;
    Ok(())
}

fn list_workspace_files(root: &Path, parameters: &Value) -> Result<RawToolOutput, ToolError> {
    let relative = parameters
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or(".");
    let max_depth = parameters
        .get("depth")
        .and_then(Value::as_u64)
        .unwrap_or(4)
        .clamp(1, 8) as usize;
    let directory = resolve_workspace_path(root, relative, true)?;
    let mut pending = vec![(directory, 0_usize)];
    let mut files = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        let mut entries = std::fs::read_dir(&directory)
            .map_err(|error| ToolError::execution("list_files", error.to_string(), false))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ToolError::execution("list_files", error.to_string(), false))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if blocked_workspace_name(&name) {
                continue;
            }
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| ToolError::execution("list_files", error.to_string(), false))?;
            if file_type.is_symlink() {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|error| ToolError::execution("list_files", error.to_string(), false))?;
            if metadata.is_dir() {
                if depth + 1 < max_depth {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if metadata.is_file() {
                let relative = path.strip_prefix(root).map_err(|_| {
                    ToolError::PermissionDenied("path escaped the workspace".into())
                })?;
                files.push(relative.to_string_lossy().replace('\\', "/"));
                if files.len() >= 2_000 {
                    files.push("[truncated at 2000 files]".into());
                    return Ok(RawToolOutput::text(files.join("\n")));
                }
            }
        }
    }
    files.sort();
    Ok(RawToolOutput::text(files.join("\n")))
}

fn read_workspace_file(root: &Path, parameters: &Value) -> Result<RawToolOutput, ToolError> {
    let relative = parameters
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgument("read_file path is required".into()))?;
    let path = resolve_workspace_path(root, relative, false)?;
    let metadata = std::fs::metadata(&path)
        .map_err(|error| ToolError::execution("read_file", error.to_string(), false))?;
    if metadata.len() > 256 * 1024 {
        return Err(ToolError::InvalidArgument(
            "read_file supports files up to 256 KiB".into(),
        ));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| ToolError::execution("read_file", error.to_string(), false))?;
    let mut output = RawToolOutput::text(&content);
    output.metadata.insert(
        "sha256".into(),
        format!("{:x}", Sha256::digest(content.as_bytes())),
    );
    Ok(output)
}

fn write_workspace_file(root: &Path, parameters: &Value) -> Result<RawToolOutput, ToolError> {
    let relative = parameters
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgument("write_file path is required".into()))?;
    let content = parameters
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgument("write_file content is required".into()))?;
    if content.len() > 256 * 1024 {
        return Err(ToolError::InvalidArgument(
            "write_file supports content up to 256 KiB".into(),
        ));
    }
    let path = resolve_workspace_write_path(root, relative)?;
    if path.exists() {
        let current = std::fs::read(&path)
            .map_err(|error| ToolError::execution("write_file", error.to_string(), false))?;
        let expected = parameters
            .get("expected_sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ToolError::InvalidArgument(
                    "expected_sha256 is required when replacing an existing file".into(),
                )
            })?;
        let actual = format!("{:x}", Sha256::digest(&current));
        if expected != actual {
            return Err(ToolError::Validation(
                "write_file expected_sha256 does not match current content".into(),
            ));
        }
    }
    std::fs::write(&path, content)
        .map_err(|error| ToolError::execution("write_file", error.to_string(), false))?;
    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let mut output = RawToolOutput::text(format!(
        "wrote {} bytes to {}",
        content.len(),
        path.strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/")
    ));
    output.metadata.insert("sha256".into(), hash);
    Ok(output)
}

fn resolve_workspace_write_path(root: &Path, relative: &str) -> Result<PathBuf, ToolError> {
    let relative_path = Path::new(relative);
    if relative.trim().is_empty()
        || relative.len() > 4_096
        || relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || relative_path
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .any(blocked_workspace_name)
    {
        return Err(ToolError::PermissionDenied(
            "write path is outside the workspace boundary".into(),
        ));
    }
    let candidate = root.join(relative_path);
    let parent = candidate
        .parent()
        .ok_or_else(|| ToolError::InvalidArgument("write path has no parent".into()))?;
    let parent = std::fs::canonicalize(parent)
        .map_err(|error| ToolError::execution("write_file", error.to_string(), false))?;
    if !parent.starts_with(root) {
        return Err(ToolError::PermissionDenied(
            "write path escaped the workspace".into(),
        ));
    }
    let candidate = parent.join(
        candidate
            .file_name()
            .ok_or_else(|| ToolError::InvalidArgument("write path is invalid".into()))?,
    );
    if candidate.exists() {
        let canonical = std::fs::canonicalize(&candidate)
            .map_err(|error| ToolError::execution("write_file", error.to_string(), false))?;
        if !canonical.starts_with(root) || !canonical.is_file() {
            return Err(ToolError::PermissionDenied(
                "write target is not a workspace file".into(),
            ));
        }
        return Ok(canonical);
    }
    Ok(candidate)
}

async fn run_workspace_command(
    root: &Path,
    parameters: &Value,
) -> Result<RawToolOutput, ToolError> {
    let command = parameters
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgument("run_command command is required".into()))?;
    if command.trim().is_empty()
        || command.len() > 16 * 1024
        || command.contains('\0')
        || hard_denied_command(command)
    {
        return Err(ToolError::PermissionDenied(
            "command is empty, oversized or blocked by the hard safety policy".into(),
        ));
    }
    let timeout_ms = parameters
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(120_000)
        .clamp(1_000, 120_000);
    #[cfg(windows)]
    let mut process = {
        let mut process = tokio::process::Command::new("powershell");
        process.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = tokio::process::Command::new("sh");
        process.args(["-lc", command]);
        process
    };
    process
        .current_dir(root)
        .kill_on_drop(true)
        .env_remove("CORE_AGENT_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("DASHSCOPE_API_KEY");
    let output = tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        process.output(),
    )
    .await
    .map_err(|_| ToolError::Timeout {
        tool: "run_command".into(),
        timeout_ms,
    })?
    .map_err(|error| ToolError::execution("run_command", error.to_string(), true))?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.stderr.is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if combined.len() > 1024 * 1024 {
        combined.truncate(1024 * 1024);
        combined.push_str("\n[output truncated at 1 MiB]");
    }
    if !output.status.success() {
        return Err(ToolError::execution(
            "run_command",
            format!("exit {}: {combined}", output.status),
            false,
        ));
    }
    Ok(RawToolOutput::text(combined))
}

fn hard_denied_command(command: &str) -> bool {
    let normalized = command.to_ascii_lowercase();
    [
        "format ",
        "diskpart",
        "shutdown ",
        "restart-computer",
        "stop-computer",
        "reg delete",
        "remove-item env:",
        "git reset --hard",
        "git clean -f",
        "rm -rf /",
        "rd /s /q c:\\",
    ]
    .iter()
    .any(|value| normalized.contains(value))
}

fn resolve_workspace_path(
    root: &Path,
    relative: &str,
    directory: bool,
) -> Result<PathBuf, ToolError> {
    let path = resolve_workspace_resource(root, relative)?;
    if (directory && !path.is_dir()) || (!directory && !path.is_file()) {
        return Err(ToolError::PermissionDenied(
            "path is not a readable workspace resource".into(),
        ));
    }
    Ok(path)
}

pub(crate) fn resolve_workspace_resource(
    root: &Path,
    relative: &str,
) -> Result<PathBuf, ToolError> {
    let relative_path = Path::new(relative);
    if relative.trim().is_empty()
        || relative.len() > 4_096
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
        || relative_path
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .any(blocked_workspace_name)
    {
        return Err(ToolError::PermissionDenied(
            "path is outside the readable workspace boundary".into(),
        ));
    }
    let path = std::fs::canonicalize(root.join(relative_path))
        .map_err(|error| ToolError::execution("workspace_path", error.to_string(), false))?;
    if !path.starts_with(root) {
        return Err(ToolError::PermissionDenied(
            "path is not a readable workspace resource".into(),
        ));
    }
    Ok(path)
}

pub(crate) fn blocked_workspace_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        ".git"
            | ".agent"
            | ".ssh"
            | ".aws"
            | ".azure"
            | ".kube"
            | ".gnupg"
            | "target"
            | "node_modules"
            | ".env"
            | "credentials"
            | "credentials.json"
            | "secrets"
            | "secrets.json"
    ) || normalized.starts_with(".env")
        || normalized.ends_with(".key")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
}

fn model_tool_definitions(
    definitions: &[ToolDefinition],
) -> EnterpriseAgentResult<Vec<ModelToolDefinition>> {
    let mut names = BTreeSet::new();
    let mut result = Vec::new();
    for definition in definitions.iter().filter(|definition| definition.enabled) {
        if !names.insert(definition.name.clone()) {
            return Err(EnterpriseAgentError::Tool(format!(
                "model-visible tool name is ambiguous: {}",
                definition.name
            )));
        }
        result.push(ModelToolDefinition {
            name: definition.name.clone(),
            description: definition.description.clone(),
            parameters: definition.input_schema.clone(),
        });
    }
    Ok(result)
}

fn resolve_tool_definition<'a>(
    definitions: &'a [ToolDefinition],
    requested: &str,
) -> EnterpriseAgentResult<&'a ToolDefinition> {
    if let Some(definition) = definitions
        .iter()
        .find(|definition| definition.enabled && definition.key == requested)
    {
        return Ok(definition);
    }
    let matches = definitions
        .iter()
        .filter(|definition| definition.enabled && definition.name == requested)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [definition] => Ok(definition),
        [] => Err(EnterpriseAgentError::Tool(format!(
            "model requested unknown tool: {requested}"
        ))),
        _ => Err(EnterpriseAgentError::Tool(format!(
            "model requested ambiguous tool: {requested}"
        ))),
    }
}

fn tool_permission_requirement(
    mode: PermissionMode,
    definition: &ToolDefinition,
    parameters: &Value,
) -> PermissionDecision {
    match definition.default_permission {
        PermissionDecision::Allow | PermissionDecision::Deny => definition.default_permission,
        PermissionDecision::Ask => match mode {
            PermissionMode::Strict => PermissionDecision::Ask,
            PermissionMode::Auto => PermissionDecision::Allow,
            PermissionMode::RiskBased
                if definition.category == "process.execute" && safe_command(parameters) =>
            {
                PermissionDecision::Allow
            }
            PermissionMode::RiskBased => PermissionDecision::Ask,
        },
    }
}

fn safe_command(parameters: &Value) -> bool {
    let command = parameters
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let has_shell_control = command.chars().any(|character| {
        matches!(
            character,
            ';' | '|' | '&' | '>' | '<' | '`' | '$' | '(' | ')' | '{' | '}' | '\n' | '\r'
        )
    });
    if command.is_empty()
        || has_shell_control
        || command.contains("..")
        || command.contains(':')
        || command.contains('\\')
    {
        return false;
    }

    matches!(
        command,
        "rg --files"
            | "git status"
            | "git status --short"
            | "git status --porcelain"
            | "git status --porcelain --branch"
            | "git diff --no-ext-diff"
            | "git diff --no-ext-diff --stat"
    )
}

fn tool_risk(definition: &ToolDefinition, parameters: &Value) -> &'static str {
    match definition.category.as_str() {
        "filesystem.read" => "LOW",
        "filesystem.write" => "MEDIUM",
        "process.execute" if safe_command(parameters) => "LOW",
        "process.execute" => "HIGH",
        _ => "HIGH",
    }
}

fn permission_mode_name(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Strict => "strict",
        PermissionMode::RiskBased => "risk-based",
        PermissionMode::Auto => "auto",
    }
}

fn context_model_request(
    context: &Context,
    profile: &str,
    session_id: Uuid,
) -> EnterpriseAgentResult<ModelRequest> {
    let mut messages = Vec::new();
    if let Some(prompt) = context
        .system
        .prompt
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        messages.push(ModelMessage::text(ModelRole::System, prompt));
    }
    messages.extend(context.conversation.messages.iter().filter_map(|message| {
        let role = match message.role.as_str() {
            "SYSTEM" => ModelRole::System,
            "USER" => ModelRole::User,
            "ASSISTANT" | "AGENT" => ModelRole::Assistant,
            "TOOL" => {
                return Some(ModelMessage::text(
                    ModelRole::User,
                    format!("Previous tool result:\n{}", message.content),
                ));
            }
            _ => return None,
        };
        Some(ModelMessage::text(role, message.content.clone()))
    }));
    if let Some(input) = context
        .user
        .current_input
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        messages.push(ModelMessage::text(ModelRole::User, input));
    }
    let mut request = ModelRequest::new(messages).with_profile(profile);
    request
        .metadata
        .insert("session_id".into(), session_id.to_string());
    request
        .metadata
        .insert("context_id".into(), context.id.to_string());
    request.validate().map_err(model_error)?;
    Ok(request)
}

fn session_status(
    session: SessionResponse,
    model: &str,
) -> EnterpriseAgentResult<EnterpriseSessionStatus> {
    Ok(EnterpriseSessionStatus {
        session_id: parse_uuid(&session.id, "session")?,
        state: session.state,
        title: session.title,
        model: model.into(),
        updated_at: session.updated_at,
    })
}

fn database_path(directory: &Path, name: &str) -> EnterpriseAgentResult<String> {
    directory
        .join(name)
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| {
            EnterpriseAgentError::Configuration("database path is not valid UTF-8".into())
        })
}

fn parse_uuid(value: &str, label: &str) -> EnterpriseAgentResult<Uuid> {
    Uuid::parse_str(value).map_err(|_| {
        EnterpriseAgentError::Session(format!("{label} id is not a valid UUID: {value}"))
    })
}

fn validate_message(message: &str) -> EnterpriseAgentResult<()> {
    if message.trim().is_empty()
        || message.len() > 64 * 1024
        || message.chars().any(|character| character == '\0')
    {
        return Err(EnterpriseAgentError::InvalidArgument(
            "message must contain at most 64 KiB of text".into(),
        ));
    }
    Ok(())
}

fn title(message: &str) -> String {
    let title: String = message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(80)
        .collect();
    if title.is_empty() {
        "New Agent Session".into()
    } else {
        title
    }
}

fn session_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Session(error.to_string())
}

fn context_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Context(error.to_string())
}

fn model_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Model(error.to_string())
}

fn tool_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Tool(error.to_string())
}

fn checkpoint_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Runtime(error.to_string())
}

fn checkpoint_tool_error(error: impl std::fmt::Display) -> ToolError {
    ToolError::execution("write_file_checkpoint", error.to_string(), false)
}

fn workspace_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Workspace(error.to_string())
}

fn runtime_error(error: impl std::fmt::Display) -> EnterpriseAgentError {
    EnterpriseAgentError::Runtime(error.to_string())
}

pub type EnterpriseAgentResult<T> = Result<T, EnterpriseAgentError>;

#[derive(Debug, thiserror::Error)]
pub enum EnterpriseAgentError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("session runtime error: {0}")]
    Session(String),
    #[error("context runtime error: {0}")]
    Context(String),
    #[error("model runtime error: {0}")]
    Model(String),
    #[error("tool runtime error: {0}")]
    Tool(String),
    #[error("workspace runtime error: {0}")]
    Workspace(String),
    #[error("Runtime composition error: {0}")]
    Runtime(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governed_tool(category: &str) -> ToolDefinition {
        let mut definition = ToolDefinition::new(
            "workspace",
            "governed",
            "1.0.0",
            json!({"type": "object", "additionalProperties": true}),
        );
        definition.category = category.into();
        definition.default_permission = PermissionDecision::Ask;
        definition
    }

    #[test]
    fn permission_modes_preserve_the_expected_safety_tradeoff() {
        let write = governed_tool("filesystem.write");
        let command = governed_tool("process.execute");
        let safe = json!({"command": "git status --short"});
        let risky = json!({"command": "cargo test"});

        assert_eq!(
            tool_permission_requirement(PermissionMode::Strict, &command, &safe),
            PermissionDecision::Ask
        );
        assert_eq!(
            tool_permission_requirement(PermissionMode::RiskBased, &command, &safe),
            PermissionDecision::Allow
        );
        assert_eq!(
            tool_permission_requirement(PermissionMode::RiskBased, &command, &risky),
            PermissionDecision::Ask
        );
        assert_eq!(
            tool_permission_requirement(
                PermissionMode::RiskBased,
                &write,
                &json!({"path": "src/lib.rs"})
            ),
            PermissionDecision::Ask
        );
        assert_eq!(
            tool_permission_requirement(PermissionMode::Auto, &command, &risky),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn safe_command_classifier_rejects_shell_and_path_escape_variants() {
        assert!(safe_command(&json!({"command": "rg --files"})));
        assert!(safe_command(
            &json!({"command": "git diff --no-ext-diff --stat"})
        ));
        for command in [
            "cargo test",
            "git status; whoami",
            "git status\nwhoami",
            "git status $(whoami)",
            "rg --files ..",
            "rg --files C:\\Users",
        ] {
            assert!(!safe_command(&json!({"command": command})), "{command}");
        }
    }

    #[test]
    fn workspace_write_is_bounded_and_uses_optimistic_concurrency() {
        let workspace = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(workspace.path()).unwrap();
        std::fs::write(root.join("existing.txt"), "before").unwrap();

        let missing_hash =
            write_workspace_file(&root, &json!({"path": "existing.txt", "content": "after"}))
                .unwrap_err();
        assert!(matches!(missing_hash, ToolError::InvalidArgument(_)));

        let wrong_hash = write_workspace_file(
            &root,
            &json!({"path": "existing.txt", "content": "after", "expected_sha256": "wrong"}),
        )
        .unwrap_err();
        assert!(matches!(wrong_hash, ToolError::Validation(_)));

        let expected = format!("{:x}", Sha256::digest(b"before"));
        write_workspace_file(
            &root,
            &json!({"path": "existing.txt", "content": "after", "expected_sha256": expected}),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("existing.txt")).unwrap(),
            "after"
        );

        let escaped = write_workspace_file(
            &root,
            &json!({"path": "../escaped.txt", "content": "blocked"}),
        )
        .unwrap_err();
        assert!(matches!(escaped, ToolError::PermissionDenied(_)));
    }

    #[test]
    fn destructive_commands_remain_hard_denied() {
        assert!(hard_denied_command("git reset --hard"));
        assert!(hard_denied_command("Remove-Item Env:CORE_AGENT_API_KEY"));
        assert!(!hard_denied_command("git status --short"));
    }

    #[test]
    fn model_configuration_debug_never_exposes_the_api_key() {
        let config = EnterpriseModelConfig {
            api_key: Some("secret-value-that-must-not-appear".into()),
            ..EnterpriseModelConfig::default()
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("api_key_configured: true"));
        assert!(!debug.contains("secret-value-that-must-not-appear"));
    }

    #[test]
    fn workspace_listing_never_follows_directory_symlinks() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(workspace.path()).unwrap();
        std::fs::write(outside.path().join("outside-secret.txt"), "hidden").unwrap();
        let link = root.join("external-link");
        #[cfg(windows)]
        let created = std::os::windows::fs::symlink_dir(outside.path(), &link);
        #[cfg(unix)]
        let created = std::os::unix::fs::symlink(outside.path(), &link);
        if created.is_err() {
            return;
        }

        let output = list_workspace_files(&root, &json!({})).unwrap();
        let core_agent_tool::ToolContent::Text(files) = &output.content[0] else {
            panic!("list_files must return text")
        };
        assert!(!files.contains("external-link"));
        assert!(!files.contains("outside-secret.txt"));
    }
}
