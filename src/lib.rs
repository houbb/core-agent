//! core-agent — Enterprise Agent Runtime
//!
//! Agent Operating System — 企业级 Agent 运行时平台。
//!
//! 当前阶段：Phase 0 (Session Runtime) + Phase 1 (Context Runtime)
//! + Phase 2 (Model Runtime) + Phase 3 (Tool Runtime)
//! + Phase 4 (Workspace Runtime)
//! + Phase 5 (Planning Runtime)
//! + Phase 6 (Execution Runtime)
//! + Phase 7 (Agent Runtime)
//! + Phase 8 (Memory Runtime)
//! + Phase 9 (Event Runtime)
//! + Phase 10 (Workflow Runtime)
//! + Phase 11 (Multi-Agent Runtime)
//! + Phase 12 (Extension Runtime)
//! + Phase 13 (Platform Runtime)

mod checkpoint;
mod command_runtime;
mod enterprise;
mod guidance;
mod hook_runtime;
mod interaction;
mod managed_policy;
mod mcp_runtime;
mod memory_tools;
mod skill_tools;
mod subagent_runtime;
mod web_runtime;
mod workspace_tools;

pub use command_runtime::{
    BackgroundCommandManager, BackgroundCommandSnapshot, BackgroundCommandStatus, CommandEvent,
    CommandObserver, CommandOutcome as GovernedCommandOutcome,
    CommandRequest as GovernedCommandRequest, CommandRunError, CommandRunResult, CommandRunner,
    CommandSandboxPolicy, CommandSandboxStatus, CommandStream, LocalCommandRunner,
    NoopCommandObserver, SandboxNetworkPolicy, SandboxRequirement,
};
pub use hook_runtime::{
    HookEvent, HookFailurePolicy, HookInvocation, HookResult, HookRule, HookRuntime,
    HookRuntimeError, HookRuntimeResult,
};
pub use managed_policy::{
    ManagedAgentPolicy, ManagedPolicyDecision, ManagedPolicyError, ManagedPolicyResult,
};
pub use mcp_runtime::{
    discover_mcp_servers, McpClient, McpRuntimeError, McpRuntimeResult, McpServerConfig,
    McpToolProvider,
};
pub use subagent_runtime::{SubAgentOutcome, SubAgentProfile, SubAgentRuntime};
pub use web_runtime::{
    OpenAiWebSearchProvider, WebDomainPolicy, WebFetchResult, WebRuntime, WebRuntimeError,
    WebRuntimeResult, WebSearchProvider, WebSearchRequest, WebSearchResult, WebSource,
};

pub use enterprise::{
    EnterpriseAgent, EnterpriseAgentConfig, EnterpriseAgentError, EnterpriseAgentEvent,
    EnterpriseAgentResult, EnterpriseApprovalDecision, EnterpriseApprovalHandler,
    EnterpriseApprovalRequest, EnterpriseCommandAction, EnterpriseCommandOutcome,
    EnterpriseModelConfig, EnterpriseRun, EnterpriseRuntimes, EnterpriseSessionStatus,
    PermissionMode,
};
pub use guidance::{
    default_guidance_home, default_skill_roots, GuidanceError, GuidanceResult, GuidanceScope,
    InstructionChain, InstructionDocument, LoadedSkill, SkillCatalog, SkillDescriptor, SkillRoot,
    DEFAULT_INSTRUCTION_BUDGET_BYTES, DEFAULT_MAX_SKILLS, DEFAULT_SKILL_FILE_LIMIT_BYTES,
    DEFAULT_SKILL_METADATA_BUDGET_BYTES,
};
pub use interaction::{
    ContextCandidateIndex, ContextCandidateSearch, ContextMentionLimits, ContextMentionResolver,
    InteractionCommandDefinition, InteractionCommandInvocation, InteractionCommandRegistry,
    InteractionCommandRoute, InteractionEntryAction, InteractionEntryOutcome, InteractionError,
    InteractionResult, ResolvedContextItem, ResolvedContextMentions,
};

pub use core_agent_config as config_runtime;
pub use core_agent_config::{
    project_storage_key, standard_config_manager, AgentConfig, ConfigCompression, ConfigManager,
    ConfigManagerBuilder, ConfigModel, ConfigProvider, ConfigRequest, ConfigSourceInfo,
    EnvironmentConfigProvider, EnvironmentSecretResolver, ProjectFileConfigProvider,
    ResolvedConfig, SecretResolver, UserConfigSnapshot, UserConfigUpdate, UserConfigWriter,
    UserFileConfigProvider,
};

