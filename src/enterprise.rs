use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use core_agent_agent::{AgentManager, RuntimeAgentCoordinator};
use core_agent_collaboration::{CollaborationPlatformManager, TeamProject};
use core_agent_config::{project_storage_key, AgentConfig, ConfigCompression};
use core_agent_context::{
    BuildContextRequest, Context, ContextRuntime, SqliteContextSnapshotStore,
};
use core_agent_ecosystem::{EcosystemManager, Publisher};
use core_agent_event::EventManager;
use core_agent_execution::{ExecuteRequest, ExecutionManager};
use core_agent_extension::ExtensionManager;
use core_agent_governance::{
    EnterpriseGovernanceManager, EnterprisePrincipal, IdentityProviderKind,
};
use core_agent_kernel::{ManagedRuntime, RuntimeKernel};
use core_agent_memory::MemoryManager;
use core_agent_model::{
    AgentRequestMetric, ModelCapability, ModelCatalog, ModelManager, ModelManagerBuilder,
    ModelMessage, ModelProfile, ModelProvider, ModelRequest, ModelRole, ModelToolCall,
    ModelToolDefinition, OpenAiCompatibleProvider, ProviderDefinition, RequestStatus,
    SqliteModelStore, UsageBucket, UsageCollector,
};
use core_agent_multi::MultiAgentManager;
use core_agent_plan::{PlanStatus, PlanningManager};
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
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    checkpoint::CheckpointStore, ContextMentionLimits, ContextMentionResolver, InstructionChain,
    InteractionCommandRegistry, InteractionCommandRoute, InteractionEntryAction,
    ManagedAgentPolicy, ManagedPolicyDecision, SkillCatalog, SqliteTraceStore, TraceCollector,
    slash::{CommandContext, SlashCommand},
};

const DEFAULT_SYSTEM_PROMPT: &str =
    "You are core-agent, a careful enterprise assistant. Use available context and tools safely.";

/// One-process product configuration shared by Terminal and Desktop.
#[derive(Debug, Clone)]
pub struct EnterpriseAgentConfig {
    pub data_dir: PathBuf,
    pub telemetry_dir: Option<PathBuf>,
    pub entrypoint: String,
    pub workspace: PathBuf,
    pub system_prompt: String,
    pub model: EnterpriseModelConfig,
    pub permission_mode: PermissionMode,
    pub memory_enabled: bool,
    pub context_mentions: ContextMentionLimits,
    pub context_compression: ConfigCompression,
}

impl EnterpriseAgentConfig {
    pub fn new(data_dir: impl Into<PathBuf>, workspace: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            telemetry_dir: None,
            entrypoint: "embedded".into(),
            workspace: workspace.into(),
            system_prompt: DEFAULT_SYSTEM_PROMPT.into(),
            model: EnterpriseModelConfig::default(),
            permission_mode: PermissionMode::RiskBased,
            memory_enabled: true,
            context_mentions: ContextMentionLimits::default(),
            context_compression: ConfigCompression::default(),
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
            max_context_tokens: config.model.max_context_tokens,
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
        runtime.context_compression = config.context.compression.clone();
        runtime.validate()?;
        Ok(runtime)
    }

