//! Domain 层 — Context Runtime 核心实体
//!
//! 定义 Context 体系的所有实体和值对象：
//! Context + ContextSegment + ContextSource + ContextSlot + 7 个子 Context

pub mod context;
pub mod slot;
pub mod system_context;
pub mod conversation_context;
pub mod workspace_context;
pub mod memory_context;
pub mod environment_context;
pub mod plugin_context;
pub mod user_context;

// 重导出核心类型
pub use context::{Context, ContextSegment, ContextSource, TokenDistribution};
pub use slot::{ContextSlot, SlotConfig, TokenCounter};
pub use system_context::SystemContext;
pub use conversation_context::{ContextMessage, ConversationContext};
pub use workspace_context::WorkspaceContext;
pub use memory_context::MemoryContext;
pub use environment_context::EnvironmentContext;
pub use plugin_context::PluginContext;
pub use user_context::UserContext;