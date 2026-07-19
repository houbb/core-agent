//! Local typed Event Runtime.
//!
//! P9 provides in-process routing with durable audit, replay and dead letters.
//! It is not a distributed broker, queue, CQRS or Event Sourcing runtime.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::{
    DefaultEventLifecycle, DefaultEventRouter, EmbeddedEventPolicy, InMemoryEventBus,
    InMemoryEventRegistry, InMemoryEventStore, LocalEventDispatcher,
};
pub use domain::*;
pub use error::{EventError, EventResult};
pub use infrastructure::*;
pub use manager::{EventManager, EventManagerBuilder};
pub use persistence::SqliteEventStore;

pub type EventRuntime = EventManager;
