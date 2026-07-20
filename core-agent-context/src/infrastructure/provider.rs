//! ContextProvider — 上下文提供者 trait
//!
//! 每个 Provider 负责收集一种类型的 Context 数据。
//! Provider 是无状态的纯函数式接口，每次 build() 时创建并执行。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::context_reference::ContextReference;
use crate::domain::slot::ContextSlot;
use crate::error::ContextResult;
use core_agent_session::SessionStore;

/// ProviderContext — Provider 执行上下文
///
/// 包含 Provider 执行所需的全部依赖数据。
/// 设计为不可变快照，避免 Provider 间相互影响。
#[derive(Clone)]
pub struct ProviderContext {
    /// Session ID
    pub session_id: uuid::Uuid,
    /// Conversation ID（可选，若指定则只收集该对话的消息）
    pub conversation_id: Option<uuid::Uuid>,
    /// Session Store 引用（只读访问 Session Runtime 数据）
    pub session_store: Arc<dyn SessionStore>,
    /// 系统提示（可由外部注入）
    pub system_prompt: Option<String>,
    /// 当前工作目录
    pub working_directory: Option<String>,
    /// 最大消息数
    pub max_messages: Option<usize>,
    /// 扩展参数
    pub extensions: HashMap<String, serde_json::Value>,
    /// 上下文引用（Context Annotation）
    pub references: Vec<ContextReference>,
}

impl ProviderContext {
    /// 创建新的 ProviderContext
    pub fn new(session_id: uuid::Uuid, session_store: Arc<dyn SessionStore>) -> Self {
        Self {
            session_id,
            conversation_id: None,
            session_store,
            system_prompt: None,
            working_directory: None,
            max_messages: None,
            extensions: HashMap::new(),
            references: Vec::new(),
        }
    }

    /// 设置 Conversation ID
    pub fn with_conversation(mut self, conversation_id: uuid::Uuid) -> Self {
        self.conversation_id = Some(conversation_id);
        self
    }

    /// 设置系统提示
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// 设置上下文引用
    pub fn with_references(mut self, references: Vec<ContextReference>) -> Self {
        self.references = references;
        self
    }
}

/// ContextProvider — 上下文提供者
///
/// 每个 Provider 实现收集一种类型的上下文数据。
///
/// # Lifecycle
///
/// Provider 在每次 build_context() 调用时创建并执行。
/// 不需要长期持有 Provider 实例。
#[async_trait]
pub trait ContextProvider: Send + Sync {
    /// Provider 名称（用于日志和调试）
    fn name(&self) -> &str;

    /// Provider 负责的 ContextSource
    fn source(&self) -> ContextSource;

    /// Provider 填充的 ContextSlot
    fn slot(&self) -> ContextSlot;

    /// 收集上下文数据
    ///
    /// 返回一个或多个 ContextSegment。返回空 Vec 表示该 Provider 无数据可提供。
    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>>;

    /// 是否启用（可根据配置动态控制）
    fn enabled(&self) -> bool {
        true
    }
}