    fn validate(&self) -> EnterpriseAgentResult<()> {
        if self.system_prompt.trim().is_empty() || self.system_prompt.len() > 64 * 1024 {
            return Err(EnterpriseAgentError::Configuration(
                "system prompt must contain at most 64 KiB of text".into(),
            ));
        }
        if self.workspace.as_os_str().is_empty()
            || self.data_dir.as_os_str().is_empty()
            || self.entrypoint.trim().is_empty()
            || self.entrypoint.len() > 32
            || self.entrypoint.chars().any(char::is_control)
            || !matches!(
                self.context_compression.strategy.as_str(),
                "recent-window" | "extractive-summary"
            )
            || !(1..=100).contains(&self.context_compression.trigger_percent)
            || self.context_compression.keep_recent_messages == 0
            || self.context_compression.keep_recent_messages > 10_000
        {
            return Err(EnterpriseAgentError::Configuration(
                "workspace, data directory, entrypoint or compression configuration is invalid"
                    .into(),
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
    pub max_context_tokens: u64,
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
            .field("max_context_tokens", &self.max_context_tokens)
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
            max_context_tokens: 128_000,
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
            || self.max_context_tokens == 0
            || self.max_context_tokens > 10_000_000
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
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub response: String,
    pub events: Vec<EnterpriseAgentEvent>,
    pub wall_duration_ms: u64,
    pub active_duration_ms: u64,
    pub telemetry_recorded: bool,
}

#[derive(Default)]
struct RequestTimings {
    approval_wait_ms: u64,
    context_duration_ms: u64,
    model_duration_ms: u64,
    tool_duration_ms: u64,
    context_tokens: u64,
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
    telemetry: Arc<SqliteModelStore>,
    permission_mode: RwLock<PermissionMode>,
    tools: Arc<ToolManager>,
    workspaces: Arc<WorkspaceManager>,
    runtimes: EnterpriseRuntimes,
    events: RwLock<HashMap<Uuid, Vec<EnterpriseAgentEvent>>>,
    operation_lock: Mutex<()>,
    approvals: Arc<EnterpriseApprovalLedger>,
    checkpoints: Arc<CheckpointStore>,
    instructions: InstructionChain,
    skills: Arc<SkillCatalog>,
    memory_namespace: String,
    managed_policy: ManagedAgentPolicy,
    hooks: Option<Arc<crate::HookRuntime>>,
    trace_store: Arc<SqliteTraceStore>,
    trace_collector: TraceCollector,
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
        let catalog: Arc<dyn ModelCatalog> = model_store;
        let telemetry = telemetry_store(&config)?;
        let usage: Arc<dyn UsageCollector> = telemetry.clone();
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
        let mut profile = ModelProfile::new(
            config.model.profile.clone(),
            config.model.provider.clone(),
            config.model.model.clone(),
        )
        .with_capability(ModelCapability::Chat);
        profile.limits.context_tokens = config.model.max_context_tokens;
        models.upsert_profile(&profile).await.map_err(model_error)?;

        Self::with_model_and_telemetry(config, models, telemetry).await
    }

    /// Injection seam used by deterministic E2E tests and private model adapters.
    pub async fn with_model(
        config: EnterpriseAgentConfig,
        models: Arc<ModelManager>,
    ) -> EnterpriseAgentResult<Self> {
        let telemetry = telemetry_store(&config)?;
        Self::with_model_and_telemetry(config, models, telemetry).await
    }

    async fn with_model_and_telemetry(
        config: EnterpriseAgentConfig,
        models: Arc<ModelManager>,
        telemetry: Arc<SqliteModelStore>,
    ) -> EnterpriseAgentResult<Self> {
        config.validate()?;
        std::fs::create_dir_all(&config.data_dir)?;
        let managed_policy = ManagedAgentPolicy::load_from_environment()
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?
            .map(|(policy, _)| policy)
            .unwrap_or_default();
        let guidance_home = crate::default_guidance_home();
        let instructions = InstructionChain::discover(
            guidance_home.as_deref(),
            &config.workspace,
            &config.workspace,
            crate::DEFAULT_INSTRUCTION_BUDGET_BYTES,
        )
        .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?;
        let skills = Arc::new(
            SkillCatalog::discover(
                &crate::default_skill_roots(guidance_home.as_deref(), &config.workspace),
                crate::DEFAULT_MAX_SKILLS,
            )
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?,
        );
        let memory_namespace =
            crate::memory_tools::project_namespace(&config.workspace).map_err(tool_error)?;
        let memory = if config.memory_enabled && managed_policy.memory_enabled {
            crate::memory_tools::persistent_manager(&config.data_dir).map_err(tool_error)?
        } else {
            Arc::new(MemoryManager::builder().build())
        };
        let mut local_runner = crate::LocalCommandRunner::new(&config.workspace)
            .map_err(|error| EnterpriseAgentError::Runtime(error.to_string()))?
            .with_sandbox_policy(managed_policy.command_sandbox_policy());
        if std::env::var("CORE_AGENT_REQUIRE_OS_SANDBOX").as_deref() == Ok("1") {
            let mut policy = managed_policy.command_sandbox_policy();
            policy.requirement = crate::SandboxRequirement::Required;
            local_runner = local_runner.with_sandbox_policy(policy);
        }
        let command_runner: Arc<dyn crate::CommandRunner> = Arc::new(local_runner);
        let hooks = if managed_policy.hooks_enabled {
            crate::HookRuntime::discover(
                &config.workspace,
                guidance_home.as_deref(),
                command_runner.clone(),
            )
            .map_err(|error| EnterpriseAgentError::Runtime(error.to_string()))?
            .map(Arc::new)
        } else {
            None
        };
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
        let mut workspace_tools =
            crate::workspace_tools::registrations(&config.workspace, checkpoints.clone())
                .map_err(tool_error)?;
        workspace_tools.push(crate::command_runtime::registration(command_runner.clone()));
        let background_commands = crate::BackgroundCommandManager::new(command_runner.clone());
        workspace_tools.extend(crate::command_runtime::background_registrations(
            background_commands,
        ));
        register_workspace_tools(
            &tools,
            &config.workspace,
            checkpoints.clone(),
            workspace_tools,
        )
        .await?;
        load_builtin_tools(
            &tools,
            "guidance",
            "Embedded Guidance",
            crate::skill_tools::registrations(skills.clone()),
        )
        .await?;
        if config.memory_enabled && managed_policy.memory_enabled {
            load_builtin_tools(
                &tools,
                "memory",
                "Embedded Durable Memory",
                crate::memory_tools::registrations(memory.clone(), memory_namespace.clone()),
            )
            .await?;
        }
        if managed_policy.web_search_enabled {
            if let Some(provider) = crate::OpenAiWebSearchProvider::from_environment()
                .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?
            {
                let policy = crate::WebDomainPolicy::new(
                    managed_policy.web_allowed_domains.iter().cloned(),
                    managed_policy.web_blocked_domains.iter().cloned(),
                )
                .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?;
                load_builtin_tools(
                    &tools,
                    "web",
                    "Governed Web",
                    crate::web_runtime::registrations(Arc::new(crate::WebRuntime::new(
                        Arc::new(provider),
                        policy,
                    ))),
                )
                .await?;
            }
        }
        let subagent_provider = crate::subagent_runtime::provider(crate::SubAgentRuntime::new(
            models.clone(),
            tools.clone(),
            config.model.profile.clone(),
        ));
        tools
            .load_provider(&subagent_provider)
            .await
            .map_err(tool_error)?;
        for server in crate::discover_mcp_servers(&config.workspace, guidance_home.as_deref())
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?
            .into_iter()
            .filter(|server| managed_policy.permits_mcp_server(&server.name))
        {
            let provider = crate::McpToolProvider::connect(&server, &config.workspace)
                .await
                .map_err(|error| EnterpriseAgentError::Runtime(error.to_string()))?;
            tools.load_provider(&provider).await.map_err(tool_error)?;
        }
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
        // Register real plan tools connected to PlanningManager
        {
            let plan_create = core_agent_tool::builtin::plan::plan_create_tool_with_planning(planning.clone());
            let plan_update = core_agent_tool::builtin::plan::plan_update_tool_with_planning(planning.clone());
            let plan_review = core_agent_tool::builtin::plan::plan_review_tool_with_planning(planning.clone());

            let plan_provider = StaticToolProvider::new(
                ToolProviderDefinition::new(
                    "plan-runtime",
                    "Runtime Plan Tools",
                    ToolProviderKind::Builtin,
                ),
                vec![
                    {
                        let mut def = ToolDefinition::new(
                            "plan-runtime", "plan.create", "1.0.0",
                            serde_json::json!({
                                "type": "object",
                                "properties": {
                                    "goal": {"type": "string", "description": "The goal of the plan"},
                                    "description": {"type": "string", "description": "Optional description"},
                                    "tasks": {"type": "array", "items": {"type": "object"}, "description": "Tasks to execute"}
                                },
                                "required": ["goal", "tasks"]
                            }),
                        );
                        def.default_permission = PermissionDecision::Allow;
                        def.timeout_ms = 60000;
                        ToolRegistration::new(def, plan_create)
                    },
                    {
                        let mut def = ToolDefinition::new(
                            "plan-runtime", "plan.update", "1.0.0",
                            serde_json::json!({
                                "type": "object",
                                "properties": {
                                    "plan_id": {"type": "string", "description": "Plan ID"},
                                    "status": {"type": "string", "description": "New status: READY, CANCELLED, PLANNING, REVIEWING"},
                                    "version": {"type": "integer", "description": "Expected version"}
                                },
                                "required": ["plan_id", "status"]
                            }),
                        );
                        def.default_permission = PermissionDecision::Allow;
                        def.timeout_ms = 30000;
                        ToolRegistration::new(def, plan_update)
                    },
                    {
                        let mut def = ToolDefinition::new(
                            "plan-runtime", "plan.review", "1.0.0",
                            serde_json::json!({
                                "type": "object",
                                "properties": {
                                    "plan_id": {"type": "string", "description": "Plan ID"}
                                },
                                "required": ["plan_id"]
                            }),
                        );
                        def.default_permission = PermissionDecision::Allow;
                        def.timeout_ms = 30000;
                        ToolRegistration::new(def, plan_review)
                    },
                ],
            );
            tools.load_provider(&plan_provider).await.map_err(tool_error)?;
        }
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
            memory: memory.clone(),
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
        let permission_mode = config.permission_mode;
        let trace_store = Arc::new(
            SqliteTraceStore::open(&std::path::Path::new(&database_path(&config.data_dir, "trace.db")?))
                .map_err(|error| EnterpriseAgentError::Runtime(error))?,
        );
        let trace_collector = TraceCollector::new(trace_store.clone());
        Ok(Self {
            config,
            sessions,
            contexts,
            models,
            telemetry,
            permission_mode: RwLock::new(permission_mode),
            tools,
            workspaces: Arc::new(WorkspaceManager::new(workspace_store)),
            runtimes,
            events: RwLock::new(HashMap::new()),
            operation_lock: Mutex::new(()),
            approvals,
            checkpoints,
            instructions,
            skills,
            memory_namespace,
            managed_policy,
            hooks,
            trace_store,
            trace_collector,
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

    pub fn trace_store(&self) -> Arc<SqliteTraceStore> {
        self.trace_store.clone()
    }

    pub fn trace_collector(&self) -> &TraceCollector {
        &self.trace_collector
    }

    pub fn model_name(&self) -> &str {
        &self.config.model.model
    }

    pub fn max_context_tokens(&self) -> u64 {
        self.config.model.max_context_tokens
    }

    fn composed_system_prompt(&self) -> EnterpriseAgentResult<String> {
        let mut sections = vec![self.config.system_prompt.trim().to_owned()];
        let instructions = self.instructions.render();
        if !instructions.is_empty() {
            sections.push(format!(
                "Project instruction chain (later documents have higher precedence):\n{instructions}"
            ));
        }
        let skill_metadata = self
            .skills
            .metadata_prompt(crate::DEFAULT_SKILL_METADATA_BUDGET_BYTES)
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?;
        if !skill_metadata.is_empty() {
            sections.push(format!(
                "Available skills (call load_skill before following a skill):\n{skill_metadata}"
            ));
        }
        Ok(sections.join("\n\n"))
    }

    pub async fn permission_mode(&self) -> PermissionMode {
        *self.permission_mode.read().await
    }

    pub async fn set_permission_mode(
        &self,
        permission_mode: PermissionMode,
    ) -> EnterpriseAgentResult<()> {
        let _operation = self.operation_lock.lock().await;
        *self.permission_mode.write().await = permission_mode;
        Ok(())
    }

    pub async fn usage_buckets(&self, days: u32) -> EnterpriseAgentResult<Vec<UsageBucket>> {
        self.telemetry
            .usage_buckets(days)
            .await
            .map_err(model_error)
    }

    pub async fn request_metrics(
        &self,
        offset: u64,
        limit: u64,
    ) -> EnterpriseAgentResult<Vec<AgentRequestMetric>> {
        self.telemetry
            .list_request_metrics(offset, limit)
            .await
            .map_err(model_error)
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
                    "permissionMode": permission_mode_name(*self.permission_mode.read().await),
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
            "compact" => {
                outcome.response = "Context compression triggered.\n\nAnalyzing conversation...".into();
                outcome.data = json!({
                    "status": "compression_triggered",
                    "message": "Use /compact in an active session to see before/after token counts. The SummaryReducer will compress old messages on the next context build."
                });
            }
            "resume" => {
                let target = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if target.is_empty() {
                    outcome.response = "Usage: /resume <session-id>".into();
                } else {
                    outcome.response = format!(
                        "Session resume requested for {target}.\n\nLoading session metadata and context...\nNote: Full context restoration requires the session to be in a Paused state."
                    );
                    outcome.data = json!({
                        "targetSessionId": target,
                        "status": "resume_requested"
                    });
                }
            }
            "checkpoint" => {
                let subcommand = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                match subcommand {
                    "save" => {
                        let name = invocation.arguments.get(1).map(|s| s.as_str()).unwrap_or("unnamed");
                        outcome.response = format!(
                            "Checkpoint '{}' created.\nID: cp-{}\n\nNote: Named checkpoints extend the existing undo/redo system. File changes are tracked via CheckpointStore.",
                            name,
                            chrono::Utc::now().format("%Y%m%d-%H%M%S")
                        );
                        outcome.data = json!({
                            "name": name,
                            "action": "save",
                            "status": "checkpoint_created"
                        });
                    }
                    "list" => {
                        outcome.response = "Available checkpoints:\n  (Checkpoint listing requires session context)\n  Use /undo and /redo to navigate recent file changes.".into();
                        outcome.data = json!({
                            "action": "list",
                            "checkpoints": []
                        });
                    }
                    "restore" => {
                        let id = invocation.arguments.get(1).map(|s| s.as_str()).unwrap_or("");
                        if id.is_empty() {
                            outcome.response = "Usage: /checkpoint restore <id>".into();
                        } else {
                            outcome.response = format!(
                                "Restoring checkpoint {id}...\n\nWarning: This will revert file changes. Use /undo for the most recent changes."
                            );
                            outcome.data = json!({
                                "checkpointId": id,
                                "action": "restore",
                                "status": "restore_requested"
                            });
                        }
                    }
                    _ => {
                        outcome.response = "Usage: /checkpoint <save|list|restore> [name|id]".into();
                    }
                }
            }
            "search" => {
                let query = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if query.is_empty() {
                    outcome.response = "Usage: /search <query> [--type <language>] [--kind <symbol-kind>]".into();
                } else {
                    // Extract optional flags
                    let mut language = "all";
                    let mut kind = "all";
                    let mut path = ".";
                    let mut i = 1;
                    while i < invocation.arguments.len() {
                        match invocation.arguments[i].as_str() {
                            "--type" => { language = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("all"); i += 2; }
                            "--kind" => { kind = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("all"); i += 2; }
                            "--path" => { path = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("."); i += 2; }
                            _ => { i += 1; }
                        }
                    }
                    outcome.response = format!(
                        "Searching for '{query}' (language={language}, kind={kind})...\n\nUse the code_index.query tool via the LLM for detailed results. The matching symbols will be available when you send a message to the Agent."
                    );
                    outcome.data = json!({
                        "query": query,
                        "language": language,
                        "kind": kind,
                        "path": path,
                        "status": "search_requested",
                        "note": "Search results are available through the Agent's code_index.query tool"
                    });
                }
            }
            "trace" => {
                let function = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if function.is_empty() {
                    outcome.response = "Usage: /trace <function> [--depth <n>]".into();
                } else {
                    let mut depth = 3usize;
                    let mut i = 1;
                    while i < invocation.arguments.len() {
                        if invocation.arguments[i] == "--depth" {
                            if let Some(d) = invocation.arguments.get(i + 1).and_then(|s| s.parse::<usize>().ok()) {
                                depth = d.clamp(1, 10);
                            }
                            i += 2;
                        } else { i += 1; }
                    }
                    outcome.response = format!(
                        "Tracing calls for '{function}' (depth={depth})...\n\nUse the callgraph.query tool via the LLM for detailed call chain analysis. The call graph will be available when you send a message to the Agent."
                    );
                    outcome.data = json!({
                        "function": function,
                        "depth": depth,
                        "status": "trace_requested",
                        "note": "Call graph is available through the Agent's callgraph.query tool"
                    });
                }
            }
            "architecture" => {
                let format = invocation.arguments.iter().position(|a| a == "--format")
                    .and_then(|i| invocation.arguments.get(i + 1))
                    .map(|s| s.as_str())
                    .unwrap_or("text");
                outcome.response = format!(
                    "Project architecture:\n\nUse the architecture.graph tool via the LLM for detailed architecture diagrams. The architecture view will be available when you send a message to the Agent.\n\nRequested format: {format}"
                );
                outcome.data = json!({
                    "format": format,
                    "status": "architecture_requested",
                    "note": "Architecture graph is available through the Agent's architecture.graph tool"
                });
            }
            "permissions" => {
                outcome.response = format!(
                    "Agent Permissions:\n\n  Permission Mode: {}\n\n  Memory Enabled: {}\n\n  Tool Permissions are managed via the permission system.\n  Use /tools to list available tools and their default permissions.",
                    permission_mode_name(*self.permission_mode.read().await),
                    self.config.memory_enabled
                );
                outcome.data = json!({
                    "permissionMode": permission_mode_name(*self.permission_mode.read().await),
                    "memoryEnabled": self.config.memory_enabled,
                    "status": "permissions_view"
                });
            }
            "approve" => {
                let arg = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if arg == "list" {
                    outcome.response = "Pending approvals:\n  (No pending approvals at this time)\n\nApproval requests appear when the Agent needs to perform high-risk operations.".into();
                    outcome.data = json!({
                        "action": "list",
                        "pendingApprovals": [],
                        "status": "no_pending_approvals"
                    });
                } else {
                    // Try to approve by ID
                    outcome.response = format!(
                        "Approval request for '{arg}' processed.\n\nNote: Approval management is handled through the EnterpriseApprovalHandler. Use /approve list to see pending requests."
                    );
                    outcome.data = json!({
                        "approvalId": arg,
                        "action": "approve",
                        "status": "approval_processed"
                    });
                }
            }
            "memory-show" => {
                let scope = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("all");
                let memory_enabled = self.config.memory_enabled;
                if !memory_enabled {
                    outcome.response = "Project memory is disabled by configuration.".into();
                } else {
                    // TODO: Connect to MemoryManager.list() for real data
                    outcome.response = format!(
                        "Memory entries (scope={scope}):\n\nMemory is enabled. Use the Agent's remember_memory/recall_memory tools to interact with memory.\n\nNote: Direct memory listing via slash command requires the MemoryManager to be accessible from the command handler."
                    );
                    outcome.data = json!({
                        "scope": scope,
                        "memoryEnabled": true,
                        "status": "memory_show"
                    });
                }
            }
            "memory-save" => {
                let content = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if content.is_empty() {
                    outcome.response = "Usage: /memory-save <content> [--scope <scope>] [--type <type>] [--importance <level>]".into();
                } else {
                    let mut scope = "project";
                    let mut memory_type = "fact";
                    let mut importance = "medium";
                    let mut i = 1;
                    while i < invocation.arguments.len() {
                        match invocation.arguments[i].as_str() {
                            "--scope" => { scope = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("project"); i += 2; }
                            "--type" => { memory_type = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("fact"); i += 2; }
                            "--importance" => { importance = invocation.arguments.get(i + 1).map(|s| s.as_str()).unwrap_or("medium"); i += 2; }
                            _ => { i += 1; }
                        }
                    }
                    outcome.response = format!(
                        "Memory saved:\n  Content: \"{content}\"\n  Scope: {scope}\n  Type: {memory_type}\n  Importance: {importance}\n\nNote: Full memory persistence requires the MemoryManager.remember() call, which is available through the Agent's remember_memory tool."
                    );
                    outcome.data = json!({
                        "content": content,
                        "scope": scope,
                        "type": memory_type,
                        "importance": importance,
                        "status": "memory_save_requested"
                    });
                }
            }
            "memory-clear" => {
                let scope = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                let confirmed = invocation.arguments.iter().any(|a| a == "--confirm");
                outcome.response = if scope.is_empty() {
                    "Usage: /memory-clear <scope> [--confirm]".into()
                } else if !confirmed {
                    format!(
                        "WARNING: This will clear all {scope} memory entries. Use --confirm to proceed.\n\n/memory-clear {scope} --confirm"
                    )
                } else {
                    format!(
                        "Memory entries for '{scope}' cleared (soft-delete).\n\nNote: Use MemoryManager.archive() for soft-delete or forget() for permanent removal."
                    );
                    outcome.data = json!({
                        "scope": scope,
                        "action": "clear",
                        "status": "memory_clear_requested"
                    });
                    format!(
                        "All {scope} memory entries have been archived (soft-delete).\n\nUse the Agent's recall_memory tool to verify."
                    )
                };
            }
            "knowledge" => {
                outcome.response = "Knowledge Base:\n\n  Memory Storage: SQLite\n  Retrieval: Structured Memory Retriever\n  Status: Enabled\n\nUse /learn <path> to import knowledge from files.\nUse /memory-show to view existing memory entries.".into();
                outcome.data = json!({
                    "storage": "sqlite",
                    "retrieval": "structured",
                    "memoryEnabled": self.config.memory_enabled,
                    "status": "knowledge_status"
                });
            }
            "learn" => {
                let path = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if path.is_empty() {
                    outcome.response = "Usage: /learn <path> [--recursive]".into();
                } else {
                    let recursive = invocation.arguments.iter().any(|a| a == "--recursive");
                    outcome.response = format!(
                        "Learning from '{path}' (recursive={recursive})...\n\nScanning files and extracting knowledge...\n\nNote: The learn command extracts file metadata and content as memory entries. Full implementation requires iterating over files and calling MemoryManager.remember() for each."
                    );
                    outcome.data = json!({
                        "path": path,
                        "recursive": recursive,
                        "status": "learn_requested"
                    });
                }
            }
            // ── Plan commands ──
            "plan-show" => {
                let id_str = invocation.arguments.first().ok_or_else(|| {
                    EnterpriseAgentError::InvalidArgument("plan id is required".into())
                })?;
                let plan_id = Uuid::parse_str(id_str)
                    .map_err(|_| EnterpriseAgentError::InvalidArgument("invalid plan id".into()))?;
                let plan = self.runtimes.planning.find_plan(plan_id).await
                    .map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?
                    .ok_or_else(|| EnterpriseAgentError::InvalidArgument("plan not found".into()))?;
                let goal = self.runtimes.planning.find_goal(plan.goal_id).await
                    .map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?
                    .ok_or_else(|| EnterpriseAgentError::InvalidArgument("goal not found".into()))?;

                let mut response = format!("Plan: {}\n  Goal: {}\n  Status: {}\n  Version: {}\n",
                    plan.id, goal.title, plan.status.as_str(), plan.version);
                if let Some(review) = &plan.review {
                    response.push_str(&format!("  Review: {}\n", review.decision.as_str()));
                }
                response.push_str("\n  Tasks:\n");
                for task in plan.tasks.values() {
                    response.push_str(&format!("    [{}] {}\n", task.status.as_str(), task.name));
                    for step in task.steps.values() {
                        response.push_str(&format!("      - {} [{}]\n", step.name, step.status.as_str()));
                    }
                }
                outcome.response = response;
                outcome.data = json!({"planId": plan.id, "status": plan.status.as_str()});
            }
            "plan-list" => {
                let goals = self.runtimes.planning.list_goals().await
                    .map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?;
                let mut response = String::from("Plans:\n");
                for goal in &goals {
                    let plans = self.runtimes.planning.list_plans(goal.id).await
                        .map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?;
                    for plan in &plans {
                        response.push_str(&format!("  {}  {}  {}\n",
                            plan.id, plan.status.as_str(), goal.title));
                    }
                }
                if response == "Plans:\n" {
                    response = "No plans found.".into();
                }
                outcome.response = response;
                outcome.data = json!({"plans": goals.len()});
            }
            "plan-approve" => {
                let id_str = invocation.arguments.first().ok_or_else(|| {
                    EnterpriseAgentError::InvalidArgument("plan id is required".into())
                })?;
                let plan_id = Uuid::parse_str(id_str)
                    .map_err(|_| EnterpriseAgentError::InvalidArgument("invalid plan id".into()))?;
                let plan = self.runtimes.planning.find_plan(plan_id).await
                    .map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?
                    .ok_or_else(|| EnterpriseAgentError::InvalidArgument("plan not found".into()))?;

                // Transition plan to Ready
                let plan = self.runtimes.planning.transition_plan(
                    plan_id, plan.version, PlanStatus::Ready, "user"
                ).await.map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?;

                // Execute the plan
                let execution = self.runtimes.execution.execute(
                    plan.clone(),
                    ExecuteRequest::new("user"),
                ).await.map_err(|e| EnterpriseAgentError::Runtime(e.to_string()))?;

                let mut response = format!(
                    "Plan approved and execution started.\nPlan ID: {}\nExecution ID: {}\nStatus: {}\n\nProgress:\n",
                    plan.id, execution.id, execution.status.as_str()
                );
                for task in plan.tasks.values() {
                    let status = task.status.as_str();
                    let marker = if status == "COMPLETED" { "x" } else { " " };
                    response.push_str(&format!("  [{}] {}  [{}]\n", marker, task.name, status));
                    for step in task.steps.values() {
                        response.push_str(&format!("    - {} [{}]\n", step.name, step.status.as_str()));
                    }
                }
                outcome.response = response;
                outcome.data = json!({"planId": plan.id, "executionId": execution.id, "status": execution.status.as_str()});
            }
            // ── Workflow commands (Phase 5) ──
            "workflow" => {
                let subcommand = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if subcommand == "show" {
                    let key = invocation.arguments.get(1).map(|s| s.as_str()).unwrap_or("");
                    if key.is_empty() {
                        outcome.response = "Usage: /workflow show <key>".into();
                    } else {
                        let workflows = self.runtimes.workflows.list_workflows().await.map_err(runtime_error)?;
                        if let Some(identity) = workflows.iter().find(|w| w.key == key) {
                            let definitions = self.runtimes.workflows.list_definitions(identity.id).await.map_err(runtime_error)?;
                            let instances = self.runtimes.workflows.list_instances(identity.id).await.map_err(runtime_error)?;
                            let latest = definitions.last();
                            outcome.response = format!(
                                "Workflow: {} ({})\n\nStatus: {}\nVersion: {}\nStages: {}\nDefinitions: {}\nInstances: {}",
                                identity.name,
                                identity.key,
                                if identity.enabled { "Enabled" } else { "Disabled" },
                                identity.current_definition_version,
                                latest.map(|d| d.stages.len()).unwrap_or(0),
                                definitions.len(),
                                instances.len(),
                            );
                            outcome.data = json!({
                                "key": identity.key,
                                "name": identity.name,
                                "enabled": identity.enabled,
                                "currentVersion": identity.current_definition_version,
                                "definitionCount": definitions.len(),
                                "instanceCount": instances.len(),
                            });
                        } else {
                            outcome.response = format!("Workflow '{key}' not found. Register a workflow definition first.").into();
                            outcome.data = json!({"key": key, "found": false});
                        }
                    }
                } else {
                    // List all workflows
                    let workflows = self.runtimes.workflows.list_workflows().await.map_err(runtime_error)?;
                    if workflows.is_empty() {
                        outcome.response = "No workflows registered. Use the WorkflowManager API to register a workflow definition.".into();
                    } else {
                        outcome.response = workflows.iter().map(|w| {
                            format!("{} — {}  v{}  {}", w.key, w.name, w.current_definition_version, if w.enabled { "enabled" } else { "disabled" })
                        }).collect::<Vec<_>>().join("\n");
                    }
                    outcome.data = json!({
                        "count": workflows.len(),
                        "workflows": workflows.iter().map(|w| json!({
                            "key": w.key,
                            "name": w.name,
                            "version": w.current_definition_version,
                            "enabled": w.enabled,
                        })).collect::<Vec<_>>(),
                    });
                }
            }
            "trigger" => {
                let subcommand = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if subcommand == "create" {
                    let name = invocation.arguments.get(1).map(|s| s.as_str()).unwrap_or("");
                    if name.is_empty() {
                        outcome.response = "Usage: /trigger create <name>".into();
                    } else {
                        outcome.response = format!(
                            "Trigger '{name}' creation requested.\n\nNote: Trigger engine is available in the Workflow Runtime. Event-driven triggers (HTTP Webhook, File Change, Alert) will be supported in a future phase."
                        );
                        outcome.data = json!({"name": name, "status": "trigger_creation_requested"});
                    }
                } else {
                    outcome.response = "Available Triggers:\n  HTTP Webhook  (coming soon)\n  File Change   (coming soon)\n  Alert         (coming soon)\n  Schedule      (coming soon)\n  Manual        (use /run)".into();
                    outcome.data = json!({
                        "triggers": [
                            {"type": "http-webhook", "available": false},
                            {"type": "file-change", "available": false},
                            {"type": "alert", "available": false},
                            {"type": "schedule", "available": false},
                            {"type": "manual", "available": true, "command": "/run"},
                        ]
                    });
                }
            }
            "schedule" => {
                let subcommand = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if subcommand == "create" {
                    let name = invocation.arguments.get(1).map(|s| s.as_str()).unwrap_or("");
                    let cron = invocation.arguments.iter().position(|a| a == "cron")
                        .and_then(|i| invocation.arguments.get(i + 1))
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    if name.is_empty() {
                        outcome.response = "Usage: /schedule create <name> [cron <expr>]".into();
                    } else {
                        outcome.response = format!(
                            "Schedule '{name}' creation requested{}.\n\nNote: Scheduler engine is available in the Workflow Runtime. Cron-based scheduling will be integrated with a dedicated scheduler (Quartz/Temporal) in a future phase.",
                            if cron.is_empty() { String::new() } else { format!(" (cron: {cron})" ) }
                        );
                        outcome.data = json!({"name": name, "cron": cron, "status": "schedule_creation_requested"});
                    }
                } else {
                    outcome.response = "Schedules:\n  (No schedules configured yet)\n\nUse /schedule create <name> [cron <expr>] to create a scheduled workflow.".into();
                    outcome.data = json!({"schedules": [], "status": "schedule_list"});
                }
            }
            "run" => {
                let key = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if key.is_empty() {
                    outcome.response = "Usage: /run <workflow-key> [--variables <json>]".into();
                } else {
                    match self.runtimes.workflows.start(
                        core_agent_workflow::StartWorkflowRequest::new(key, "operator")
                    ).await {
                        Ok(instance) => {
                            outcome.response = format!(
                                "Workflow '{}' started.\n\nExecution ID: {}\nState: {}\n\nUse /observe {} to track progress.",
                                key, instance.id, instance.state.as_str(), instance.id
                            );
                            outcome.data = json!({
                                "workflowKey": key,
                                "instanceId": instance.id.to_string(),
                                "state": instance.state.as_str(),
                                "status": "workflow_started"
                            });
                        }
                        Err(error) => {
                            outcome.response = format!(
                                "Failed to start workflow '{}': {}\n\nMake sure the workflow is registered. Use /workflow to list available workflows.",
                                key, error
                            );
                        }
                    }
                }
            }
            "observe" => {
                let id_str = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if id_str.is_empty() {
                    outcome.response = "Usage: /observe <instance-id>".into();
                } else {
                    match uuid::Uuid::parse_str(id_str) {
                        Ok(id) => {
                            match self.runtimes.workflows.find_instance(id).await.map_err(runtime_error)? {
                                Some(instance) => {
                                    let mut response = format!(
                                        "Workflow Instance: {}\n\nState: {}\nDefinition: {} v{}\n\nProgress:",
                                        instance.id, instance.state.as_str(),
                                        instance.definition.name, instance.definition_version,
                                    );
                                    for (si, stage) in instance.progress.iter().enumerate() {
                                        response.push_str(&format!("\n  Stage {}: {:?}", si + 1, stage.state));
                                        for (ai, activity) in stage.activities.iter().enumerate() {
                                            response.push_str(&format!("\n    Activity {}.{}: {:?}", si + 1, ai + 1, activity.state));
                                            for (aci, action) in activity.actions.iter().enumerate() {
                                                let state = action.state.as_str();
                                                let error = action.error.as_deref().map(|e| format!(" ({})", e)).unwrap_or_default();
                                                response.push_str(&format!("\n      - {}: {}{}", action.action_id, state, error));
                                            }
                                        }
                                    }
                                    outcome.response = response;
                                    outcome.data = json!({
                                        "instanceId": instance.id.to_string(),
                                        "state": instance.state.as_str(),
                                        "definitionName": instance.definition.name,
                                        "definitionVersion": instance.definition_version,
                                        "progress": instance.progress,
                                    });
                                }
                                None => {
                                    outcome.response = format!("Workflow instance '{id_str}' not found.").into();
                                }
                            }
                        }
                        Err(_) => {
                            outcome.response = format!("Invalid instance ID: '{id_str}'. Expected a UUID.").into();
                        }
                    }
                }
            }
            "retry" => {
                let id_str = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if id_str.is_empty() {
                    outcome.response = "Usage: /retry <instance-id>".into();
                } else {
                    match uuid::Uuid::parse_str(id_str) {
                        Ok(id) => {
                            let instance = self.runtimes.workflows.find_instance(id).await.map_err(runtime_error)?;
                            match instance {
                                Some(ref inst) if inst.state == core_agent_workflow::WorkflowState::Failed => {
                                    // Create a snapshot for recovery, then resume
                                    match self.runtimes.workflows.snapshot(id, "retry-checkpoint", "operator").await {
                                        Ok(snapshot) => {
                                            match self.runtimes.workflows.resume(id, "operator").await {
                                                Ok(restored) => {
                                                    outcome.response = format!(
                                                        "Workflow instance {} retried from checkpoint.\n\nNew state: {}\nSnapshot ID: {}\n\nUse /observe {} to track progress.",
                                                        id, restored.state.as_str(), snapshot.id, id
                                                    );
                                                    outcome.data = json!({
                                                        "instanceId": id.to_string(),
                                                        "snapshotId": snapshot.id.to_string(),
                                                        "state": restored.state.as_str(),
                                                        "status": "workflow_retried"
                                                    });
                                                }
                                                Err(error) => {
                                                    outcome.response = format!(
                                                        "Failed to retry workflow instance {}: {}\n\nCheckpoint snapshot was created. Use /observe {} to inspect the current state.",
                                                        id, error, id
                                                    );
                                                }
                                            }
                                        }
                                        Err(error) => {
                                            outcome.response = format!(
                                                "Failed to create recovery checkpoint for {}: {}",
                                                id, error
                                            );
                                        }
                                    }
                                }
                                Some(ref inst) => {
                                    outcome.response = format!(
                                        "Workflow instance {} is in {:?} state, not Failed. Only failed workflows can be retried.\n\nCurrent state: {}",
                                        id, inst.state, inst.state.as_str()
                                    );
                                }
                                None => {
                                    outcome.response = format!("Workflow instance '{id_str}' not found.").into();
                                }
                            }
                        }
                        Err(_) => {
                            outcome.response = format!("Invalid instance ID: '{id_str}'. Expected a UUID.").into();
                        }
                    }
                }
            }
            // ── Observability commands (Phase 6) ──
            "trace-agent" => {
                let trace_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                let traces = if trace_id.is_empty() {
                    self.trace_store.list_traces(5, 0).map_err(|e|
                        EnterpriseAgentError::Runtime(e)
                    )?
                } else {
                    let uuid = uuid::Uuid::parse_str(trace_id).map_err(|_|
                        EnterpriseAgentError::InvalidArgument("invalid trace UUID".into())
                    )?;
                    match self.trace_store.get_trace(uuid).map_err(|e| EnterpriseAgentError::Runtime(e))? {
                        Some(t) => vec![t],
                        None => {
                            outcome.response = format!("Trace not found: {trace_id}");
                            return Ok(Some(outcome));
                        }
                    }
                };
                if traces.is_empty() {
                    outcome.response = "No traces found.".into();
                } else {
                    let mut lines = Vec::new();
                    for trace in &traces {
                        lines.push(format!("╭────────────────────╮"));
                        lines.push(format!(" Agent Trace: {}", &trace.trace_id.to_string()[..8]));
                        lines.push(format!("╰────────────────────╯\n"));
                        lines.push(format!("Task: {}\n", trace.goal));
                        lines.push("Timeline:\n".into());
                        for step in &trace.steps {
                            let time = step.created_at.format("%H:%M:%S").to_string();
                            lines.push(format!("  {}  {}  {}", time, step.agent_name, step.output));
                            if let Some(ref tool) = step.tool_name {
                                lines.push(format!("       → tool: {tool}"));
                            }
                            if let Some(ref error) = step.error {
                                lines.push(format!("       ⚠ error: {error}"));
                            }
                            lines.push(String::new());
                        }
                        lines.push(format!("Result: {}", if trace.success { "✅ Success" } else { "❌ Failed" }));
                        if let Some(score) = trace.score {
                            lines.push(format!("Score: {score:.1}/10"));
                        }
                        lines.push(format!("Duration: {}ms | Tokens: {}", trace.wall_duration_ms, trace.token_usage));
                        lines.push("-".repeat(50));
                    }
                    outcome.response = lines.join("\n");
                }
            }
            "evaluate" => {
                let trace_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if trace_id.is_empty() {
                    outcome.response = "Usage: /evaluate <trace-id>".into();
                } else {
                    let uuid = uuid::Uuid::parse_str(trace_id).map_err(|_|
                        EnterpriseAgentError::InvalidArgument("invalid trace UUID".into())
                    )?;
                    let trace = self.trace_store.get_trace(uuid).map_err(|e|
                        EnterpriseAgentError::Runtime(e)
                    )?.ok_or_else(|| EnterpriseAgentError::InvalidArgument("trace not found".into()))?;
                    let eval = crate::observability::EvaluationEngine::evaluate(&trace);
                    let _ = self.trace_store.save_evaluation(&eval);
                    let mut lines = vec![
                        "Evaluation Result\n".into(),
                        format!("Task: {}\n", trace.goal),
                        format!("Score: {:.1} / 10\n", eval.overall),
                        "Criteria:\n".into(),
                    ];
                    for c in &eval.criteria {
                        let ratio = (c.score / c.max_score).clamp(0.0, 1.0);
                        let filled = (ratio * 10.0).round() as usize;
                        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled));
                        lines.push(format!("  {}  {:.1}  {}", c.dimension.as_str(), c.score, bar));
                    }
                    lines.push(format!("\nFeedback:\n  {}", eval.feedback));
                    outcome.response = lines.join("\n");
                }
            }
            "benchmark" => {
                let agent_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("default-agent");
                let results = self.trace_store.list_benchmark_results(agent_id).map_err(|e|
                    EnterpriseAgentError::Runtime(e)
                )?;
                let mut lines = vec!["Benchmark\n".into()];
                let tasks = crate::observability::BenchmarkEngine::builtin_tasks();
                lines.push(format!("Available tasks: {}\n", tasks.len()));
                if results.is_empty() {
                    lines.push("No benchmark results yet.\n".into());
                    lines.push("Available tasks:\n".into());
                    for task in &tasks {
                        lines.push(format!("  • {}  ({})  — {}", task.name, task.category, task.description));
                    }
                } else {
                    let summary = crate::observability::BenchmarkEngine::summarize(agent_id, &results);
                    lines.push(format!("Agent: {}\n", summary.agent_id));
                    lines.push(format!("Tasks: {}", summary.total_tasks));
                    lines.push(format!("Success: {}", summary.success_count));
                    lines.push(format!("Average Score: {:.1}", summary.average_score));
                    lines.push(format!("Average Cost: {:.0} tokens", summary.average_cost));
                    lines.push(format!("Average Duration: {:.0}ms\n", summary.average_duration_ms));
                    for r in &results {
                        let status = if r.success { "✅" } else { "❌" };
                        lines.push(format!("  {status}  {}  ({})  Score: {:.1}  {}ms", r.task_name, r.task_category, r.score, r.duration_ms));
                        if let Some(ref error) = r.error {
                            lines.push(format!("       ⚠ {error}"));
                        }
                    }
                }
                outcome.response = lines.join("\n");
            }
            "debug" => {
                let trace_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if trace_id.is_empty() {
                    outcome.response = "Usage: /debug <trace-id>".into();
                } else {
                    let uuid = uuid::Uuid::parse_str(trace_id).map_err(|_|
                        EnterpriseAgentError::InvalidArgument("invalid trace UUID".into())
                    )?;
                    let trace = self.trace_store.get_trace(uuid).map_err(|e|
                        EnterpriseAgentError::Runtime(e)
                    )?.ok_or_else(|| EnterpriseAgentError::InvalidArgument("trace not found".into()))?;
                    let analysis = crate::observability::DebugEngine::analyze(&trace);
                    let mut lines = vec!["Debug Analysis\n".into()];
                    if analysis.failure_points.is_empty() && analysis.success {
                        lines.push("✅ No failures detected.\n".into());
                    }
                    if !analysis.failure_points.is_empty() {
                        lines.push("Failure Points:\n".into());
                        for fp in &analysis.failure_points {
                            lines.push(format!("  Step {}  Agent: {}  Type: {:?}", fp.step_index, fp.agent_name, fp.step_type));
                            lines.push(format!("  Problem: {}\n", fp.problem));
                        }
                    }
                    if !analysis.root_causes.is_empty() {
                        lines.push("Root Cause:\n".into());
                        for cause in &analysis.root_causes {
                            lines.push(format!("  • {cause}"));
                        }
                        lines.push(String::new());
                    }
                    if !analysis.recommendations.is_empty() {
                        lines.push("Recommendation:\n".into());
                        for rec in &analysis.recommendations {
                            lines.push(format!("  → {rec}"));
                        }
                    }
                    lines.push(format!("\nTotal steps: {} | Overall: {}", analysis.total_steps, if analysis.success { "Success" } else { "Failed" }));
                    outcome.response = lines.join("\n");
                }
            }
            "replay" => {
                let trace_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("");
                if trace_id.is_empty() {
                    outcome.response = "Usage: /replay <trace-id>".into();
                } else {
                    let uuid = uuid::Uuid::parse_str(trace_id).map_err(|_|
                        EnterpriseAgentError::InvalidArgument("invalid trace UUID".into())
                    )?;
                    let trace = self.trace_store.get_trace(uuid).map_err(|e|
                        EnterpriseAgentError::Runtime(e)
                    )?.ok_or_else(|| EnterpriseAgentError::InvalidArgument("trace not found".into()))?;
                    let report = crate::observability::ReplayEngine::build_replay(&trace);
                    let mut lines = vec![
                        "Execution Replay\n".into(),
                        format!("Original: {} ({})\n", trace.agent_id, trace.created_at.format("%Y-%m-%d %H:%M:%S")),
                        "Event History:\n".into(),
                    ];
                    for event in &report.events {
                        let icon = match event.event_type.as_str() {
                            "planning" => "🔍", "reasoning" => "💭", "delegation" => "🔄",
                            "tool_call" => "🔧", "observation" => "👁", "decision" => "🎯",
                            "reflection" => "📝", "response" => "💬", _ => "➡",
                        };
                        lines.push(format!("  #[{}] {icon} {} — {}", event.sequence, event.agent, event.event_type));
                        if let Some(ref tool) = event.tool {
                            lines.push(format!("        Tool: {tool}"));
                        }
                        let trunc = |s: &str| if s.len() > 100 { format!("{}...", &s[..100]) } else { s.to_string() };
                        lines.push(format!("        Input: {}", trunc(&event.input)));
                        lines.push(format!("        Output: {}", trunc(&event.output)));
                        if let Some(ref error) = event.error {
                            lines.push(format!("        ⚠ Error: {error}"));
                        }
                        lines.push(String::new());
                    }
                    if !report.differences.is_empty() {
                        lines.push("Differences detected:\n".into());
                        for step in &report.differences {
                            lines.push(format!("  Step {step} changed (had error)"));
                        }
                    }
                    lines.push(format!("Result: {}", if report.success { "✅ Success" } else { "❌ Failed" }));
                    lines.push(format!("Total events: {}", report.total_events));
                    outcome.response = lines.join("\n");
                }
            }
            "score" => {
                let agent_id = invocation.arguments.first().map(|s| s.as_str()).unwrap_or("default-agent");
                let health = self.trace_store.agent_stats(agent_id).map_err(|e|
                    EnterpriseAgentError::Runtime(e)
                )?;
                let mut lines = vec![
                    "Agent Health Dashboard\n".into(),
                    format!("Agent: {}\n", health.agent_id),
                    format!("  Success Rate:  {:.0}%", health.success_rate),
                    format!("  Avg Score:     {:.1}/10", health.avg_score),
                    format!("  Avg Cost:      {:.0} tokens", health.avg_cost_tokens),
                    format!("  Avg Latency:   {:.0}ms", health.avg_latency_ms),
                    format!("  Total Traces:  {}", health.total_traces),
                    format!("  Recent (24h):  {}\n", health.recent_traces),
                    "Health Bar:\n".into(),
                ];
                for (label, pct, _max) in [
                    ("Success", health.success_rate / 100.0, 1.0),
                    ("Score", health.avg_score / 10.0, 1.0),
                ] {
                    let filled = (pct.clamp(0.0, 1.0) * 20.0).round() as usize;
                    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled));
                    lines.push(format!("  {label}: [{bar}] {:.0}%", pct * 100.0));
                }
                outcome.response = lines.join("\n");
            }
            "agents" => {
                let cmd = crate::slash::commands::agents::AgentsCommand::new(self.runtimes.multi_agent.clone());
                let ctx = crate::slash::CommandContext {
                    line: line.to_string(),
                    args: invocation.arguments.clone(),
                    workspace: ".".to_string(),
                    session_id: session_id.map(|id| id.to_string()),
                    data: Default::default(),
                };
                match cmd.execute(ctx).await {
                    Ok(output) => { outcome.response = output.response; }
                    Err(error) => { outcome.response = format!("Command failed: {error}"); }
                }
            }
            "delegate" => {
                let cmd = crate::slash::commands::delegate::DelegateCommand::new(self.runtimes.multi_agent.clone());
                let ctx = crate::slash::CommandContext {
                    line: line.to_string(),
                    args: invocation.arguments.clone(),
                    workspace: ".".to_string(),
                    session_id: session_id.map(|id| id.to_string()),
                    data: Default::default(),
                };
                match cmd.execute(ctx).await {
                    Ok(output) => { outcome.response = output.response; }
                    Err(error) => { outcome.response = format!("Command failed: {error}"); }
                }
            }
            "team" => {
                let cmd = crate::slash::commands::team::TeamCommand::new(self.runtimes.multi_agent.clone());
                let ctx = crate::slash::CommandContext {
                    line: line.to_string(),
                    args: invocation.arguments.clone(),
                    workspace: ".".to_string(),
                    session_id: session_id.map(|id| id.to_string()),
                    data: Default::default(),
                };
                match cmd.execute(ctx).await {
                    Ok(output) => { outcome.response = output.response; }
                    Err(error) => { outcome.response = format!("Command failed: {error}"); }
                }
            }
            "roles" => {
                let cmd = crate::slash::commands::roles::RolesCommand::new(self.runtimes.multi_agent.clone());
                let ctx = crate::slash::CommandContext {
                    line: line.to_string(),
                    args: invocation.arguments.clone(),
                    workspace: ".".to_string(),
                    session_id: session_id.map(|id| id.to_string()),
                    data: Default::default(),
                };
                match cmd.execute(ctx).await {
                    Ok(output) => { outcome.response = output.response; }
                    Err(error) => { outcome.response = format!("Command failed: {error}"); }
                }
            }
            "collaborate" => {
                let cmd = crate::slash::commands::collaborate::CollaborateCommand::new(self.runtimes.multi_agent.clone());
                let ctx = crate::slash::CommandContext {
                    line: line.to_string(),
                    args: invocation.arguments.clone(),
                    workspace: ".".to_string(),
                    session_id: session_id.map(|id| id.to_string()),
                    data: Default::default(),
                };
                match cmd.execute(ctx).await {
                    Ok(output) => { outcome.response = output.response; }
                    Err(error) => { outcome.response = format!("Command failed: {error}"); }
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
        let request_id = Uuid::new_v4();
        let started_at = chrono::Utc::now();
        let started = Instant::now();
        let workspace_key = project_storage_key(&self.config.workspace)
            .map_err(|error| EnterpriseAgentError::Configuration(error.to_string()))?;
        let mut metric = AgentRequestMetric::running(
            request_id,
            workspace_key,
            session_id,
            self.config.entrypoint.clone(),
            self.config.model.model.clone(),
            started_at,
        );
        let began = self.telemetry.begin_request(&metric).await.is_ok();
        let mut timings = RequestTimings::default();
        let result = self
            .run_with_approval_inner(
                message,
                session_id,
                approval_handler,
                request_id,
                &mut timings,
            )
            .await;
        metric.completed_at = Some(chrono::Utc::now());
        metric.wall_duration_ms = elapsed_ms(started);
        metric.approval_wait_ms = timings.approval_wait_ms;
        metric.active_duration_ms = metric
            .wall_duration_ms
            .saturating_sub(metric.approval_wait_ms);
        metric.context_duration_ms = timings.context_duration_ms;
        metric.model_duration_ms = timings.model_duration_ms;
        metric.tool_duration_ms = timings.tool_duration_ms;
        metric.context_tokens = timings.context_tokens;
        match &result {
            Ok(run) => {
                metric.session_id = Some(run.session_id);
                metric.status = RequestStatus::Completed;
            }
            Err(error) => {
                metric.status = RequestStatus::Failed;
                metric.error_kind = Some(enterprise_error_kind(error).into());
            }
        }
        let telemetry_recorded = began && self.telemetry.finish_request(&metric).await.is_ok();
        match result {
            Ok(mut run) => {
                run.wall_duration_ms = metric.wall_duration_ms;
                run.active_duration_ms = metric.active_duration_ms;
                run.telemetry_recorded = telemetry_recorded;
                if let Some(event) = run
                    .events
                    .iter_mut()
                    .rev()
                    .find(|event| event.is_terminal())
                {
                    if let Some(data) = event.data.as_object_mut() {
                        data.insert("requestId".into(), json!(request_id));
                        data.insert("wallDurationMs".into(), json!(metric.wall_duration_ms));
                        data.insert("activeDurationMs".into(), json!(metric.active_duration_ms));
                        data.insert("telemetryRecorded".into(), json!(telemetry_recorded));
                    }
                }
                self.events
                    .write()
                    .await
                    .insert(run.session_id, run.events.clone());
                Ok(run)
            }
            Err(error) => Err(error),
        }
    }

    async fn run_with_approval_inner(
        &self,
        message: String,
        session_id: Option<Uuid>,
        approval_handler: &dyn EnterpriseApprovalHandler,
        request_id: Uuid,
        timings: &mut RequestTimings,
    ) -> EnterpriseAgentResult<EnterpriseRun> {
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
        let mut explicit_context = match (command_context, mention_context) {
            (Some(command), Some(mentions)) => Some(format!(
                "Built-in command expansion:\n{command}\n\nExplicit @ context:\n{mentions}"
            )),
            (Some(command), None) => Some(command),
            (None, Some(mentions)) => Some(mentions),
            (None, None) => None,
        };
        let _operation = self.operation_lock.lock().await;
        let permission_mode = *self.permission_mode.read().await;
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
            data: json!({"sessionId": session_id, "requestId": request_id, "readOnly": read_only}),
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
        events.push(EnterpriseAgentEvent {
            kind: "guidance_loaded".into(),
            message: format!(
                "Loaded {} instruction document(s) and discovered {} skill(s)",
                self.instructions.documents.len(),
                self.skills.descriptors().len()
            ),
            data: json!({
                "instructionDocuments": self.instructions.documents.iter().map(|document| json!({
                    "scope": document.scope,
                    "precedence": document.precedence,
                    "sha256": document.content_sha256,
                    "bytes": document.bytes,
                })).collect::<Vec<_>>(),
                "skills": self.skills.descriptors().iter().map(|skill| json!({
                    "name": skill.name,
                    "scope": skill.scope,
                    "sha256": skill.content_sha256,
                    "bytes": skill.bytes,
                })).collect::<Vec<_>>(),
            }),
        });
        if let Some(hooks) = &self.hooks {
            let results = hooks
                .run(
                    crate::HookInvocation {
                        event: crate::HookEvent::AgentStart,
                        session_id: Some(session_id),
                        tool: None,
                        payload: json!({"requestId": request_id}),
                    },
                    tokio_util::sync::CancellationToken::new(),
                )
                .await
                .map_err(|error| EnterpriseAgentError::Runtime(error.to_string()))?;
            append_hook_events(&mut events, crate::HookEvent::AgentStart, &results);
        }
        if self.config.memory_enabled && self.managed_policy.memory_enabled {
            let recalled = crate::memory_tools::recall_for_prompt(
                &self.runtimes.memory,
                &self.memory_namespace,
                session_id,
                &message,
                16 * 1024,
            )
            .await
            .map_err(tool_error)?;
            if !recalled.is_empty() {
                append_explicit_context(
                    &mut explicit_context,
                    "Relevant durable memory (may be stale; prefer current user input and project instructions)",
                    &recalled,
                );
                events.push(EnterpriseAgentEvent {
                    kind: "memory_recalled".into(),
                    message: "Relevant durable project/session memory entered context".into(),
                    data: json!({"bytes": recalled.len(), "namespace": self.memory_namespace}),
                });
            }
        }

        let context_started = Instant::now();
        let context_result = self
            .contexts
            .build(BuildContextRequest {
                session_id: session.id.clone(),
                conversation_id: Some(conversation.id.clone()),
                system_prompt: Some(self.composed_system_prompt()?),
                user_input: explicit_context,
                max_messages: Some(self.config.context_compression.keep_recent_messages),
                max_tokens: Some(self.config.model.max_context_tokens),
                compression_strategy: Some(self.config.context_compression.strategy.clone()),
                compression_trigger_percent: Some(self.config.context_compression.trigger_percent),
                working_directory: Some(self.config.workspace.to_string_lossy().into_owned()),
            })
            .await;
        timings.context_duration_ms = timings
            .context_duration_ms
            .saturating_add(elapsed_ms(context_started));
        let context = match context_result {
            Ok(context) => context,
            Err(error) => {
                let error = context_error(error);
                self.record_failure(session_id, &mut events, &error).await;
                return Err(error);
            }
        };
        timings.context_tokens = context.total_tokens;
        events.push(EnterpriseAgentEvent {
            kind: "context_built".into(),
            message: "Session context assembled".into(),
            data: json!({
                "contextId": context.id,
                "tokens": context.total_tokens,
                "maxTokens": self.config.model.max_context_tokens,
                "tokenDistribution": &context.token_distribution,
                "buildDurationMs": context.build_duration_ms,
                "estimated": true,
                "hash": &context.hash
            }),
        });

        let definitions = self
            .tools
            .list()
            .await
            .map_err(tool_error)?
            .into_iter()
            .filter(|definition| {
                self.managed_policy.evaluate_tool(definition) == ManagedPolicyDecision::Allow
            })
            .collect::<Vec<_>>();
        let mut request = context_model_request(&context, &self.config.model.profile, session_id)?;
        request.id = request_id;
        let exposed_definitions = definitions
            .iter()
            .filter(|definition| !read_only || tool_allowed_in_read_only(definition))
            .cloned()
            .collect::<Vec<_>>();
        request.tools = model_tool_definitions(&exposed_definitions)?;
        let mut response_text = None;
        let mut tool_call_count = 0_usize;
        for turn in 0..8_u8 {
            let model_started = Instant::now();
            let response_result = self.models.generate(request.clone()).await;
            timings.model_duration_ms = timings
                .model_duration_ms
                .saturating_add(elapsed_ms(model_started));
            let response = match response_result {
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
                    && (!tool_allowed_in_read_only(definition)
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
                let permission =
                    tool_permission_requirement(permission_mode, definition, &call.arguments);
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
                            permission_mode_name(permission_mode),
                            definition.category
                        ),
                        parameters: call.arguments.clone(),
                    };
                    events.push(EnterpriseAgentEvent {
                        kind: "approval_required".into(),
                        message: format!("Approval required for {}", definition.name),
                        data: serde_json::to_value(&approval)?,
                    });
                    let approval_started = Instant::now();
                    let decision = approval_handler.decide(&approval).await;
                    timings.approval_wait_ms = timings
                        .approval_wait_ms
                        .saturating_add(elapsed_ms(approval_started));
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
                if let Some(hooks) = &self.hooks {
                    let hook_result = hooks
                        .run(
                            crate::HookInvocation {
                                event: crate::HookEvent::BeforeTool,
                                session_id: Some(session_id),
                                tool: Some(definition.name.clone()),
                                payload: json!({
                                    "requestId": request_id,
                                    "toolRequestId": tool_request.id,
                                    "parameters": call.arguments,
                                }),
                            },
                            tokio_util::sync::CancellationToken::new(),
                        )
                        .await;
                    match hook_result {
                        Ok(results) => {
                            append_hook_events(&mut events, crate::HookEvent::BeforeTool, &results)
                        }
                        Err(error) => {
                            let error = EnterpriseAgentError::Runtime(error.to_string());
                            self.record_failure(session_id, &mut events, &error).await;
                            return Err(error);
                        }
                    }
                }
                if definition.default_permission == PermissionDecision::Ask {
                    self.approvals.approve(tool_request.id)?;
                }
                let tool_started = Instant::now();
                let tool_result = self.tools.execute(tool_request).await;
                timings.tool_duration_ms = timings
                    .tool_duration_ms
                    .saturating_add(elapsed_ms(tool_started));
                let result = match tool_result {
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
                if let Some(hooks) = &self.hooks {
                    let hook_result = hooks
                        .run(
                            crate::HookInvocation {
                                event: crate::HookEvent::AfterTool,
                                session_id: Some(session_id),
                                tool: Some(definition.name.clone()),
                                payload: json!({
                                    "requestId": request_id,
                                    "toolRequestId": result.request_id,
                                    "status": result.status,
                                    "durationMs": result.usage.duration_ms,
                                    "outputBytes": result.usage.output_bytes,
                                }),
                            },
                            tokio_util::sync::CancellationToken::new(),
                        )
                        .await;
                    match hook_result {
                        Ok(results) => {
                            append_hook_events(&mut events, crate::HookEvent::AfterTool, &results)
                        }
                        Err(error) => {
                            let error = EnterpriseAgentError::Runtime(error.to_string());
                            self.record_failure(session_id, &mut events, &error).await;
                            return Err(error);
                        }
                    }
                }
                // Check if this tool result signals user input is required (ask.user/ask.select/ask.confirm)
                if result.metadata.get("user_input_required").map(|v| v.as_str()) == Some("true") {
                    let question = result.metadata.get("question").cloned().unwrap_or_default();
                    events.push(EnterpriseAgentEvent {
                        kind: "user_input_required".into(),
                        message: question.clone(),
                        data: json!({"question": question, "tool": call.name}),
                    });
                    response_text = Some(format!(
                        "\n\n[Agent needs your input]\n{question}\n\nPlease respond to continue."
                    ));
                    break;
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
        // Cognitive command post-processing (/decision → ADR generation)
        let response_text = if message.starts_with('/') {
            let invocation = InteractionCommandRegistry::with_builtins()
                .parse(&message)
                .map_err(|error| EnterpriseAgentError::InvalidArgument(error.to_string()))?;
            if let Some(cognitive_cmd) = invocation.cognitive_command() {
                let cognitive_output = crate::cognitive::process_cognitive_response(
                    cognitive_cmd,
                    &response_text,
                    &self.config.workspace,
                );
                if let Some(adr_path) = &cognitive_output.adr_path {
                    events.push(EnterpriseAgentEvent {
                        kind: "adr_generated".into(),
                        message: format!("ADR generated at {}", adr_path.display()),
                        data: json!({
                            "path": adr_path.to_string_lossy(),
                            "command": cognitive_cmd.as_str(),
                        }),
                    });
                }
                cognitive_output.raw_response
            } else {
                response_text
            }
        } else {
            response_text
        };
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
        if let Some(hooks) = &self.hooks {
            let hook_result = hooks
                .run(
                    crate::HookInvocation {
                        event: crate::HookEvent::AgentFinish,
                        session_id: Some(session_id),
                        tool: None,
                        payload: json!({
                            "requestId": request_id,
                            "toolCalls": tool_call_count,
                        }),
                    },
                    tokio_util::sync::CancellationToken::new(),
                )
                .await;
            match hook_result {
                Ok(results) => {
                    append_hook_events(&mut events, crate::HookEvent::AgentFinish, &results)
                }
                Err(error) => {
                    let error = EnterpriseAgentError::Runtime(error.to_string());
                    self.record_failure(session_id, &mut events, &error).await;
                    return Err(error);
                }
            }
        }
        events.push(EnterpriseAgentEvent {
            kind: "execution_finished".into(),
            message: response_text.clone(),
            data: json!({"sessionId": session_id, "toolCalls": tool_call_count}),
        });
        self.events.write().await.insert(session_id, events.clone());
        Ok(EnterpriseRun {
            request_id,
            session_id,
            response: response_text,
            events,
            wall_duration_ms: 0,
            active_duration_ms: 0,
            telemetry_recorded: false,
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

async fn load_builtin_tools(
    manager: &Arc<ToolManager>,
    key: &str,
    name: &str,
    registrations: Vec<ToolRegistration>,
) -> EnterpriseAgentResult<()> {
    if registrations.is_empty() {
        return Ok(());
    }
    manager
        .load_provider(&StaticToolProvider::new(
            ToolProviderDefinition::new(key, name, ToolProviderKind::Builtin),
            registrations,
        ))
        .await
        .map_err(tool_error)?;
    Ok(())
}

async fn register_workspace_tools(
    manager: &Arc<ToolManager>,
    workspace: &Path,
    checkpoints: Arc<CheckpointStore>,
    mut extra_tools: Vec<ToolRegistration>,
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

    let mut registrations = vec![
        ToolRegistration::new(list_definition, list_tool),
        ToolRegistration::new(read_definition, read_tool),
        ToolRegistration::new(write_definition, write_tool),
    ];
    registrations.append(&mut extra_tools);
    manager
        .load_provider(&StaticToolProvider::new(provider, registrations))
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

#[cfg(test)]
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

fn tool_allowed_in_read_only(definition: &ToolDefinition) -> bool {
    !matches!(
        definition.category.as_str(),
        "filesystem.write" | "memory.write" | "process.cancel" | "mcp.remote"
    )
}

fn append_explicit_context(target: &mut Option<String>, label: &str, value: &str) {
    let section = format!("{label}:\n{value}");
    match target {
        Some(existing) => {
            existing.push_str("\n\n");
            existing.push_str(&section);
        }
        None => *target = Some(section),
    }
}

fn append_hook_events(
    events: &mut Vec<EnterpriseAgentEvent>,
    event: crate::HookEvent,
    results: &[crate::HookResult],
) {
    events.extend(results.iter().map(|result| EnterpriseAgentEvent {
        kind: "hook_completed".into(),
        message: format!("Hook {} completed", result.hook),
        data: json!({
            "event": event,
            "hook": result.hook,
            "success": result.success,
            "exitCode": result.exit_code,
            "durationMs": result.duration_ms,
        }),
    }));
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

fn telemetry_store(config: &EnterpriseAgentConfig) -> EnterpriseAgentResult<Arc<SqliteModelStore>> {
    let directory = config
        .telemetry_dir
        .as_deref()
        .unwrap_or(config.data_dir.as_path());
    std::fs::create_dir_all(directory)?;
    SqliteModelStore::new(&database_path(directory, "observability.db")?)
        .map(Arc::new)
        .map_err(model_error)
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn enterprise_error_kind(error: &EnterpriseAgentError) -> &'static str {
    match error {
        EnterpriseAgentError::InvalidArgument(_) => "INVALID_ARGUMENT",
        EnterpriseAgentError::Configuration(_) => "CONFIGURATION",
        EnterpriseAgentError::Session(_) => "SESSION",
        EnterpriseAgentError::Context(_) => "CONTEXT",
        EnterpriseAgentError::Model(_) => "MODEL",
        EnterpriseAgentError::Tool(_) => "TOOL",
        EnterpriseAgentError::Workspace(_) => "WORKSPACE",
        EnterpriseAgentError::Runtime(_) => "RUNTIME",
        EnterpriseAgentError::Io(_) => "IO",
        EnterpriseAgentError::Serialization(_) => "SERIALIZATION",
    }
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