pub use core_agent_agent as agent_runtime;
pub use core_agent_agent::{
    Agent, AgentCapability, AgentCommit, AgentCoordinator, AgentError, AgentExecutionControl,
    AgentFactory, AgentGoalRequest, AgentInterceptor, AgentLifecycle, AgentManager,
    AgentManagerBuilder, AgentMetadata, AgentObservation, AgentOperation, AgentPolicy,
    AgentPolicyDecision, AgentPolicyDefinition, AgentProfile, AgentRegistry, AgentResult,
    AgentRunOutcome, AgentRunReference, AgentRuntime, AgentSnapshot, AgentSnapshotStore,
    AgentStage, AgentState, AgentStateRecord, AgentStore, CreateAgentRequest, DefaultAgentFactory,
    DefaultAgentLifecycle, EmbeddedAgentPolicy, InMemoryAgentStore, RuntimeAgentCoordinator,
    SqliteAgentStore, UnavailableAgentCoordinator,
};
pub use core_agent_app as app_contract;
pub use core_agent_app::{
    evaluate_readiness, ExperienceSurface, PhaseDefinition, PhaseReadiness, ProductCapability,
    ProductPhase,
};
pub use core_agent_collaboration as collaboration_runtime;
pub use core_agent_collaboration::{
    ActivityRecord as CollaborationActivity, ApprovalRecord, CollaborationPlatformError,
    CollaborationPlatformManager, CollaborationPlatformResult, KnowledgeAsset, KnowledgeState,
    ProjectRole, ProjectState, ReviewDecision as CollaborationReviewDecision, ReviewState,
    TaskState as CollaborationTaskState, TeamProject, TeamReview, TeamTask,
};
pub use core_agent_context as context_runtime;
pub use core_agent_context::{
    BuildContextRequest, Context, ContextCache, ContextComposer, ContextError, ContextMessage,
    ContextObservation, ContextObserver, ContextPipeline, ContextProvider, ContextReducer,
    ContextResponse, ContextResult, ContextRuntime, ContextSegment, ContextSerializer, ContextSlot,
    ContextSnapshotMeta, ContextSnapshotResponse, ContextSnapshotStore, ContextSource,
    ContextStage, DefaultComposer, EnvironmentContext, JsonContextSerializer,
    ListResponse as ContextListResponse, MemoryContext, PluginContext, ProviderContext,
    ReducerConfig, SlotConfig, SqliteContextSnapshotStore, SummaryReducer, SystemContext,
    TokenCounter, TokenDistribution, ToolContext, UserContext, WorkspaceContext,
};
pub use core_agent_ecosystem as ecosystem_runtime;
pub use core_agent_ecosystem::{
    EcosystemError, EcosystemManager, EcosystemResult, InstallationPlan, MarketplacePackage,
    PackageCoordinate, PackageDependency, PackageKind, PackageRating, PackageState,
    PublicationDecision, PublicationReview, Publisher, PublisherState,
};
pub use core_agent_event as event_runtime;
pub use core_agent_event::{
    DeadLetterQueue, DefaultEventLifecycle, DefaultEventRouter, DeliveryState, EmbeddedEventPolicy,
    EventBus, EventCategory, EventCommit, EventDeadLetter, EventDefinition, EventDelivery,
    EventDeliveryContext, EventDispatcher, EventEnvelope, EventError, EventHandler,
    EventInterceptor, EventLifecycle, EventManager, EventManagerBuilder, EventMetadata,
    EventObservation, EventObserver, EventOperation, EventPolicy, EventPolicyDefinition,
    EventPriority, EventRegistry, EventReplay, EventReplayRecord, EventResult, EventRouter,
    EventRuntime, EventSource, EventSourceKind, EventStage, EventState, EventStore,
    EventSubscription, EventVisibility, InMemoryEventBus, InMemoryEventRegistry,
    InMemoryEventStore, LocalEventDispatcher, PublishOutcome, ReplayRequest, ReplayState,
    SqliteEventStore, TypedEventPayload,
};
pub use core_agent_execution as execution_runtime;
pub use core_agent_execution::{
    ActionExecutionStatus, ActionExecutor, AllowAllExecutionPolicy, CheckpointManager,
    CheckpointStore, CommandFailure, CommandKind, CommandResult, DefaultActionExecutor,
    DefaultExecutionLifecycle, DefaultStateMachine, DispatchItem, Dispatcher, ExecuteRequest,
    Execution, ExecutionCheckpoint, ExecutionCommand, ExecutionCommit, ExecutionControl,
    ExecutionEngine, ExecutionError, ExecutionInterceptor, ExecutionLifecycle, ExecutionManager,
    ExecutionManagerBuilder, ExecutionObservation, ExecutionObserver, ExecutionOperation,
    ExecutionPolicy, ExecutionResult, ExecutionRuntime, ExecutionStage, ExecutionStateRecord,
    ExecutionStatus, ExecutionStore, ExplicitRollbackPolicy, ExponentialRetryPolicy,
    InMemoryExecutionStore, RetryManager, RetryPolicy, RetryRecord, RetryStatus, RollbackManager,
    RollbackPolicy, RollbackRecord, RollbackStatus, SequentialDispatcher, SqliteExecutionStore,
    StateMachine,
};
pub use core_agent_extension as extension_runtime;
pub use core_agent_extension::{
    Capability as ExtensionCapability, CapabilityInvocation, CapabilityManifest,
    CapabilityRegistry, CapabilityResult, DefaultExtensionLifecycle, EmbeddedExtensionPolicy,
    Extension, ExtensionError, ExtensionHost, ExtensionInterceptor, ExtensionLifecycle,
    ExtensionLoadHandle, ExtensionLoader, ExtensionManager, ExtensionManagerBuilder,
    ExtensionManifest, ExtensionManifestRecord, ExtensionMetadata, ExtensionObservation,
    ExtensionObserver, ExtensionOperation, ExtensionPermission, ExtensionPolicy,
    ExtensionRegistrationCommit, ExtensionResult, ExtensionRuntime, ExtensionStage, ExtensionState,
    ExtensionStateCommit, ExtensionStateRecord, ExtensionStore, InMemoryExtensionStore,
    InstallExtensionRequest, LocalManifestLoader, Provider as ExtensionProvider,
    ProviderKind as ExtensionProviderKind, ProviderManager, ProviderManifest, SqliteExtensionStore,
    UnavailableExtensionHost,
};
pub use core_agent_governance as governance_runtime;
pub use core_agent_governance::{
    AiAssetType, AssetApproval, AssetEnvironment, CostRecord, DataClassification, EnterpriseError,
    EnterpriseGovernanceManager, EnterprisePrincipal, EnterpriseResult, GovernanceAsset,
    GovernanceAssetState, GovernanceSnapshot, IdentityProviderKind, PrincipalState,
};
pub use core_agent_kernel as runtime_kernel;
pub use core_agent_kernel::{
    ConfigSnapshot as KernelConfigSnapshot, KernelConfig, KernelError, KernelEvent,
    KernelEventKind, KernelEventSink, KernelHook, KernelResult, KernelStatus, LifecycleContext,
    LifecycleOperation, ManagedRuntime, NoopKernelEventSink, RuntimeContext, RuntimeDependency,
    RuntimeDescriptor, RuntimeHealth, RuntimeKernel, RuntimeKernelBuilder, RuntimeStatus,
    RuntimeVersion, ServiceRegistry,
};
pub use core_agent_memory as memory_runtime;
pub use core_agent_memory::{
    DefaultMemoryClassifier, DefaultMemoryIndexer, DefaultMemoryLifecycle, EmbeddedMemoryPolicy,
    InMemoryMemoryStore, Memory, MemoryClassification, MemoryClassifier, MemoryCommit,
    MemoryContent, MemoryError, MemoryEvent, MemoryEventKind, MemoryImportance, MemoryIndexEntry,
    MemoryIndexer, MemoryInterceptor, MemoryKind, MemoryLifecycle, MemoryManager,
    MemoryManagerBuilder, MemoryMetadata, MemoryObservation, MemoryObserver, MemoryOperation,
    MemoryPolicy, MemoryPolicyDefinition, MemoryQuery, MemoryRecallHit, MemoryResult,
    MemoryRetriever, MemoryRuntime, MemorySnapshot, MemorySource, MemorySourceKind, MemoryStage,
    MemoryState, MemoryStore, MemoryType, MemoryUpdate, RememberResult, SqliteMemoryStore,
    StructuredMemoryRetriever,
};
pub use core_agent_model::*;
pub use core_agent_multi as multi_agent_runtime;
pub use core_agent_multi::{
    AgentAvailability as MultiAgentAvailability, AgentDescriptor as MultiAgentDescriptor,
    AgentDirectory as MultiAgentDirectory, AgentDispatcher as MultiAgentDispatcher, AgentMember,
    AgentMessage, AgentRouter as MultiAgentRouter, AssignmentRequest, Collaboration,
    CollaborationBinding, CollaborationCommit, CollaborationOutcome, CollaborationResult,
    CollaborationState, CreateTeamRequest, DeterministicAgentRouter, EmbeddedMultiAgentPolicy,
    EmbeddedTeamLifecycle, InMemoryMultiAgentStore, MemberState, MessagePriority, MultiAgentError,
    MultiAgentInterceptor, MultiAgentManager, MultiAgentManagerBuilder, MultiAgentMetadata,
    MultiAgentObservation, MultiAgentObserver, MultiAgentOperation, MultiAgentPolicy,
    MultiAgentResult, MultiAgentRuntime, MultiAgentStage, MultiAgentStore, Organization,
    Role as MultiAgentRole, RoutingCandidate, SqliteMultiAgentStore, Team, TeamLifecycle,
    TeamPolicyDefinition, TeamState, UnavailableAgentDirectory, UnavailableAgentDispatcher,
};
pub use core_agent_plan as planning_runtime;
pub use core_agent_plan::{
    Action as PlannedAction, ActionDraft, ActionKind, AllowAllPlanningPolicy, CreateGoalRequest,
    CreatePlanRequest, DefaultPlanningLifecycle, DefaultPlanningStrategy, Goal, GoalManager,
    GoalProvider, GoalStatus, InMemoryPlanningCatalog, Intent, Plan, PlanBuilder, PlanDraft,
    PlanError, PlanResult, PlanReview, PlanReviewer, PlanSnapshot, PlanSnapshotStore, PlanStatus,
    PlanningCatalog, PlanningContext, PlanningEdge, PlanningGraph, PlanningInterceptor,
    PlanningLifecycle, PlanningManager, PlanningManagerBuilder, PlanningMetadata, PlanningNode,
    PlanningNodeKind, PlanningObservation, PlanningObserver, PlanningOperation, PlanningPolicy,
    PlanningRelation, PlanningRequestKind, PlanningRuntime, PlanningStage, PlanningStrategy,
    PlanningWorkspaceRef, ReviewDecision, RulePlanBuilder, SqlitePlanningStore, Step as PlanStep,
    StepDraft, StepManager, StructuralPlanReviewer, Task as PlanTask, TaskDraft, TaskManager,
    TaskScheduler, ToolReference, UpdateGoalRequest, UpdatePlanRequest, WorkStatus,
};
pub use core_agent_platform as platform_runtime;
pub use core_agent_platform::{
    AuditDecision, AuditRecord, DeterministicPolicyEngine, EmptyHealthCenter, GovernanceCommit,
    GovernanceDecision, GovernanceRequest, HealthCenter, HealthStatus, InMemoryMetricsCenter,
    InMemoryPlatformStore, MetricPoint, MetricsCenter, PlatformError, PlatformInterceptor,
    PlatformManager, PlatformManagerBuilder, PlatformMetadata, PlatformObserver,
    PlatformOrganization, PlatformPolicy, PlatformPolicyEngine, PlatformResult, PlatformRuntime,
    PlatformState, PlatformStore, PolicyEffect, PolicyRule, Quota, SqlitePlatformStore, Tenant,
    TenantState,
};
pub use core_agent_protocol as protocol_runtime;
pub use core_agent_protocol::{
    AgentSpec, CapabilitySpec, CommandSpec, CompatibilityIssue, CompatibilityReport,
    CompatibilityTestKit, DiscoveryQuery, EventSpec, MarketplaceSpec, MemorySpec, ProtocolDocument,
    ProtocolError, ProtocolKind, ProtocolRegistry, ProtocolResult, ProtocolSpec, ProtocolVersion,
    RegisteredProtocol, ResourceCoordinate, RuntimeSpec, SdkSpec, TraceSpec, UiFieldSpec,
    UiPanelSpec, UiSpec, WorkflowEdge, WorkflowNode as ProtocolWorkflowNode, WorkflowSpec,
};
pub use core_agent_session as session_runtime;
pub use core_agent_session::{
    AppendMessageRequest, Attachment, AttachmentId, AttachmentType, Conversation, ConversationId,
    ConversationResponse, ConversationType, CreateConversationRequest, CreateSessionRequest,
    EventBus as SessionEventBus, JsonSessionSerializer, ListResponse as SessionListResponse,
    Manifest, ManifestId, ManifestResponse, Message, MessageId, MessageResponse, MessageRole,
    MessageStatus, MessageStatusError, Metadata, NoopSessionLifecycle, Session, SessionError,
    SessionEvent, SessionId, SessionLifecycle, SessionObserver, SessionResponse, SessionResult,
    SessionRuntime, SessionSerializer, SessionState, SessionStore, SqliteSessionStore,
    UpdateMessageRequest, UpdateSessionRequest,
};
pub use core_agent_tool as tool_runtime;
pub use core_agent_tool::{
    AllowAllToolPolicy, DefaultToolExecutor, DefaultToolPermission, DefaultToolResultMapper,
    FixedToolPermission, FunctionTool, InMemoryToolCatalog, InMemoryToolLifecycle,
    InMemoryToolRegistry, JsonSchemaToolValidator, NoopToolLifecycle, PermissionDecision,
    RawToolOutput, SqliteToolStore, StaticToolProvider, Tool, ToolAttachment, ToolCapability,
    ToolCatalog, ToolContent, ToolContext as ToolExecutionContext, ToolDefinition, ToolError,
    ToolExecutionRecord, ToolExecutor, ToolFailure, ToolInterceptor, ToolLifecycle,
    ToolLifecycleStatus, ToolManager, ToolManagerBuilder, ToolObservation, ToolObserver,
    ToolPermission, ToolPermissionRule, ToolPermissionStore, ToolPolicy, ToolProvider,
    ToolProviderDefinition, ToolProviderKind, ToolRegistration, ToolRegistry, ToolRequest,
    ToolResult, ToolResultMapper, ToolRuntime, ToolRuntimeResult, ToolStage, ToolUsage,
    ToolValidator,
};
pub use core_agent_visual as visual_runtime;
pub use core_agent_visual::{
    ActionMethod as VisualActionMethod, FieldKind as VisualFieldKind, PanelKind, RegisteredPanel,
    StudioPanelCatalog, VisualAction, VisualDataSource, VisualDescriptor, VisualError, VisualField,
    VisualPanelDescriptor, VisualRegistry, VisualResult,
};
pub use core_agent_workflow as workflow_runtime;
pub use core_agent_workflow::{
    ActionProgress as WorkflowActionProgress, ActivityProgress as WorkflowActivityProgress,
    DefaultWorkflowLifecycle, EmbeddedWorkflowPolicy, InMemoryWorkflowRegistry,
    InMemoryWorkflowStore, InMemoryWorkflowVariableStore, SequentialWorkflowScheduler,
    SqliteWorkflowStore, StageProgress as WorkflowStageProgress, StartWorkflowRequest,
    UnavailableWorkflowEngine, WorkItemState, WorkflowAction, WorkflowActionContext,
    WorkflowActionOutcome, WorkflowActionResult, WorkflowActivity, WorkflowBinding,
    WorkflowControl, WorkflowCursor, WorkflowDefinition, WorkflowDsl, WorkflowEngine,
    WorkflowError, WorkflowIdentity, WorkflowInstance, WorkflowInstanceCommit, WorkflowInterceptor,
    WorkflowLifecycle, WorkflowManager, WorkflowManagerBuilder, WorkflowMetadata,
    WorkflowObservation, WorkflowObserver, WorkflowOperation, WorkflowPolicy,
    WorkflowPolicyDefinition, WorkflowRegistrationCommit, WorkflowRegistry, WorkflowResult,
    WorkflowRuntime, WorkflowScheduler, WorkflowSnapshot, WorkflowSnapshotStore,
    WorkflowStage as WorkflowRuntimeStage, WorkflowStageDefinition, WorkflowState,
    WorkflowStateRecord, WorkflowStore, WorkflowVariableStore, WorkflowVariables,
};
pub use core_agent_workspace as workspace_runtime;
pub use core_agent_workspace::{
    AllowAllWorkspacePolicy, DefaultWorkspaceLifecycle, Environment, EnvironmentDetector,
    EnvironmentManager, GraphEdge, GraphNode, GraphNodeKind, GraphRelation,
    InMemoryWorkspaceCatalog, InMemoryWorkspaceRegistry, LocalEnvironmentDetector,
    LocalProjectScanner, LocalResourceProvider, LocalWorkspaceIndexer, LocalWorkspaceProvider,
    LocalWorkspaceSnapshot, NoopWorkspaceObserver, Project, ProjectKind, ProjectManager,
    ProjectScanner, Resource, ResourceCapability, ResourceManager, ResourceProvider, ResourceType,
    ScanOptions, Snapshot, SnapshotOptions, SqliteWorkspaceStore, Workspace, WorkspaceCatalog,
    WorkspaceError, WorkspaceGraph, WorkspaceIndexer, WorkspaceInterceptor, WorkspaceLifecycle,
    WorkspaceManager, WorkspaceManagerBuilder, WorkspaceObservation, WorkspaceObserver,
    WorkspaceOpenRequest, WorkspaceOperation, WorkspacePolicy, WorkspaceProvider,
    WorkspaceRegistry, WorkspaceResult, WorkspaceRuntime, WorkspaceSearchHit, WorkspaceSnapshot,
    WorkspaceState,
};

