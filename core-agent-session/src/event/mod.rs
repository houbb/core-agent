//! 事件系统
//!
//! 第一版就加入完整的事件发布/订阅机制。
//! 以后 Audit / Analytics / Workflow 直接监听，不需要侵入业务代码。

use std::sync::Arc;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use crate::domain::{
    conversation::Conversation, manifest::Manifest, message::Message, session::Session,
};

/// 事件总线 — 基于 tokio::sync::broadcast
///
/// 所有事件通过此总线发布，任何 Runtime 都可以订阅。
pub struct EventBus {
    sender: broadcast::Sender<Arc<SessionEvent>>,
    observers: Arc<RwLock<Vec<Arc<dyn crate::infrastructure::SessionObserver>>>>,
}

impl EventBus {
    /// 创建新的事件总线
    ///
    /// `capacity` 指定缓冲区大小，默认 256。
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            sender,
            observers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// 发布事件
    pub fn publish(&self, event: SessionEvent) {
        let event = Arc::new(event);
        let _ = self.sender.send(event.clone());
        if let Ok(observers) = self.observers.read() {
            for observer in observers.iter() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    observer.on_event(&event);
                }));
            }
        }
    }

    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<SessionEvent>> {
        self.sender.subscribe()
    }

    /// 注册日志、审计或指标观察者。
    pub fn register_observer(&self, observer: Arc<dyn crate::infrastructure::SessionObserver>) {
        if let Ok(mut observers) = self.observers.write() {
            observers.push(observer);
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            observers: self.observers.clone(),
        }
    }
}

/// Session Runtime 事件枚举
#[derive(Debug, Clone)]
pub enum SessionEvent {
    // ── Session 事件 ──
    /// Session 已创建
    SessionCreated {
        session: Session,
        timestamp: DateTime<Utc>,
    },
    /// Session 已更新
    SessionUpdated {
        session: Session,
        timestamp: DateTime<Utc>,
    },
    /// Session 状态变更
    SessionStateChanged {
        session_id: crate::domain::session::SessionId,
        old_state: crate::domain::session::SessionState,
        new_state: crate::domain::session::SessionState,
        timestamp: DateTime<Utc>,
    },
    /// Session 已删除
    SessionDeleted {
        session_id: crate::domain::session::SessionId,
        timestamp: DateTime<Utc>,
    },

    // ── Conversation 事件 ──
    /// Conversation 已创建
    ConversationCreated {
        conversation: Conversation,
        timestamp: DateTime<Utc>,
    },

    // ── Message 事件 ──
    /// Message 已添加
    MessageAdded {
        message: Message,
        timestamp: DateTime<Utc>,
    },
    /// Message 已更新
    MessageUpdated {
        message: Message,
        timestamp: DateTime<Utc>,
    },
    /// Message 已删除
    MessageDeleted {
        message_id: crate::domain::message::MessageId,
        conversation_id: crate::domain::conversation::ConversationId,
        timestamp: DateTime<Utc>,
    },

    // ── Manifest 事件 ──
    /// Manifest 已更新
    ManifestUpdated {
        manifest: Manifest,
        timestamp: DateTime<Utc>,
    },
}

impl SessionEvent {
    /// 事件名称
    pub fn event_name(&self) -> &'static str {
        match self {
            SessionEvent::SessionCreated { .. } => "session.created",
            SessionEvent::SessionUpdated { .. } => "session.updated",
            SessionEvent::SessionStateChanged { .. } => "session.state_changed",
            SessionEvent::SessionDeleted { .. } => "session.deleted",
            SessionEvent::ConversationCreated { .. } => "conversation.created",
            SessionEvent::MessageAdded { .. } => "message.added",
            SessionEvent::MessageUpdated { .. } => "message.updated",
            SessionEvent::MessageDeleted { .. } => "message.deleted",
            SessionEvent::ManifestUpdated { .. } => "manifest.updated",
        }
    }

    /// 事件发生时间
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            SessionEvent::SessionCreated { timestamp, .. }
            | SessionEvent::SessionUpdated { timestamp, .. }
            | SessionEvent::SessionStateChanged { timestamp, .. }
            | SessionEvent::SessionDeleted { timestamp, .. }
            | SessionEvent::ConversationCreated { timestamp, .. }
            | SessionEvent::MessageAdded { timestamp, .. }
            | SessionEvent::MessageUpdated { timestamp, .. }
            | SessionEvent::MessageDeleted { timestamp, .. }
            | SessionEvent::ManifestUpdated { timestamp, .. } => *timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::session::Session;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingObserver(AtomicUsize);

    struct PanickingObserver;

    impl crate::infrastructure::SessionObserver for CountingObserver {
        fn on_event(&self, _event: &SessionEvent) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl crate::infrastructure::SessionObserver for PanickingObserver {
        fn on_event(&self, _event: &SessionEvent) {
            panic!("observer failure");
        }
    }

    #[test]
    fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        let session = Session::new("Test");
        bus.publish(SessionEvent::SessionCreated {
            session: session.clone(),
            timestamp: Utc::now(),
        });

        let received = rx.try_recv().unwrap();
        assert_eq!(received.event_name(), "session.created");
    }

    #[test]
    fn test_event_names() {
        let session = Session::new("Test");
        let event = SessionEvent::SessionCreated {
            session,
            timestamp: Utc::now(),
        };
        assert_eq!(event.event_name(), "session.created");
    }

    #[test]
    fn test_event_observer_receives_event() {
        let bus = EventBus::new(16);
        let observer = Arc::new(CountingObserver(AtomicUsize::new(0)));
        bus.register_observer(observer.clone());

        bus.publish(SessionEvent::SessionCreated {
            session: Session::new("Observed"),
            timestamp: Utc::now(),
        });

        assert_eq!(observer.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_observer_failure_does_not_escape_event_bus() {
        let bus = EventBus::new(0);
        let observer = Arc::new(CountingObserver(AtomicUsize::new(0)));
        bus.register_observer(Arc::new(PanickingObserver));
        bus.register_observer(observer.clone());

        bus.publish(SessionEvent::SessionCreated {
            session: Session::new("Observed"),
            timestamp: Utc::now(),
        });

        assert_eq!(observer.0.load(Ordering::SeqCst), 1);
    }
}
