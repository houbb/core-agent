//! Session 生命周期钩子扩展点。

use async_trait::async_trait;

use crate::domain::{Session, SessionState};
use crate::error::SessionResult;

/// 在状态持久化前后接入审批、审计或自定义生命周期逻辑。
#[async_trait]
pub trait SessionLifecycle: Send + Sync {
    /// 状态转换持久化前执行。返回错误会中止转换。
    async fn before_transition(
        &self,
        _session: &Session,
        _target: SessionState,
    ) -> SessionResult<()> {
        Ok(())
    }

    /// 状态转换成功后执行。
    async fn after_transition(&self, _session: &Session, _previous: SessionState) {}
}

/// 默认无操作生命周期实现。
#[derive(Debug, Default)]
pub struct NoopSessionLifecycle;

#[async_trait]
impl SessionLifecycle for NoopSessionLifecycle {}