/// Cross-Runtime adapters live in the composition crate so lower Runtime crates
/// remain independent and dependency direction stays acyclic.
pub mod integrations {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use core_agent_agent::{AgentGoalRequest, AgentManager, AgentState};
    use core_agent_context::{
        ContextError, ContextProvider, ContextResult, ContextSegment, ContextSlot, ContextSource,
        EnvironmentContext, ProviderContext, TokenCounter, WorkspaceContext,
    };
    use core_agent_event::{
        EventCategory, EventDeliveryContext, EventEnvelope, EventError, EventHandler, EventResult,
        TypedEventPayload,
    };
    use core_agent_execution::{
        ActionExecutor, CommandFailure, CommandKind, CommandResult, DefaultActionExecutor,
        ExecuteRequest, ExecutionCommand, ExecutionControl, ExecutionManager, ExecutionStatus,
    };
    use core_agent_extension::{
        CapabilityInvocation, CapabilityResult, ExtensionHost, ExtensionLoadHandle,
        ExtensionManager, ExtensionResult, Provider as ExtensionProvider,
    };
    use core_agent_kernel::{
        ConfigSnapshot, KernelError, KernelResult, ManagedRuntime, RuntimeContext,
        RuntimeDescriptor, RuntimeHealth, RuntimeVersion,
    };
    use core_agent_memory::{MemoryEvent, MemoryManager, MemoryQuery};
    use core_agent_multi::{
        AgentAvailability, AgentDescriptor, AgentDirectory, AgentDispatcher, AgentMember,
        AgentMessage, Collaboration, CollaborationBinding, CollaborationOutcome,
        CollaborationResult, MultiAgentError, MultiAgentResult,
    };
    use core_agent_plan::{
        Plan, PlanningContext, PlanningRequestKind, PlanningWorkspaceRef, ToolReference,
    };
    use core_agent_platform::{GovernanceRequest, PlatformManager, PlatformState};
    use core_agent_tool::{
        ToolDefinition, ToolLifecycleStatus, ToolManager, ToolPolicy, ToolRequest,
        ToolRuntimeResult,
    };
    use core_agent_workflow::{
        WorkflowAction, WorkflowActionContext, WorkflowActionOutcome, WorkflowActionResult,
        WorkflowBinding, WorkflowControl, WorkflowEngine, WorkflowError, WorkflowResult,
    };
    use core_agent_workspace::Workspace;
    use serde::{Deserialize, Serialize};

