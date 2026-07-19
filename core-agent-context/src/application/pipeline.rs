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

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;

use crate::application::composer::DefaultComposer;
use crate::domain::context::Context;
use crate::domain::slot::{ContextSlot, SlotConfig};
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{
    ContextComposer, ContextObservation, ContextObserver, ContextProvider, ContextReducer,
    ContextSnapshotStore, ContextStage, ProviderContext, ReducerConfig,
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
    reducer_config: ReducerConfig,
    slot_configs: HashMap<ContextSlot, SlotConfig>,
    observers: Vec<Arc<dyn ContextObserver>>,
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
        self.execute_with_config(
            session_id,
            conversation_id,
            provider_ctx,
            &self.reducer_config,
        )
        .await
    }

    /// 使用本次请求的 Reducer 配置执行 Pipeline。
    pub async fn execute_with_config(
        &self,
        session_id: uuid::Uuid,
        conversation_id: Option<uuid::Uuid>,
        provider_ctx: &ProviderContext,
        reducer_config: &ReducerConfig,
    ) -> ContextResult<Context> {
        if provider_ctx.session_id != session_id || provider_ctx.conversation_id != conversation_id
        {
            return Err(ContextError::InvalidArgument(
                "ProviderContext identity does not match Pipeline input".into(),
            ));
        }

        let started_at = Instant::now();
        let effective_config = self.effective_reducer_config(reducer_config);

        // ── Phase 1: Collect ──
        let mut segments = Vec::new();
        for provider in &self.providers {
            if provider.enabled() && self.slot_enabled(provider.slot()) {
                let result = provider.collect(provider_ctx).await?;
                segments.extend(result);
            }
        }
        for segment in &mut segments {
            if let Some(priority) = self
                .slot_configs
                .get(&segment.slot)
                .and_then(|config| config.priority)
            {
                segment.priority = priority;
            }
        }
        segments.retain(|segment| self.slot_enabled(segment.slot));
        self.observe(
            session_id,
            conversation_id,
            ContextStage::Collected,
            segments.len(),
            token_sum(&segments),
            started_at,
        );

        // ── Phase 2: Reduce ──
        let mut reduced = segments;
        for reducer in &self.reducers {
            reduced = reducer.reduce(reduced, &effective_config).await?;
        }
        self.observe(
            session_id,
            conversation_id,
            ContextStage::Reduced,
            reduced.len(),
            token_sum(&reduced),
            started_at,
        );

        // ── Phase 3: Compose ──
        let reduced_count = reduced.len();
        let mut context = if let Some(composer) = &self.composer {
            composer
                .compose(session_id, conversation_id, reduced)
                .await?
        } else {
            DefaultComposer::new()
                .compose(session_id, conversation_id, reduced)
                .await?
        };
        context.build_duration_ms = elapsed_millis(started_at);
        self.observe(
            session_id,
            conversation_id,
            ContextStage::Composed,
            reduced_count,
            context.total_tokens,
            started_at,
        );

        // ── Phase 4: Snapshot ──
        if let Some(store) = &self.snapshot_store {
            store.save_snapshot(&context).await?;
            self.observe(
                session_id,
                conversation_id,
                ContextStage::Snapshotted,
                reduced_count,
                context.total_tokens,
                started_at,
            );
        }
        self.observe(
            session_id,
            conversation_id,
            ContextStage::Completed,
            reduced_count,
            context.total_tokens,
            started_at,
        );

        Ok(context)
    }

    fn effective_reducer_config(&self, requested: &ReducerConfig) -> ReducerConfig {
        let mut effective = requested.clone();
        for config in self.slot_configs.values() {
            if config.token_budget > 0 {
                effective
                    .slot_budgets
                    .insert(config.slot, config.token_budget);
            }
            if config.slot == ContextSlot::Conversation {
                if let Some(max_messages) = config.max_messages {
                    effective.keep_recent_messages = max_messages;
                }
            }
        }
        effective
    }

    fn slot_enabled(&self, slot: ContextSlot) -> bool {
        self.slot_configs
            .get(&slot)
            .map(|config| config.enabled)
            .unwrap_or(true)
    }

    fn observe(
        &self,
        session_id: uuid::Uuid,
        conversation_id: Option<uuid::Uuid>,
        stage: ContextStage,
        segment_count: usize,
        total_tokens: u64,
        started_at: Instant,
    ) {
        let observation = ContextObservation {
            session_id,
            conversation_id,
            stage,
            segment_count,
            total_tokens,
            duration_ms: elapsed_millis(started_at),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

fn elapsed_millis(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn token_sum(segments: &[crate::domain::ContextSegment]) -> u64 {
    segments.iter().fold(0, |total, segment| {
        total.saturating_add(segment.token_count)
    })
}

// ── Builder ──

/// ContextPipelineBuilder
#[derive(Default)]
pub struct ContextPipelineBuilder {
    providers: Vec<Box<dyn ContextProvider>>,
    reducers: Vec<Box<dyn ContextReducer>>,
    composer: Option<Box<dyn ContextComposer>>,
    snapshot_store: Option<Arc<dyn ContextSnapshotStore>>,
    reducer_config: ReducerConfig,
    slot_configs: HashMap<ContextSlot, SlotConfig>,
    observers: Vec<Arc<dyn ContextObserver>>,
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

    /// 设置 Pipeline 默认 Reducer 配置。
    pub fn with_reducer_config(mut self, config: ReducerConfig) -> Self {
        self.reducer_config = config;
        self
    }

    /// 配置单个 Slot 的启用状态、预算和消息上限。
    pub fn configure_slot(mut self, config: SlotConfig) -> Self {
        self.slot_configs.insert(config.slot, config);
        self
    }

    /// 添加观察器。
    pub fn add_observer(mut self, observer: Arc<dyn ContextObserver>) -> Self {
        self.observers.push(observer);
        self
    }

    /// 构建 ContextPipeline
    pub fn build(self) -> ContextPipeline {
        ContextPipeline {
            providers: self.providers,
            reducers: self.reducers,
            composer: self.composer,
            snapshot_store: self.snapshot_store,
            reducer_config: self.reducer_config,
            slot_configs: self.slot_configs,
            observers: self.observers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ContextSegment, ContextSource};
    use crate::infrastructure::ProviderContext;
    use crate::persistence::providers::{SystemProvider, UserProvider};
    use core_agent_session::SqliteSessionStore;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingObserver {
        stages: Mutex<Vec<ContextStage>>,
    }

    impl ContextObserver for RecordingObserver {
        fn on_observation(&self, observation: &ContextObservation) {
            self.stages.lock().unwrap().push(observation.stage);
        }
    }

    struct PanickingObserver;

    impl ContextObserver for PanickingObserver {
        fn on_observation(&self, _observation: &ContextObservation) {
            panic!("observer failure");
        }
    }

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
        let ctx = ProviderContext::new(session_id, store).with_system_prompt("You are helpful.");

        let pipeline = ContextPipeline::builder()
            .add_provider(SystemProvider::new())
            .build();

        let context = pipeline.execute(session_id, None, &ctx).await.unwrap();

        assert!(context.total_tokens > 0);
        assert!(context.system.prompt.is_some());
    }

    #[tokio::test]
    async fn test_pipeline_slot_disable_and_observer_isolation() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let mut provider_context = ProviderContext::new(session_id, store);
        provider_context.extensions.insert(
            "user_input".into(),
            serde_json::Value::String("hidden".into()),
        );
        let observer = Arc::new(RecordingObserver::default());
        let pipeline = ContextPipeline::builder()
            .add_provider(UserProvider::new())
            .configure_slot(SlotConfig::new(ContextSlot::User).disabled())
            .add_observer(observer.clone())
            .add_observer(Arc::new(PanickingObserver))
            .build();

        let context = pipeline
            .execute(session_id, None, &provider_context)
            .await
            .unwrap();

        assert!(context.user.current_input.is_none());
        assert_eq!(context.total_tokens, 0);
        assert_eq!(
            observer.stages.lock().unwrap().as_slice(),
            [
                ContextStage::Collected,
                ContextStage::Reduced,
                ContextStage::Composed,
                ContextStage::Completed,
            ]
        );
    }

    struct WorkspaceProvider;

    #[async_trait::async_trait]
    impl ContextProvider for WorkspaceProvider {
        fn name(&self) -> &str {
            "workspace-test-provider"
        }

        fn source(&self) -> ContextSource {
            ContextSource::Workspace
        }

        fn slot(&self) -> ContextSlot {
            ContextSlot::Workspace
        }

        async fn collect(&self, _ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
            Ok(vec![ContextSegment::new(
                ContextSource::Workspace,
                ContextSlot::Workspace,
                serde_json::json!({"root_path": "/workspace"}),
                4,
                ContextSlot::Workspace.default_priority(),
            )])
        }
    }

    #[tokio::test]
    async fn test_pipeline_accepts_custom_provider() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let provider_context = ProviderContext::new(session_id, store);
        let pipeline = ContextPipeline::builder()
            .add_provider(WorkspaceProvider)
            .add_reducer(crate::application::SummaryReducer)
            .add_composer(DefaultComposer::new())
            .build();

        let context = pipeline
            .execute(session_id, None, &provider_context)
            .await
            .unwrap();

        assert!(context.workspace.enabled);
        assert_eq!(context.workspace.root_path.as_deref(), Some("/workspace"));
    }

    struct PluginProvider;

    #[async_trait::async_trait]
    impl ContextProvider for PluginProvider {
        fn name(&self) -> &str {
            "plugin-test-provider"
        }

        fn source(&self) -> ContextSource {
            ContextSource::Plugin
        }

        fn slot(&self) -> ContextSlot {
            ContextSlot::Plugin
        }

        async fn collect(&self, _ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
            Ok(vec![ContextSegment::new(
                ContextSource::Plugin,
                ContextSlot::Plugin,
                serde_json::json!({"plugin": "test"}),
                4,
                ContextSlot::Plugin.default_priority(),
            )])
        }
    }

    #[tokio::test]
    async fn test_slot_priority_controls_budget_selection() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let provider_context = ProviderContext::new(session_id, store);
        let pipeline = ContextPipeline::builder()
            .add_provider(WorkspaceProvider)
            .add_provider(PluginProvider)
            .add_reducer(crate::application::SummaryReducer)
            .add_composer(DefaultComposer::new())
            .with_reducer_config(ReducerConfig {
                max_total_tokens: 4,
                ..ReducerConfig::default()
            })
            .configure_slot(SlotConfig::new(ContextSlot::Plugin).with_priority(95))
            .build();

        let context = pipeline
            .execute(session_id, None, &provider_context)
            .await
            .unwrap();

        assert!(context.plugin.enabled);
        assert!(!context.workspace.enabled);
    }
}
