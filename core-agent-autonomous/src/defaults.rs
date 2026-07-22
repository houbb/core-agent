use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    AutonomousGoal, AutonomousLoopState, AutonomousLoopStatus, AutonomousQuery,
    AutonomousSnapshot, validate_actor,
};
use crate::error::{AutonomousError, AutonomousResult};
use crate::infrastructure::AutonomousStore;

#[derive(Default)]
pub struct InMemoryAutonomousStore {
    goals: RwLock<Vec<AutonomousGoal>>,
    loops: RwLock<Vec<AutonomousLoopState>>,
}

#[async_trait]
impl AutonomousStore for InMemoryAutonomousStore {
    async fn save_goal(&self, goal: &AutonomousGoal, actor: &str) -> AutonomousResult<()> {
        validate_actor(actor)?;
        goal.validate()?;
        let mut goals = self
            .goals
            .write()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        if let Some(pos) = goals.iter().position(|g| g.id == goal.id) {
            goals[pos] = goal.clone();
            return Ok(());
        }
        goals.push(goal.clone());
        Ok(())
    }

    async fn find_goal(&self, id: Uuid) -> AutonomousResult<Option<AutonomousGoal>> {
        let goals = self
            .goals
            .read()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        Ok(goals.iter().find(|g| g.id == id).cloned())
    }

    async fn save_loop(&self, state: &AutonomousLoopState, actor: &str) -> AutonomousResult<()> {
        validate_actor(actor)?;
        let mut loops = self
            .loops
            .write()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        if let Some(pos) = loops.iter().position(|l| l.agent_id == state.agent_id) {
            loops[pos] = state.clone();
            return Ok(());
        }
        loops.push(state.clone());
        Ok(())
    }

    async fn find_loop(&self, agent_id: Uuid) -> AutonomousResult<Option<AutonomousLoopState>> {
        let loops = self
            .loops
            .read()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        Ok(loops.iter().find(|l| l.agent_id == agent_id).cloned())
    }

    async fn list_goals(&self, query: &AutonomousQuery) -> AutonomousResult<Vec<AutonomousGoal>> {
        let goals = self
            .goals
            .read()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        Ok(goals
            .iter()
            .filter(|g| {
                query.agent_id.map_or(true, |a| g.agent_id == a)
                    && query
                        .autonomy_level
                        .map_or(true, |l| g.autonomy_level == l)
                    && query.active.map_or(true, |a| g.active == a)
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect())
    }

    async fn snapshot(&self, agent_id: Uuid) -> AutonomousResult<AutonomousSnapshot> {
        let goals = self
            .goals
            .read()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;
        let loops = self
            .loops
            .read()
            .map_err(|_| AutonomousError::Internal("lock poisoned".into()))?;

        let active = goals.iter().filter(|g| g.agent_id == agent_id && g.active).count() as u64;
        let loop_state = loops.iter().find(|l| l.agent_id == agent_id);

        Ok(AutonomousSnapshot {
            agent_id,
            total_cycles: loop_state.map_or(0, |l| l.current_cycle),
            current_status: loop_state
                .map_or("IDLE".into(), |l| l.status.as_str().into()),
            autonomy_level: loop_state
                .map_or("L0_SUGGEST".into(), |l| l.autonomy_level.as_str().into()),
            active_goals: active,
        })
    }
}

pub struct NoopAutonomousObserver;

impl crate::infrastructure::AutonomousObserver for NoopAutonomousObserver {
    fn on_cycle(&self, _agent_id: Uuid, _cycle: u64, _status: AutonomousLoopStatus) {}
}