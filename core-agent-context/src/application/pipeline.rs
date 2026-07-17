//! ContextPipeline — 上下文处理管道
//!
//! 链式执行：Provider collect → Reducer → Composer → Snapshot。
//! 使用 Builder 模式构建，管道不可变（构建后不再修改步骤列表）。
//!
//! # Example
//!
//! ```ignore
//! let pipeline = ContextPipeline::builder()
//!     .add_provider(SystemProvider::new())
//!     .add_provider(ConversationProvider::new())
//!     .add_reducer(SummaryReducer::new(ReducerConfig::default()))
//!     .add_composer(DefaultComposer::new())
//!     .add_snapshot_store(Arc::new(store))
//!     .build();
//!
//! let context = pipeline.execute(session_id, conversation_id, provider_ctx).await?;
//! ```

use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::domain::context::{Context, ContextSegment, TokenDistribution};
use crate::domain::*;
use crate::error::ContextResult;
use crate::infrastructure::{
    ContextComposer, ContextProvider, ContextReducer, ContextSnapshotStore, ProviderContext,
    ReducerConfig,
};

/// ContextPipeline — 不可变的上下文处理管道
///
/// 管道由四个阶段组成：
/// 1. Collect Phase — 所有 Provider 收集数据
/// 2. Reduce Phase — Reducer 裁剪数据
/// 3. Compose Phase — Composer 组装 Context
/// 4. Snapshot Phase — 持久化快照
pub struct ContextPipeline {
    providers: Vec<Box<dyn ContextProvider>>,
    reducers: Vec<Box<dyn ContextReducer>>,
    composer: Option<Box<dyn ContextComposer>>,
    snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
}

impl ContextPipeline {
    /// 创建 Builder
    pub fn builder() -> ContextPipelineBuilder {
        ContextPipelineBuilder::default()
    }

    /// 执行完整管道
    ///
    /// # Arguments
    /// * `session_id` - Session ID
    /// * `conversation_id` - Conversation ID（可选）
    /// * `provider_ctx` - Provider 执行上下文
    ///
    /// # Returns
    /// 完整的 Context 对象
    pub async fn execute(
        &self,
        session_id: uuid::Uuid,
        conversation_id: Option<uuid::Uuid>,
        provider_ctx: &ProviderContext,
    ) -> ContextResult<Context> {
        // ── Phase 1: Collect ──
        let mut segments = Vec::new();
        for provider in &self.providers {
            if provider.enabled() {
                let result = provider.collect(provider_ctx).await?;
                segments.extend(result);
            }
        }

        // ── Phase 2: Reduce ──
        let mut reduced = segments;
        for reducer in &self.reducers {
            let config = ReducerConfig::default();
            reduced = reducer.reduce(reduced, &config).await?;
        }

        // ── Phase 3: Compose ──
        let context = if let Some(composer) = &self.composer {
            composer
                .compose(session_id, conversation_id, reduced)
                .await?
        } else {
            // 无 Composer 时使用默认构建
            build_default_context(session_id, conversation_id, reduced)?
        };

        // ── Phase 4: Snapshot ──
        if let Some(store) = &self.snapshot_store {
            store.save_snapshot(&context).await?;
        }

        Ok(context)
    }
}

