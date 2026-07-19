use core_agent_collaboration::{
    CollaborationPlatformError, CollaborationPlatformManager, ProjectRole, ReviewDecision,
    ReviewState, TaskState, TeamProject,
};

#[test]
fn project_task_review_approval_and_activity_form_one_flow() {
    let manager = CollaborationPlatformManager::default();
    let project = manager
        .create_project(TeamProject::new("monolith", "Monolith", "alice"))
        .unwrap();
    manager
        .add_member(project.id, "bob", ProjectRole::Reviewer, "alice")
        .unwrap();
    let task = manager
        .create_task(project.id, "Refactor login", "alice", None, "alice")
        .unwrap();
    let task = manager
        .update_task(task.id, TaskState::Running, 70, None, "alice")
        .unwrap();
    let review = manager
        .request_review(task.id, "bob", "medium", "Authentication diff", "alice")
        .unwrap();
    let approved = manager
        .decide_review(review.id, ReviewDecision::Approve, "Looks good", "bob")
        .unwrap();

    assert_eq!(approved.state, ReviewState::Approved);
    assert_eq!(
        manager.tasks(project.id).unwrap()[0].state,
        TaskState::Completed
    );
    assert_eq!(manager.tasks(project.id).unwrap()[0].progress, 100);
    let activities = manager.activities(project.id).unwrap();
    assert_eq!(
        activities.len(),
        6,
        "activity kinds: {:?}",
        activities
            .iter()
            .map(|item| item.kind.as_str())
            .collect::<Vec<_>>()
    );
    assert!(activities.iter().any(|item| item.kind == "review.decided"));
    // A member only receives events whose audience included them at event time.
    assert_eq!(manager.notifications(project.id, "bob").unwrap().len(), 5);
}

#[test]
fn creator_cannot_self_approve_and_rejection_returns_task_to_running() {
    let manager = CollaborationPlatformManager::default();
    let project = manager
        .create_project(TeamProject::new("monolith", "Monolith", "alice"))
        .unwrap();
    manager
        .add_member(project.id, "bob", ProjectRole::Reviewer, "alice")
        .unwrap();
    let task = manager
        .create_task(project.id, "Fix login", "alice", None, "alice")
        .unwrap();
    manager
        .update_task(task.id, TaskState::Running, 80, None, "alice")
        .unwrap();
    let review = manager
        .request_review(task.id, "bob", "high", "Risky change", "alice")
        .unwrap();
    assert!(matches!(
        manager.decide_review(review.id, ReviewDecision::Approve, "self", "alice"),
        Err(CollaborationPlatformError::Denied(_))
    ));
    let rejected = manager
        .decide_review(
            review.id,
            ReviewDecision::Reject,
            "Add regression test",
            "bob",
        )
        .unwrap();
    assert_eq!(rejected.state, ReviewState::ChangesRequested);
    assert_eq!(
        manager.tasks(project.id).unwrap()[0].state,
        TaskState::Running
    );
}

#[test]
fn external_activity_is_idempotent_and_requires_membership() {
    let manager = CollaborationPlatformManager::default();
    let project = manager
        .create_project(TeamProject::new("monolith", "Monolith", "alice"))
        .unwrap();
    let entity = uuid::Uuid::new_v4();
    manager
        .record_external_activity(
            project.id,
            "agent.completed:1",
            "agent.completed",
            "alice",
            "Coding Agent completed Task #1",
            "agent",
            entity,
        )
        .unwrap();
    assert!(manager
        .record_external_activity(
            project.id,
            "agent.completed:1",
            "agent.completed",
            "alice",
            "duplicate",
            "agent",
            entity,
        )
        .is_err());
    assert!(manager
        .record_external_activity(
            project.id,
            "agent.completed:2",
            "agent.completed",
            "mallory",
            "not a member",
            "agent",
            entity,
        )
        .is_err());
}
