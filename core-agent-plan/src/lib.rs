//! core-agent-plan — provider-neutral Planning Runtime.
//!
//! It turns Goals into reviewable, persistent Plans. It owns planning state,
//! hierarchy and dependency validation, but intentionally does not invoke
//! Models or Tools and does not schedule execution.

mod defaults;
mod domain;
mod error;
mod infrastructure;
mod manager;
mod persistence;

pub use defaults::*;
pub use domain::*;
pub use error::{PlanError, PlanResult};
pub use infrastructure::*;
pub use manager::{GoalManager, PlanningManager, PlanningManagerBuilder, StepManager, TaskManager};
pub use defaults::ExternalPlanBuilder;
pub use persistence::SqlitePlanningStore;

pub type PlanningRuntime = PlanningManager;