    /// Projects a real Kernel runtime into AgentOS Internal Contract 0.1.
    pub fn kernel_runtime_protocol(
        descriptor: &core_agent_kernel::RuntimeDescriptor,
        capabilities: Vec<core_agent_protocol::ResourceCoordinate>,
        events: Vec<core_agent_protocol::ResourceCoordinate>,
        ui: Vec<core_agent_protocol::ResourceCoordinate>,
    ) -> core_agent_protocol::ProtocolDocument {
        let base = format!("/api/protocol/runtimes/{}", descriptor.id);
        core_agent_protocol::ProtocolDocument::new(
            descriptor.id.clone(),
            descriptor.name.clone(),
            descriptor.version.to_string(),
            core_agent_protocol::ProtocolSpec::Runtime(core_agent_protocol::RuntimeSpec {
                lifecycle_endpoint: format!("{base}/lifecycle"),
                health_endpoint: format!("{base}/health"),
                event_endpoint: format!("{base}/events"),
                capabilities,
                events,
                ui,
            }),
        )
    }

    /// Projects the existing safe Visual descriptor into the generic UI protocol.
    pub fn visual_descriptor_protocol(
        descriptor: &core_agent_visual::VisualDescriptor,
    ) -> core_agent_protocol::ProtocolDocument {
        let panels = descriptor
            .panels
            .iter()
            .map(|panel| core_agent_protocol::UiPanelSpec {
                key: panel.key.clone(),
                title: panel.title.clone(),
                panel_type: format!("{:?}", panel.kind).to_ascii_lowercase(),
                data_endpoint: panel.data_source.endpoint.clone(),
                fields: panel
                    .fields
                    .iter()
                    .map(|field| core_agent_protocol::UiFieldSpec {
                        key: field.key.clone(),
                        label: field.label.clone(),
                        value_type: format!("{:?}", field.kind).to_ascii_lowercase(),
                    })
                    .collect(),
            })
            .collect();
        core_agent_protocol::ProtocolDocument::new(
            format!("{}.ui", descriptor.runtime_id),
            format!("{} UI", descriptor.runtime_id),
            descriptor.runtime_version.clone(),
            core_agent_protocol::ProtocolSpec::Ui(core_agent_protocol::UiSpec { panels }),
        )
    }

