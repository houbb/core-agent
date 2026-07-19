//! ContextCache — P1.6 缓存实现的稳定扩展点。

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::Context;
use crate::error::ContextResult;

/// Context 缓存接口。
///
/// P1 MVP 仅定义契约，不提供默认缓存，避免引入一致性策略。
#[async_trait]
pub trait ContextCache: Send + Sync {
    /// 按缓存键读取 Context。
    async fn get(&self, key: &str) -> ContextResult<Option<Context>>;
    /// 写入或替换缓存项。
    async fn put(&self, key: &str, context: &Context) -> ContextResult<()>;
    /// 失效指定 Session 的全部缓存项，返回删除数量。
    async fn invalidate_session(&self, session_id: &Uuid) -> ContextResult<usize>;
}