/// 默认 Context 构建（无 Composer 时的 fallback）
fn build_default_context(
    session_id: uuid::Uuid,
    conversation_id: Option<uuid::Uuid>,
    segments: Vec<ContextSegment>,
) -> ContextResult<Context> {
    let now = chrono::Utc::now();
    let id = uuid::Uuid::new_v4();
    let mut dist = TokenDistribution::default();

    let mut system = SystemContext::new("");
    let mut conversation = ConversationContext::new();
    let workspace = WorkspaceContext::new();
    let memory = MemoryContext::new();
    let mut environment = EnvironmentContext::new();
    let plugin = PluginContext::new();
    let mut user = UserContext::new();

    let mut total_tokens = 0u64;

    for seg in &segments {
        total_tokens += seg.token_count;
        match seg.slot {
            ContextSlot::System => {
                dist.system += seg.token_count;
                if let Some(s) = seg.content.as_str() {
                    system.prompt = Some(s.to_string());
                }
            }
            ContextSlot::Conversation => {
                dist.conversation += seg.token_count;
                // 从 JSON content 中提取消息
                if let Some(msg_id) = seg.metadata.get("message_id") {
                    let role = seg
                        .content
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("USER")
                        .to_string();
                    let content = seg
                        .content
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let created_at = seg
                        .content
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let ctx_msg = ContextMessage {
                        id: msg_id.clone(),
                        role,
                        content,
                        token_count: seg.token_count,
                        created_at,
                    };
                    conversation.add_message(ctx_msg);
                }
            }
            ContextSlot::Workspace => dist.workspace += seg.token_count,
            ContextSlot::Memory => dist.memory += seg.token_count,
            ContextSlot::Environment => {
                dist.environment += seg.token_count;
                // 提取环境信息
                if let Some(os) = seg.content.get("os").and_then(|v| v.as_str()) {
                    environment.os = Some(os.to_string());
                }
                if let Some(wd) = seg.content.get("working_directory").and_then(|v| v.as_str()) {
                    environment.working_directory = Some(wd.to_string());
                }
                if let Some(branch) = seg.content.get("git_branch").and_then(|v| v.as_str()) {
                    environment.git_branch = Some(branch.to_string());
                }
                if let Some(root) = seg.content.get("git_root").and_then(|v| v.as_str()) {
                    environment.git_root = Some(root.to_string());
                }
            }
            ContextSlot::Tool => dist.tool += seg.token_count,
            ContextSlot::Plugin => dist.plugin += seg.token_count,
            ContextSlot::User => {
                dist.user += seg.token_count;
                if let Some(input) = seg.content.as_str() {
                    user.current_input = Some(input.to_string());
                }
            }
        }
    }

    // 计算哈希
    let context_json = serde_json::json!({
        "session_id": session_id.to_string(),
        "conversation_id": conversation_id.map(|id| id.to_string()),
        "total_tokens": total_tokens,
        "built_at": now.to_rfc3339(),
    });
    let hash_str = serde_json::to_string(&context_json).unwrap_or_default();
    let hash = hex_encode(Sha256::digest(hash_str.as_bytes()).as_slice());

    Ok(Context {
        id,
        session_id,
        conversation_id,
        system,
        conversation,
        workspace,
        memory,
        environment,
        plugin,
        user,
        total_tokens,
        token_distribution: dist,
        built_at: now,
        hash,
        build_duration_ms: 0,
    })
}

/// 十六进制编码
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect()
}

// ── Builder ──

/// ContextPipelineBuilder
#[derive(Default)]
pub struct ContextPipelineBuilder {
    providers: Vec<Box<dyn ContextProvider>>,
    reducers: Vec<Box<dyn ContextReducer>>,
    composer: Option<Box<dyn ContextComposer>>,
    snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
}

impl ContextPipelineBuilder {
    /// 添加 Provider
    pub fn add_provider<P: ContextProvider + 'static>(mut self, provider: P) -> Self {
        self.providers.push(Box::new(provider));
        self
    }

    /// 添加 Reducer
    pub fn add_reducer<R: ContextReducer + 'static>(mut self, reducer: R) -> Self {
        self.reducers.push(Box::new(reducer));
        self
    }

    /// 添加 Composer
    pub fn add_composer<C: ContextComposer + 'static>(mut self, composer: C) -> Self {
        self.composer = Some(Box::new(composer));
        self
    }

    /// 添加 Snapshot Store
    pub fn add_snapshot_store(mut self, store: Arc<dyn ContextSnapshotStore>) -> Self {
        self.snapshot_store = Some(store);
        self
    }

    /// 构建 ContextPipeline
    pub fn build(self) -> ContextPipeline {
        ContextPipeline {
            providers: self.providers,
            reducers: self.reducers,
            composer: self.composer,
            snapshot_store: self.snapshot_store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::ProviderContext;
    use crate::persistence::providers::SystemProvider;
    use core_agent_session::SqliteSessionStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_pipeline_empty() {
        let pipeline = ContextPipeline::builder().build();

        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store);

        // 即使没有 Provider，也应该生成一个基本 Context
        let context = pipeline.execute(session_id, None, &ctx).await.unwrap();

        assert_eq!(context.session_id, session_id);
        assert!(context.total_tokens == 0);
        assert!(!context.hash.is_empty());
    }

    #[tokio::test]
    async fn test_pipeline_with_providers() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store)
            .with_system_prompt("You are helpful.");

        let pipeline = ContextPipeline::builder()
            .add_provider(SystemProvider::new())
            .build();

        let context = pipeline.execute(session_id, None, &ctx).await.unwrap();

        assert!(context.total_tokens > 0);
        assert!(context.system.prompt.is_some());
    }
}