    /// Projects a Marketplace package manifest without guessing capability versions.
    pub fn marketplace_package_protocol(
        package: &core_agent_ecosystem::MarketplacePackage,
        capability_versions: &BTreeMap<String, String>,
    ) -> core_agent_protocol::ProtocolResult<core_agent_protocol::ProtocolDocument> {
        let required_capabilities = package
            .required_capabilities
            .iter()
            .map(|key| {
                capability_versions
                    .get(key)
                    .map(|version| {
                        core_agent_protocol::ResourceCoordinate::new(
                            core_agent_protocol::ProtocolKind::Capability,
                            key.clone(),
                            version.clone(),
                        )
                    })
                    .ok_or_else(|| {
                        core_agent_protocol::ProtocolError::NotFound(format!(
                            "capability version for {key}"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let dependencies = package
            .dependencies
            .iter()
            .map(|dependency| {
                core_agent_protocol::ResourceCoordinate::new(
                    core_agent_protocol::ProtocolKind::Marketplace,
                    dependency.key.clone(),
                    dependency.version.clone(),
                )
            })
            .collect();
        let package_kind = match package.kind {
            core_agent_ecosystem::PackageKind::Agent => "agent",
            core_agent_ecosystem::PackageKind::Capability => "capability",
            core_agent_ecosystem::PackageKind::Template => "template",
            core_agent_ecosystem::PackageKind::Sdk => "sdk",
        };
        Ok(core_agent_protocol::ProtocolDocument::new(
            package.key.clone(),
            package.name.clone(),
            package.package_version.clone(),
            core_agent_protocol::ProtocolSpec::Marketplace(core_agent_protocol::MarketplaceSpec {
                package_kind: package_kind.into(),
                dependencies,
                required_capabilities,
                content_sha256: package.checksum_sha256.clone(),
            }),
        ))
    }

    /// Compares a governed Marketplace installation plan with the real local
    /// Extension capability inventory without making the catalog own installs.
    pub struct EcosystemExtensionInventory {
        extensions: Arc<ExtensionManager>,
    }

    impl EcosystemExtensionInventory {
        pub fn new(extensions: Arc<ExtensionManager>) -> Self {
            Self { extensions }
        }

        pub async fn missing_capabilities(
            &self,
            plan: &core_agent_ecosystem::InstallationPlan,
        ) -> ExtensionResult<Vec<String>> {
            let mut missing = Vec::new();
            for key in &plan.required_capabilities {
                if self
                    .extensions
                    .find_capability(key)
                    .await?
                    .filter(|item| item.enabled)
                    .is_none()
                {
                    missing.push(key.clone());
                }
            }
            Ok(missing)
        }
    }

    /// Projects P11 Multi-Agent outcomes into the team Activity Stream.
    pub struct MultiAgentProjectActivityObserver {
        manager: Arc<core_agent_collaboration::CollaborationPlatformManager>,
        project_id: uuid::Uuid,
    }

    impl MultiAgentProjectActivityObserver {
        pub fn new(
            manager: Arc<core_agent_collaboration::CollaborationPlatformManager>,
            project_id: uuid::Uuid,
        ) -> Self {
            Self {
                manager,
                project_id,
            }
        }
    }

    impl core_agent_multi::MultiAgentObserver for MultiAgentProjectActivityObserver {
        fn on_observation(&self, observation: &core_agent_multi::MultiAgentObservation) {
            if observation.stage != core_agent_multi::MultiAgentStage::Outcome {
                return;
            }
            let entity_id = observation
                .collaboration_id
                .or(observation.team_id)
                .or(observation.member_id)
                .unwrap_or(self.project_id);
            let event_key = format!(
                "multi-agent:{:?}:{entity_id}:{}",
                observation.operation, observation.success
            );
            let summary = observation.message.clone().unwrap_or_else(|| {
                format!(
                    "Multi-Agent {:?} {}",
                    observation.operation,
                    if observation.success {
                        "completed"
                    } else {
                        "failed"
                    }
                )
            });
            let _ = self.manager.record_external_activity(
                self.project_id,
                &event_key,
                "multi-agent.outcome",
                &observation.actor,
                &summary,
                "collaboration",
                entity_id,
            );
        }
    }

    /// Built-in Platform visual contract. Studio can render these panels
    /// without depending on Platform implementation details.
    pub fn platform_visual_descriptor() -> core_agent_visual::VisualDescriptor {
        use core_agent_visual::{
            FieldKind, PanelKind, VisualDataSource, VisualDescriptor, VisualField,
            VisualPanelDescriptor,
        };
        VisualDescriptor::new(
            "platform",
            "1.0.0",
            vec![
                VisualPanelDescriptor {
                    key: "health".into(),
                    title: "Runtime Health".into(),
                    description: "Platform and provider health checks".into(),
                    icon: Some("activity".into()),
                    kind: PanelKind::Table,
                    data_source: VisualDataSource {
                        endpoint: "/api/platform/health".into(),
                        refresh_seconds: Some(10),
                    },
                    fields: vec![
                        VisualField {
                            key: "component".into(),
                            label: "Component".into(),
                            kind: FieldKind::Text,
                            sortable: true,
                            filterable: true,
                        },
                        VisualField {
                            key: "healthy".into(),
                            label: "Status".into(),
                            kind: FieldKind::Status,
                            sortable: true,
                            filterable: true,
                        },
                    ],
                    actions: Vec::new(),
                },
                VisualPanelDescriptor {
                    key: "audit".into(),
                    title: "Governance Audit".into(),
                    description: "Bounded policy and quota decisions".into(),
                    icon: Some("shield-check".into()),
                    kind: PanelKind::Timeline,
                    data_source: VisualDataSource {
                        endpoint: "/api/platform/audit".into(),
                        refresh_seconds: Some(15),
                    },
                    fields: vec![
                        VisualField {
                            key: "decision".into(),
                            label: "Decision".into(),
                            kind: FieldKind::Status,
                            sortable: true,
                            filterable: true,
                        },
                        VisualField {
                            key: "action".into(),
                            label: "Action".into(),
                            kind: FieldKind::Text,
                            sortable: true,
                            filterable: true,
                        },
                    ],
                    actions: Vec::new(),
                },
            ],
        )
    }

    /// Adapts the real Platform Runtime to the process-local Runtime Kernel.
    pub struct PlatformKernelRuntime {
        manager: Arc<PlatformManager>,
        configuration: Mutex<Option<ConfigSnapshot>>,
    }

    impl PlatformKernelRuntime {
        pub fn new(manager: Arc<PlatformManager>) -> Self {
            Self {
                manager,
                configuration: Mutex::new(None),
            }
        }

        pub fn configuration(&self) -> KernelResult<Option<ConfigSnapshot>> {
            self.configuration
                .lock()
                .map(|value| value.clone())
                .map_err(|_| KernelError::Internal("Platform adapter lock poisoned".into()))
        }
    }

    #[async_trait]
    impl ManagedRuntime for PlatformKernelRuntime {
        fn descriptor(&self) -> RuntimeDescriptor {
            RuntimeDescriptor::new("platform", "Platform Runtime", RuntimeVersion::new(1, 0, 0))
        }

        async fn init(&self, context: &RuntimeContext) -> KernelResult<()> {
            *self
                .configuration
                .lock()
                .map_err(|_| KernelError::Internal("Platform adapter lock poisoned".into()))? =
                Some(context.configuration.clone());
            Ok(())
        }

        async fn start(&self) -> KernelResult<()> {
            self.manager
                .start()
                .map(|_| ())
                .map_err(|error| KernelError::Internal(error.to_string()))
        }

        async fn stop(&self) -> KernelResult<()> {
            self.manager
                .shutdown()
                .map(|_| ())
                .map_err(|error| KernelError::Internal(error.to_string()))
        }

        async fn reload(&self, context: &RuntimeContext) -> KernelResult<()> {
            *self
                .configuration
                .lock()
                .map_err(|_| KernelError::Internal("Platform adapter lock poisoned".into()))? =
                Some(context.configuration.clone());
            Ok(())
        }

        async fn health(&self) -> KernelResult<RuntimeHealth> {
            if self
                .manager
                .status()
                .map_err(|error| KernelError::Internal(error.to_string()))?
                != PlatformState::Running
            {
                return Ok(RuntimeHealth {
                    runtime_id: "platform".into(),
                    healthy: false,
                    message: "Platform Runtime is not Running".into(),
                    checked_at: chrono::Utc::now(),
                });
            }
            let checks = self
                .manager
                .health()
                .await
                .map_err(|error| KernelError::Internal(error.to_string()))?;
            let healthy = checks.iter().all(|check| check.healthy);
            Ok(RuntimeHealth {
                runtime_id: "platform".into(),
                healthy,
                message: if healthy {
                    "healthy".into()
                } else {
                    "one or more Platform health checks failed".into()
                },
                checked_at: chrono::Utc::now(),
            })
        }
    }

    /// Resolves a Tool request into an explicit tenant governance request.
    #[async_trait]
    pub trait ToolGovernanceResolver: Send + Sync {
        async fn resolve(
            &self,
            request: &ToolRequest,
            tool: &ToolDefinition,
        ) -> Result<GovernanceRequest, String>;
    }

    /// Fail-closed Tool policy backed by the Platform governance Runtime.
    pub struct PlatformToolPolicy {
        manager: Arc<PlatformManager>,
        resolver: Arc<dyn ToolGovernanceResolver>,
    }

    impl PlatformToolPolicy {
        pub fn new(
            manager: Arc<PlatformManager>,
            resolver: Arc<dyn ToolGovernanceResolver>,
        ) -> Self {
            Self { manager, resolver }
        }
    }

    #[async_trait]
    impl ToolPolicy for PlatformToolPolicy {
        async fn evaluate(
            &self,
            request: &ToolRequest,
            tool: &ToolDefinition,
        ) -> ToolRuntimeResult<()> {
            let governance = self
                .resolver
                .resolve(request, tool)
                .await
                .map_err(core_agent_tool::ToolError::PolicyDenied)?;
            let decision = self.manager.govern(governance).await.map_err(|error| {
                core_agent_tool::ToolError::PolicyDenied(format!(
                    "Platform governance failed closed: {error}"
                ))
            })?;
            if decision.allowed {
                Ok(())
            } else {
                Err(core_agent_tool::ToolError::PolicyDenied(decision.reason))
            }
        }
    }

    /// Resolves an Extension Provider invocation into an existing Tool request.
    #[async_trait]
    pub trait ExtensionToolResolver: Send + Sync {
        async fn resolve(
            &self,
            provider: &ExtensionProvider,
            invocation: &CapabilityInvocation,
        ) -> Result<ToolRequest, String>;
    }

    /// Extension Host backed by the existing Tool Runtime. This keeps Tool
    /// registration/execution ownership in P3 while P12 resolves capabilities.
    pub struct ToolExtensionHost {
        manager: Arc<ToolManager>,
        resolver: Arc<dyn ExtensionToolResolver>,
    }

    impl ToolExtensionHost {
        pub fn new(manager: Arc<ToolManager>, resolver: Arc<dyn ExtensionToolResolver>) -> Self {
            Self { manager, resolver }
        }
    }

    #[async_trait]
    impl ExtensionHost for ToolExtensionHost {
        async fn start(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
            Ok(())
        }

        async fn stop(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
            Ok(())
        }

        async fn execute(
            &self,
            _handle: &ExtensionLoadHandle,
            provider: &ExtensionProvider,
            invocation: &CapabilityInvocation,
        ) -> ExtensionResult<CapabilityResult> {
            let mut request = self
                .resolver
                .resolve(provider, invocation)
                .await
                .map_err(core_agent_extension::ExtensionError::Host)?;
            request.id = invocation.request_id;
            request
                .metadata
                .insert("extension_provider_id".into(), provider.id.to_string());
            request.metadata.insert(
                "extension_capability".into(),
                invocation.capability_key.clone(),
            );
            let result = self.manager.execute(request).await.map_err(|error| {
                core_agent_extension::ExtensionError::OutcomeUnknown(error.to_string())
            })?;
            if result.status != ToolLifecycleStatus::Success {
                return Err(core_agent_extension::ExtensionError::Host(
                    result
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| format!("Tool ended in {}", result.status.as_str())),
                ));
            }
            Ok(CapabilityResult {
                request_id: invocation.request_id,
                provider_id: provider.id,
                summary: format!(
                    "Tool {} provided {}",
                    result.tool_key, invocation.capability_key
                ),
                output: serde_json::to_value(result.content)
                    .map_err(core_agent_extension::ExtensionError::Serialization)?,
                completed_at: result.completed_at,
            })
        }
    }

    /// Read-only bridge from the durable Agent Runtime into P11 routing.
    pub struct RuntimeAgentDirectory {
        manager: Arc<AgentManager>,
    }

    impl RuntimeAgentDirectory {
        pub fn new(manager: Arc<AgentManager>) -> Self {
            Self { manager }
        }
    }

    #[async_trait]
    impl AgentDirectory for RuntimeAgentDirectory {
        async fn lookup(&self, agent_id: uuid::Uuid) -> MultiAgentResult<Option<AgentDescriptor>> {
            let agent = self
                .manager
                .find(agent_id)
                .await
                .map_err(|error| MultiAgentError::Extension(error.to_string()))?;
            Ok(agent.map(|agent| AgentDescriptor {
                agent_id: agent.id,
                capabilities: agent
                    .profile
                    .capabilities
                    .iter()
                    .map(|value| value.as_str().to_string())
                    .collect(),
                availability: match agent.state {
                    AgentState::Ready | AgentState::Waiting => AgentAvailability::Available,
                    AgentState::Running => AgentAvailability::Busy,
                    _ => AgentAvailability::Offline,
                },
                workspace_id: agent.workspace_id,
            }))
        }
    }

    /// Resolves a typed Team protocol message into the existing Agent Goal API.
    #[async_trait]
    pub trait AgentAssignmentResolver: Send + Sync {
        async fn resolve(
            &self,
            dispatch_id: uuid::Uuid,
            agent_id: uuid::Uuid,
            message: &AgentMessage,
        ) -> Result<AgentGoalRequest, String>;
    }

    /// P11 dispatcher adapter. Multi-Agent owns assignment and Agent Runtime
    /// remains the sole owner of Planning/Execution and single-Agent lifecycle.
    pub struct RuntimeAgentDispatcher {
        manager: Arc<AgentManager>,
        resolver: Arc<dyn AgentAssignmentResolver>,
    }

    impl RuntimeAgentDispatcher {
        pub fn new(manager: Arc<AgentManager>, resolver: Arc<dyn AgentAssignmentResolver>) -> Self {
            Self { manager, resolver }
        }
    }

    #[async_trait]
    impl AgentDispatcher for RuntimeAgentDispatcher {
        async fn prepare(
            &self,
            collaboration: &Collaboration,
            member: &AgentMember,
            _message: &AgentMessage,
        ) -> MultiAgentResult<CollaborationBinding> {
            let agent = self
                .manager
                .find(member.agent_id)
                .await
                .map_err(|error| MultiAgentError::Extension(error.to_string()))?
                .ok_or_else(|| MultiAgentError::not_found(member.agent_id))?;
            if !matches!(agent.state, AgentState::Ready | AgentState::Waiting) {
                return Err(MultiAgentError::InvalidState(format!(
                    "Agent {} is not available for Team dispatch",
                    agent.id
                )));
            }
            Ok(CollaborationBinding {
                dispatch_id: collaboration.dispatch_id(),
                external_id: agent.id,
                external_kind: "agent".into(),
                prepared_at: agent.updated_at,
            })
        }

        async fn execute(
            &self,
            binding: &CollaborationBinding,
            message: &AgentMessage,
        ) -> MultiAgentResult<CollaborationOutcome> {
            if binding.external_kind != "agent" {
                return Err(MultiAgentError::Validation(
                    "Multi-Agent binding is not an Agent binding".into(),
                ));
            }
            let mut request = self
                .resolver
                .resolve(binding.dispatch_id, binding.external_id, message)
                .await
                .map_err(MultiAgentError::Extension)?;
            if request.goal.actor != message.actor {
                return Err(MultiAgentError::Validation(
                    "Agent Goal actor does not match Team message actor".into(),
                ));
            }
            request.goal.metadata.insert(
                "multi_agent_dispatch_id".into(),
                binding.dispatch_id.to_string().into(),
            );
            request.goal.metadata.insert(
                "multi_agent_correlation_id".into(),
                message.correlation_id.to_string().into(),
            );
            let outcome = self
                .manager
                .run_goal(binding.external_id, request)
                .await
                .map_err(|error| MultiAgentError::OutcomeUnknown(error.to_string()))?;
            Ok(match outcome.execution_status {
                ExecutionStatus::Completed => {
                    CollaborationOutcome::Completed(CollaborationResult {
                        summary: format!(
                            "Agent {} completed Goal {} with Execution {}",
                            outcome.agent.id,
                            outcome.reference.goal_id,
                            outcome.reference.execution_id
                        ),
                        external_state: "COMPLETED".into(),
                        completed_at: outcome.agent.updated_at,
                    })
                }
                ExecutionStatus::Waiting | ExecutionStatus::Paused => {
                    CollaborationOutcome::Waiting(format!(
                        "Agent Execution {} is {}",
                        outcome.reference.execution_id,
                        outcome.execution_status.as_str()
                    ))
                }
                ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
                    CollaborationOutcome::Failed(format!(
                        "Agent Execution {} ended in {}",
                        outcome.reference.execution_id,
                        outcome.execution_status.as_str()
                    ))
                }
                status => CollaborationOutcome::OutcomeUnknown(format!(
                    "Agent Execution {} returned non-terminal {}",
                    outcome.reference.execution_id,
                    status.as_str()
                )),
            })
        }
    }

    /// Resolves a business Workflow Action into an approved immutable Plan.
    #[async_trait]
    pub trait WorkflowPlanResolver: Send + Sync {
        async fn resolve(
            &self,
            action: &WorkflowAction,
            context: &WorkflowActionContext,
        ) -> Result<Plan, String>;
    }

    /// Two-phase Workflow Engine adapter. Execution is prepared first so its
    /// durable identity can be committed by Workflow before side effects start.
    pub struct ExecutionWorkflowEngine {
        manager: Arc<ExecutionManager>,
        resolver: Arc<dyn WorkflowPlanResolver>,
    }

    impl ExecutionWorkflowEngine {
        pub fn new(
            manager: Arc<ExecutionManager>,
            resolver: Arc<dyn WorkflowPlanResolver>,
        ) -> Self {
            Self { manager, resolver }
        }

        fn map_error(error: core_agent_execution::ExecutionError) -> WorkflowError {
            if matches!(
                error,
                core_agent_execution::ExecutionError::OutcomeUnknown(_)
            ) {
                WorkflowError::OutcomeUnknown(error.to_string())
            } else {
                WorkflowError::Engine(error.to_string())
            }
        }

        fn map_execution(execution: core_agent_execution::Execution) -> WorkflowActionOutcome {
            match execution.status {
                ExecutionStatus::Completed => {
                    let summary = execution
                        .completed_order
                        .last()
                        .and_then(|id| execution.steps.get(id))
                        .and_then(|step| step.result.as_ref())
                        .map(|result| result.summary.clone())
                        .unwrap_or_else(|| format!("Execution {} completed", execution.id));
                    WorkflowActionOutcome::Completed(WorkflowActionResult {
                        summary,
                        external_state: execution.status.as_str().into(),
                        completed_at: execution.completed_at.unwrap_or(execution.updated_at),
                    })
                }
                ExecutionStatus::Paused => WorkflowActionOutcome::Paused(format!(
                    "Execution {} paused at a safe boundary",
                    execution.id
                )),
                ExecutionStatus::Waiting => {
                    WorkflowActionOutcome::Waiting(format!("Execution {} is waiting", execution.id))
                }
                ExecutionStatus::Failed => {
                    let reason = execution
                        .steps
                        .values()
                        .find_map(|step| step.error.as_ref())
                        .map(|error| error.message.clone())
                        .unwrap_or_else(|| format!("Execution {} failed", execution.id));
                    WorkflowActionOutcome::Failed(reason)
                }
                ExecutionStatus::Cancelled => WorkflowActionOutcome::Cancelled(format!(
                    "Execution {} cancelled",
                    execution.id
                )),
                status => WorkflowActionOutcome::Waiting(format!(
                    "Execution {} remains {}",
                    execution.id,
                    status.as_str()
                )),
            }
        }
    }

    #[async_trait]
    impl WorkflowEngine for ExecutionWorkflowEngine {
        async fn prepare(
            &self,
            action: &WorkflowAction,
            context: &WorkflowActionContext,
        ) -> WorkflowResult<WorkflowBinding> {
            let plan = self
                .resolver
                .resolve(action, context)
                .await
                .map_err(WorkflowError::Engine)?;
            let mut request = ExecuteRequest::new(context.actor.clone());
            request.metadata.insert(
                "workflow_instance_id".into(),
                context.instance_id.to_string(),
            );
            request
                .metadata
                .insert("workflow_action_id".into(), context.action_id.to_string());
            request.metadata.insert(
                "workflow_dispatch_id".into(),
                context.dispatch_id.to_string(),
            );
            let execution = self
                .manager
                .prepare(plan, request)
                .await
                .map_err(Self::map_error)?;
            Ok(WorkflowBinding {
                dispatch_id: context.dispatch_id,
                external_id: execution.id,
                external_kind: "execution".into(),
                prepared_at: execution.created_at,
            })
        }

        async fn execute(
            &self,
            binding: &WorkflowBinding,
            _action: &WorkflowAction,
            context: &WorkflowActionContext,
            control: &WorkflowControl,
        ) -> WorkflowResult<WorkflowActionOutcome> {
            let execution = self
                .manager
                .find(binding.external_id)
                .await
                .map_err(Self::map_error)?
                .ok_or_else(|| WorkflowError::NotFound(binding.external_id.to_string()))?;
            if execution.status.is_terminal() || execution.status == ExecutionStatus::Waiting {
                return Ok(Self::map_execution(execution));
            }
            let lower_control = ExecutionControl::default();
            let future = async {
                if execution.status == ExecutionStatus::Ready {
                    self.manager
                        .start_with_control(binding.external_id, lower_control.clone())
                        .await
                } else {
                    self.manager
                        .resume_with_control(
                            binding.external_id,
                            context.actor.clone(),
                            lower_control.clone(),
                        )
                        .await
                }
            };
            tokio::pin!(future);
            let result = tokio::select! {
                biased;
                result = &mut future => result,
                _ = control.cancelled() => {
                    lower_control.cancel_as(
                        control.cancellation_actor().unwrap_or_else(|| context.actor.clone())
                    );
                    future.await
                }
                _ = control.pause_requested() => {
                    lower_control.request_pause();
                    future.await
                }
            }
            .map_err(Self::map_error)?;
            Ok(Self::map_execution(result))
        }

        async fn cancel(&self, binding: &WorkflowBinding, actor: &str) -> WorkflowResult<bool> {
            let current = self
                .manager
                .find(binding.external_id)
                .await
                .map_err(Self::map_error)?
                .ok_or_else(|| WorkflowError::NotFound(binding.external_id.to_string()))?;
            if current.status == ExecutionStatus::Cancelled {
                return Ok(true);
            }
            if matches!(
                current.status,
                ExecutionStatus::Completed | ExecutionStatus::Failed
            ) {
                return Ok(false);
            }
            let _requested = self
                .manager
                .cancel(binding.external_id, actor)
                .await
                .map_err(Self::map_error)?;
            let execution = self
                .manager
                .find(binding.external_id)
                .await
                .map_err(Self::map_error)?
                .ok_or_else(|| WorkflowError::NotFound(binding.external_id.to_string()))?;
            match execution.status {
                ExecutionStatus::Cancelled => Ok(true),
                ExecutionStatus::Completed | ExecutionStatus::Failed => Ok(false),
                status => Err(WorkflowError::OutcomeUnknown(format!(
                    "Execution {} cancellation is not terminal; current state is {}",
                    execution.id,
                    status.as_str()
                ))),
            }
        }
    }

    /// Typed composition event used to decouple Memory producers from the
    /// concrete Memory Runtime.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MemoryRememberPayload {
        pub event: MemoryEvent,
    }

    impl TypedEventPayload for MemoryRememberPayload {
        const EVENT_TYPE: &'static str = "system.memory.remember";
        const CATEGORY: EventCategory = EventCategory::System;
    }

    /// Local Event handler that applies a typed Memory Event idempotently.
    pub struct MemoryRememberEventHandler {
        manager: Arc<MemoryManager>,
    }

    impl MemoryRememberEventHandler {
        pub fn new(manager: Arc<MemoryManager>) -> Self {
            Self { manager }
        }
    }

    #[async_trait]
    impl EventHandler for MemoryRememberEventHandler {
        async fn handle(
            &self,
            event: &EventEnvelope,
            context: &EventDeliveryContext,
        ) -> EventResult<()> {
            let mut payload = event.decode::<MemoryRememberPayload>()?;
            if payload.event.namespace != event.namespace {
                return Err(EventError::Validation(
                    "Memory Event namespace does not match Event envelope".into(),
                ));
            }
            payload.event.actor = context.actor.clone();
            self.manager
                .remember(payload.event)
                .await
                .map_err(|error| EventError::Handler(error.to_string()))?;
            Ok(())
        }
    }

    /// Recalls a bounded namespace into the existing Context Memory slot.
    /// The adapter stays in the composition crate so both lower runtimes remain
    /// independently reusable.
    pub struct MemoryContextProvider {
        manager: Arc<MemoryManager>,
        namespace: String,
        query: Option<String>,
        actor: String,
        limit: usize,
    }

    impl MemoryContextProvider {
        pub fn new(manager: Arc<MemoryManager>, namespace: impl Into<String>) -> Self {
            Self {
                manager,
                namespace: namespace.into(),
                query: None,
                actor: "context-runtime".into(),
                limit: 10,
            }
        }

        pub fn with_query(mut self, value: impl Into<String>) -> Self {
            self.query = Some(value.into());
            self
        }

        pub fn with_limit(mut self, value: usize) -> Self {
            self.limit = value;
            self
        }

        pub fn with_actor(mut self, value: impl Into<String>) -> Self {
            self.actor = value.into();
            self
        }
    }

    #[async_trait]
    impl ContextProvider for MemoryContextProvider {
        fn name(&self) -> &str {
            "memory-runtime"
        }

        fn source(&self) -> ContextSource {
            ContextSource::Memory
        }

        fn slot(&self) -> ContextSlot {
            ContextSlot::Memory
        }

        async fn collect(&self, context: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
            let mut query = MemoryQuery::new(self.namespace.clone());
            query.text = context
                .extensions
                .get("memory_query")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .or_else(|| self.query.clone());
            query.workspace_id = context
                .extensions
                .get("memory_workspace_id")
                .and_then(|value| value.as_str())
                .map(str::parse)
                .transpose()
                .map_err(|_| {
                    ContextError::InvalidArgument("memory_workspace_id is not a UUID".into())
                })?;
            query.limit = self.limit;
            query.actor = self.actor.clone();
            let hits = self
                .manager
                .recall(query)
                .await
                .map_err(|error| ContextError::Internal(error.to_string()))?;
            if hits.is_empty() {
                return Ok(Vec::new());
            }
            let content = serde_json::json!({
                "namespace": self.namespace,
                "matched": hits.len(),
                "items": hits.iter().map(|hit| serde_json::json!({
                    "id": hit.memory.id,
                    "kind": hit.memory.kind,
                    "type": hit.memory.memory_type,
                    "title": hit.memory.content.title,
                    "body": hit.memory.content.body,
                    "data": hit.memory.content.data,
                    "importance": hit.memory.importance,
                    "confidence": hit.memory.confidence,
                    "tags": hit.memory.tags,
                    "source": hit.memory.source,
                    "reason": hit.memory.reason,
                    "score": hit.score,
                    "matched_by": hit.matched_by,
                })).collect::<Vec<_>>()
            });
            let segment = ContextSegment::new(
                ContextSource::Memory,
                ContextSlot::Memory,
                content.clone(),
                TokenCounter::estimate_json(&content),
                ContextSlot::Memory.default_priority(),
            )
            .with_meta("namespace", self.namespace.clone())
            .with_meta("matched", hits.len().to_string());
            Ok(vec![segment])
        }
    }

    /// Command adapter kept in the composition crate so Execution remains
    /// command-oriented and independent from the concrete Tool Runtime.
    pub struct ToolActionExecutor {
        manager: Arc<ToolManager>,
        builtin: DefaultActionExecutor,
    }

    impl ToolActionExecutor {
        pub fn new(manager: Arc<ToolManager>) -> Self {
            Self {
                manager,
                builtin: DefaultActionExecutor,
            }
        }

        fn map_error(error: core_agent_tool::ToolError) -> CommandFailure {
            if matches!(error, core_agent_tool::ToolError::Cancelled(_)) {
                CommandFailure::cancelled(error.to_string())
            } else {
                CommandFailure::new(error.kind(), error.to_string(), error.is_retryable())
            }
        }
    }

    #[async_trait]
    impl ActionExecutor for ToolActionExecutor {
        async fn execute(
            &self,
            command: &ExecutionCommand,
            control: &ExecutionControl,
        ) -> Result<CommandResult, CommandFailure> {
            if command.kind == CommandKind::Builtin {
                return self.builtin.execute(command, control).await;
            }
            if control.is_cancelled() {
                return Err(CommandFailure::cancelled(
                    "Tool command cancelled before registration",
                ));
            }
            let tool_key = command.tool_key.clone().ok_or_else(|| {
                CommandFailure::new("INVALID_COMMAND", "Tool command has no Tool key", false)
            })?;
            let definition = self
                .manager
                .find(&tool_key)
                .await
                .map_err(Self::map_error)?
                .ok_or_else(|| {
                    CommandFailure::new(
                        "TOOL_NOT_FOUND",
                        format!("Tool {tool_key} not found"),
                        false,
                    )
                })?;
            if let Some(capability) = &command.capability {
                let matches = definition
                    .capabilities
                    .iter()
                    .any(|value| value.as_str() == capability);
                if !matches {
                    return Err(CommandFailure::new(
                        "CAPABILITY_MISMATCH",
                        format!(
                            "Tool {tool_key} no longer provides approved capability {capability}"
                        ),
                        false,
                    ));
                }
            }
            let mut request = ToolRequest::new(tool_key, command.parameters.clone());
            request.id = command.id;
            request
                .metadata
                .insert("execution_id".into(), command.execution_id.to_string());
            request
                .metadata
                .insert("step_id".into(), command.step_id.to_string());
            if let Some(capability) = &command.capability {
                request
                    .metadata
                    .insert("approved_capability".into(), capability.clone());
            }
            if let Some(target_uri) = &command.target_uri {
                request
                    .metadata
                    .insert("approved_target_uri".into(), target_uri.clone());
            }
            let future = self.manager.execute(request);
            tokio::pin!(future);
            let result = tokio::select! {
                biased;
                result = &mut future => result,
                _ = control.cancelled() => {
                    match self.manager.cancel(command.id) {
                        Ok(true) => future.await,
                        Ok(false) => return Err(CommandFailure::cancelled(
                            "Tool command cancelled before in-flight registration",
                        )),
                        Err(error) => return Err(Self::map_error(error)),
                    }
                }
            }
            .map_err(Self::map_error)?;
            if result.status == ToolLifecycleStatus::Success {
                Ok(CommandResult {
                    summary: format!("Tool {} completed", result.tool_key),
                    duration_ms: result.usage.duration_ms,
                    output_bytes: result.usage.output_bytes,
                })
            } else {
                let error = result.error.unwrap_or(core_agent_tool::ToolFailure {
                    kind: "TOOL_FAILED".into(),
                    message: format!("Tool ended in {}", result.status.as_str()),
                    retryable: false,
                });
                if result.status == ToolLifecycleStatus::Cancelled {
                    Err(CommandFailure::cancelled(error.message))
                } else {
                    Err(CommandFailure::new(
                        error.kind,
                        error.message,
                        error.retryable,
                    ))
                }
            }
        }
    }

    /// Creates a bounded Context view: full graph/project/environment metadata,
    /// but no file bodies and no environment variable values.
    pub fn workspace_context(workspace: &Workspace) -> WorkspaceContext {
        WorkspaceContext {
            enabled: workspace.state != core_agent_workspace::WorkspaceState::Closed,
            root_path: workspace
                .local_path()
                .map(|path| path.to_string_lossy().into_owned()),
            content: serde_json::json!({
                "id": workspace.id,
                "name": workspace.name,
                "provider": workspace.provider_key,
                "uri": workspace.uri,
                "state": workspace.state,
                "projects": workspace.projects,
                "environment": workspace.environment,
                "resource_count": workspace.resources.len(),
                "graph": workspace.graph,
                "metadata": workspace.metadata,
            }),
        }
    }

    pub fn environment_context(workspace: &Workspace) -> EnvironmentContext {
        let environment = workspace.environment.as_ref();
        EnvironmentContext {
            os: environment.map(|value| value.os.clone()),
            os_version: None,
            shell: environment.and_then(|value| value.shell.clone()),
            working_directory: workspace
                .local_path()
                .map(|path| path.to_string_lossy().into_owned()),
            git_branch: None,
            git_root: environment
                .and_then(|value| value.git.as_ref())
                .and_then(|_| workspace.local_path())
                .map(|path| path.to_string_lossy().into_owned()),
            extra: serde_json::json!({
                "languages": environment.map(|value| &value.languages),
                "runtimes": environment.map(|value| &value.runtimes),
                "package_managers": environment.map(|value| &value.package_managers),
                "variable_names": environment.map(|value| &value.variable_names),
            }),
        }
    }

    /// Creates a bounded planning view. Tool schemas, file bodies and
    /// environment variable values deliberately stay outside Planning.
    pub fn planning_context(
        workspace: Option<&Workspace>,
        tools: &[ToolDefinition],
        request_kind: PlanningRequestKind,
    ) -> PlanningContext {
        PlanningContext {
            request_kind,
            session_id: None,
            context_id: None,
            workspace: workspace
                .filter(|workspace| {
                    matches!(
                        workspace.state,
                        core_agent_workspace::WorkspaceState::Ready
                            | core_agent_workspace::WorkspaceState::Modified
                            | core_agent_workspace::WorkspaceState::Snapshot
                    )
                })
                .map(|workspace| PlanningWorkspaceRef {
                    id: workspace.id,
                    name: workspace.name.clone(),
                    uri: workspace.uri.clone(),
                    state: workspace.state.as_str().into(),
                    project_count: workspace.projects.len(),
                    resource_count: workspace.resources.len(),
                    graph_node_count: workspace.graph.nodes.len(),
                }),
            tools: tools
                .iter()
                .filter(|tool| tool.enabled)
                .map(|tool| ToolReference {
                    key: tool.key.clone(),
                    name: tool.name.clone(),
                    capabilities: tool
                        .capabilities
                        .iter()
                        .map(|capability| capability.as_str().to_string())
                        .collect(),
                })
                .collect(),
            facts: Default::default(),
        }
    }
}
