//! core-agent-context — Context Runtime
//!
//! 负责构建 Agent 每一次推理所需要的完整上下文（Context）。
//!
//! Context ≠ Prompt。Context 是结构化的上下文数据，
//! 由多个 ContextSegment 组成，最终可以被 Composer 组装为完整 Context。
//!
//! # Architecture
//!
//! ```text
//! api/            — 公开 API（ContextRuntime）
//! application/    — 用例编排（ContextApplicationService, ContextPipeline, SummaryReducer, DefaultComposer）
//! domain/         — 核心实体（Context, ContextSegment, ContextSlot, ContextSource + 8 个子 Context）
//! infrastructure/ — Provider / Reducer / Composer / Serializer / Snapshot / Cache / Observer 扩展点
//! persistence/    — SQLite 实现（SqliteContextSnapshotStore + 4 个内置 Provider）
//! dto/            — 输入输出 DTO
//! error/          — 统一错误类型
//! ```
//!
//! # Quick Start
//!
//! ```ignore
//! use core_agent_context::{ContextRuntime, BuildContextRequest};
//! use core_agent_session::{SqliteSessionStore, Session, SessionState};
//!
//! let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
//! let runtime = ContextRuntime::new(store, None);
//!
//! let context = runtime.build_context(BuildContextRequest {
//!     session_id: "...".into(),
//!     conversation_id: None,
//!     system_prompt: Some("You are helpful.".into()),
//!     user_input: Some("Hello".into()),
//!     max_messages: Some(20),
//!     max_tokens: Some(128000),
//!     compression_strategy: None,
//!     compression_trigger_percent: None,
//!     working_directory: None,
//! }).await?;
//! ```

pub mod api;
pub mod application;
pub mod domain;
pub mod dto;
pub mod error;
pub mod infrastructure;
pub mod persistence;

// ── 重导出常用类型 ──

// API
pub use api::ContextRuntime;

// DTO
pub use dto::{
    BuildContextRequest, ContextAccessSnapshot, ContextResponse, ContextSnapshotResponse,
    ListResponse, TokenDistributionResponse,
};

// Domain
pub use domain::{
    context::{Context, ContextSegment, ContextSource, TokenDistribution},
    conversation_context::{ContextMessage, ConversationContext},
    environment_context::EnvironmentContext,
    memory_context::MemoryContext,
    plugin_context::PluginContext,
    slot::{ContextSlot, SlotConfig, TokenCounter},
    system_context::SystemContext,
    tool_context::ToolContext,
    user_context::UserContext,
    workspace_context::WorkspaceContext,
};

// Error
pub use error::{ContextError, ContextResult};

// Infrastructure
pub use infrastructure::{
    ContextCache, ContextComposer, ContextObservation, ContextObserver, ContextProvider,
    ContextReducer, ContextSerializer, ContextSnapshotMeta, ContextSnapshotStore, ContextStage,
    JsonContextSerializer, ProviderContext, ReducerConfig,
};

// Persistence
pub use persistence::SqliteContextSnapshotStore;

// Application
pub use application::{ContextPipeline, DefaultComposer, SummaryReducer};
