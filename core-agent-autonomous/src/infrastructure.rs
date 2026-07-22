use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{AutonomousGoal, AutonomousLoopState, AutonomousLoopStatus, AutonomousQuery, AutonomousSnapshot, AutonomousTrigger};
use crate::error::AutonomousResult;

#[async_trait]
pub trait AutonomousStore: Send + Sync {
    async fn save_goal(&self, goal: &AutonomousGoal, actor: &str) -> AutonomousResult<()>;
    async fn find_goal(&self, id: Uuid) -> AutonomousResult<Option<AutonomousGoal>>;
    async fn save_loop(&self, state: &AutonomousLoopState, actor: &str) -> AutonomousResult<()>;
    async fn find_loop(&self, agent_id: Uuid) -> AutonomousResult<Option<AutonomousLoopState>>;
    async fn list_goals(&self, query: &AutonomousQuery) -> AutonomousResult<Vec<AutonomousGoal>>;
    async fn snapshot(&self, agent_id: Uuid) -> AutonomousResult<AutonomousSnapshot>;
}

pub trait AutonomousObserver: Send + Sync {
    fn on_cycle(&self, agent_id: Uuid, cycle: u64, status: AutonomousLoopStatus);
}