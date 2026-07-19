//! ContextSnapshotStore — Context 快照持久化接口
//!
//! 每次 build() 后保存完整 Context JSON，用于 Replay、Debug、Audit。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::context::Context;
use crate::error::ContextResult;

/// ContextSnapshotMeta — 快照元数据
///
/// 列表查询用，不加载完整 content，减少 IO。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshotMeta {
    /// 快照 ID
    pub id: Uuid,
    /// 所属 Session ID
    pub session_id: Uuid,
    /// 关联 Conversation ID
    pub conversation_id: Option<Uuid>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// Token 总数
    pub token_count: u64,
    /// SHA-256 哈希
    pub hash: String,
}

/// ContextSnapshotStore — 快照存储接口
///
/// 实现：SQLite、PostgreSQL、S3
#[async_trait]
pub trait ContextSnapshotStore: Send + Sync {
    /// 保存快照
    async fn save_snapshot(&self, context: &Context) -> ContextResult<()>;

    /// 加载快照
    async fn load_snapshot(&self, id: &Uuid) -> ContextResult<Option<Context>>;

    /// 列出某 Session 的所有快照
    async fn list_snapshots(
        &self,
        session_id: &Uuid,
        offset: u64,
        limit: u64,
    ) -> ContextResult<(Vec<ContextSnapshotMeta>, u64)>;

    /// 删除单个快照
    async fn delete_snapshot(&self, id: &Uuid) -> ContextResult<()>;

    /// 清理过期快照（保留最近 N 条）
    async fn prune_snapshots(&self, session_id: &Uuid, keep_recent: usize) -> ContextResult<usize>;
}
