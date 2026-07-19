use crate::domain::{Workspace, WorkspaceState};
use crate::error::WorkspaceResult;

use super::{
    WorkspaceLifecycle, WorkspaceObservation, WorkspaceObserver, WorkspaceOperation,
    WorkspacePolicy,
};

#[derive(Default)]
pub struct DefaultWorkspaceLifecycle;

impl WorkspaceLifecycle for DefaultWorkspaceLifecycle {
    fn transition(&self, workspace: &mut Workspace, next: WorkspaceState) -> WorkspaceResult<()> {
        workspace.transition(next)
    }
}

#[derive(Default)]
pub struct AllowAllWorkspacePolicy;

impl WorkspacePolicy for AllowAllWorkspacePolicy {
    fn evaluate(
        &self,
        _operation: WorkspaceOperation,
        _workspace: Option<&Workspace>,
    ) -> WorkspaceResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NoopWorkspaceObserver;

impl WorkspaceObserver for NoopWorkspaceObserver {
    fn observe(&self, _observation: &WorkspaceObservation) {}
}
