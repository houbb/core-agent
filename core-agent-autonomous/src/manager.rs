use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{InMemoryAutonomousStore, NoopAutonomousObserver};
use crate::domain::{
    AutonomousGoal, AutonomousLoopState, AutonomousLoopStatus, AutonomousQuery, AutonomousSnapshot,
    AutonomousTrigger, AutonomyLevel, TriggerType, validate_actor,
};
use crate::error::{AutonomousError, AutonomousResult};
use crate::infrastructure::{AutonomousObserver, AutonomousStore};

pub struct AutonomousManagerBuilder {
    store: Arc<dyn AutonomousStore>,
    observers: Vec<Arc<dyn AutonomousObserver>>,
}

impl Default for AutonomousManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryAutonomousStore::default()),
            observers: Vec::new(),
        }
    }
}

impl AutonomousManagerBuilder {
    pub fn store(mut self, value: Arc<dyn AutonomousStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn AutonomousObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> AutonomousManager {
        AutonomousManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct AutonomousManager {
    store: Arc<dyn AutonomousStore>,
    observers: Vec<Arc<dyn AutonomousObserver>>,
}

impl AutonomousManager {
    pub fn builder() -> AutonomousManagerBuilder {
        AutonomousManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn AutonomousStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn create_goal(
        &self,
        agent_id: Uuid,
        description: &str,
        priority: u8,
        level: AutonomyLevel,
        actor: &str,
    ) -> AutonomousResult<AutonomousGoal> {
        let goal = AutonomousGoal::new(agent_id, description, priority, level, actor)?;
        self.store.save_goal(&goal, actor).await?;
        Ok(goal)
    }

    pub async fn start_loop(
        &self,
        agent_id: Uuid,
        level: AutonomyLevel,
        actor: &str,
    ) -> AutonomousResult<AutonomousLoopState> {
        validate_actor(actor)?;
        let mut state = AutonomousLoopState::new(agent_id, level);
        state.status = AutonomousLoopStatus::Observing;
        self.store.save_loop(&state, actor).await?;
        Ok(state)
    }

    pub async fn advance_cycle(
        &self,
        agent_id: Uuid,
        actor: &str,
    ) -> AutonomousResult<AutonomousLoopState> {
        validate_actor(actor)?;
        let mut state = self
            .store
            .find_loop(agent_id)
            .await?
            .ok_or_else(|| AutonomousError::NotFound(agent_id.to_string()))?;

        state.current_cycle += 1;
        state.status = match state.status {
            AutonomousLoopStatus::Observing => AutonomousLoopStatus::Analyzing,
            AutonomousLoopStatus::Analyzing => AutonomousLoopStatus::Planning,
            AutonomousLoopStatus::Planning => AutonomousLoopStatus::Acting,
            AutonomousLoopStatus::Acting => AutonomousLoopStatus::Evaluating,
            AutonomousLoopStatus::Evaluating => AutonomousLoopStatus::Learning,
            AutonomousLoopStatus::Learning => AutonomousLoopStatus::Observing,
            _ => AutonomousLoopStatus::Observing,
        };
        state.updated_at = Utc::now();
        state.version += 1;
        self.store.save_loop(&state, actor).await?;

        for observer in &self.observers {
            observer.on_cycle(agent_id, state.current_cycle, state.status);
        }
        Ok(state)
    }

    pub async fn pause_loop(
        &self,
        agent_id: Uuid,
        actor: &str,
    ) -> AutonomousResult<AutonomousLoopState> {
        validate_actor(actor)?;
        let mut state = self
            .store
            .find_loop(agent_id)
            .await?
            .ok_or_else(|| AutonomousError::NotFound(agent_id.to_string()))?;
        state.status = AutonomousLoopStatus::Idle;
        state.updated_at = Utc::now();
        state.version += 1;
        self.store.save_loop(&state, actor).await?;
        Ok(state)
    }

    pub async fn find_goal(&self, id: Uuid) -> AutonomousResult<Option<AutonomousGoal>> {
        self.store.find_goal(id).await
    }

    pub async fn find_loop(
        &self,
        agent_id: Uuid,
    ) -> AutonomousResult<Option<AutonomousLoopState>> {
        self.store.find_loop(agent_id).await
    }

    pub async fn list_goals(
        &self,
        query: &AutonomousQuery,
    ) -> AutonomousResult<Vec<AutonomousGoal>> {
        self.store.list_goals(query).await
    }

    pub async fn snapshot(&self, agent_id: Uuid) -> AutonomousResult<AutonomousSnapshot> {
        self.store.snapshot(agent_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_goal_and_start_loop() {
        let manager = AutonomousManager::builder().build();
        let agent_id = Uuid::new_v4();

        let goal = manager
            .create_goal(
                agent_id,
                "Keep system stable",
                5,
                AutonomyLevel::L2AutoExecuteLowRisk,
                "system",
            )
            .await
            .unwrap();
        assert_eq!(goal.priority, 5);

        let loop_state = manager
            .start_loop(agent_id, AutonomyLevel::L2AutoExecuteLowRisk, "system")
            .await
            .unwrap();
        assert_eq!(loop_state.status, AutonomousLoopStatus::Observing);
    }

    #[tokio::test]
    async fn cycle_progression() {
        let manager = AutonomousManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager
            .start_loop(agent_id, AutonomyLevel::L1AutoAnalyze, "system")
            .await
            .unwrap();

        let cycled = manager.advance_cycle(agent_id, "system").await.unwrap();
        assert_eq!(cycled.current_cycle, 1);
        assert_eq!(cycled.status, AutonomousLoopStatus::Analyzing);
    }

    #[tokio::test]
    async fn full_cycle_rotation() {
        let manager = AutonomousManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager
            .start_loop(agent_id, AutonomyLevel::L4FullAutonomous, "system")
            .await
            .unwrap();

        // Advance through all 7 statuses
        for expected in [
            AutonomousLoopStatus::Analyzing,
            AutonomousLoopStatus::Planning,
            AutonomousLoopStatus::Acting,
            AutonomousLoopStatus::Evaluating,
            AutonomousLoopStatus::Learning,
            AutonomousLoopStatus::Observing,
        ] {
            let state = manager.advance_cycle(agent_id, "system").await.unwrap();
            assert_eq!(state.status, expected);
        }
    }

    #[tokio::test]
    async fn pause_loop() {
        let manager = AutonomousManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager
            .start_loop(agent_id, AutonomyLevel::L0Suggest, "system")
            .await
            .unwrap();
        let paused = manager.pause_loop(agent_id, "system").await.unwrap();
        assert_eq!(paused.status, AutonomousLoopStatus::Idle);
    }

    #[tokio::test]
    async fn snapshot_works() {
        let manager = AutonomousManager::builder().build();
        let agent_id = Uuid::new_v4();
        manager
            .create_goal(agent_id, "Goal 1", 5, AutonomyLevel::L2AutoExecuteLowRisk, "system")
            .await
            .unwrap();
        manager
            .create_goal(agent_id, "Goal 2", 3, AutonomyLevel::L1AutoAnalyze, "system")
            .await
            .unwrap();
        let snap = manager.snapshot(agent_id).await.unwrap();
        assert_eq!(snap.active_goals, 2);
    }
}