use std::sync::Arc;

use core_agent::integrations::MultiAgentProjectActivityObserver;
use core_agent::{
    CollaborationPlatformManager, MultiAgentObservation, MultiAgentObserver, MultiAgentOperation,
    MultiAgentStage, TeamProject,
};

#[test]
fn multi_agent_outcome_projects_into_team_activity_stream() {
    let manager = Arc::new(CollaborationPlatformManager::default());
    let project = manager
        .create_project(TeamProject::new("monolith", "Monolith", "alice"))
        .unwrap();
    let observer = MultiAgentProjectActivityObserver::new(manager.clone(), project.id);
    observer.on_observation(&MultiAgentObservation {
        operation: MultiAgentOperation::Complete,
        stage: MultiAgentStage::Outcome,
        success: true,
        team_id: Some(uuid::Uuid::new_v4()),
        collaboration_id: Some(uuid::Uuid::new_v4()),
        member_id: None,
        actor: "alice".into(),
        message: Some("Coding Agent completed shared task".into()),
    });

    let activities = manager.activities(project.id).unwrap();
    assert_eq!(activities.len(), 2);
    assert_eq!(activities[0].kind, "multi-agent.outcome");
}
