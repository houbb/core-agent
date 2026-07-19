//! ContextObserver — Trace、Metrics、Audit 的同步观察扩展点。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Context Pipeline 阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextStage {
    /// Provider 收集完成。
    Collected,
    /// Reducer 裁剪完成。
    Reduced,
    /// Composer 组装完成。
    Composed,
    /// Snapshot 保存完成。
    Snapshotted,
    /// Pipeline 完成。
    Completed,
}

/// 不包含完整 Context 内容的观察事件，避免观察器意外复制敏感数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextObservation {
    /// Session 标识。
    pub session_id: Uuid,
    /// Conversation 标识。
    pub conversation_id: Option<Uuid>,
    /// 当前阶段。
    pub stage: ContextStage,
    /// 当前片段数量。
    pub segment_count: usize,
    /// 当前 Token 总数。
    pub total_tokens: u64,
    /// 从 Pipeline 开始计算的耗时。
    pub duration_ms: u64,
}

/// Pipeline 观察器。
///
/// 实现应快速返回；Pipeline 会隔离观察器 panic，观察失败不会改变构建结果。
pub trait ContextObserver: Send + Sync {
    /// 接收一次观察事件。
    fn on_observation(&self, observation: &ContextObservation);
}
