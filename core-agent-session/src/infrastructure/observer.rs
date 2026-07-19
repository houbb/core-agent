//! Session 事件观察者扩展点。

use crate::event::SessionEvent;

/// 用于日志、审计和指标采集的同步观察者。
pub trait SessionObserver: Send + Sync {
    fn on_event(&self, event: &SessionEvent);
}
