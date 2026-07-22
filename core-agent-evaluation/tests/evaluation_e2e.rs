use std::sync::Arc;

use core_agent_evaluation::{
    EvaluationCriteria, EvaluationDimension, EvaluationFeedback, EvaluationManager,
    EvaluationManagerBuilder, EvaluationQuery, InMemoryEvaluationStore, Score,
};
use uuid::Uuid;

#[tokio::test]
async fn evaluation_e2e_full_lifecycle() {
    // Create manager with in-memory store
    let store = Arc::new(InMemoryEvaluationStore::default());
    let manager = EvaluationManager::new(store);

    // Create evaluation
    let agent_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let execution_id = Uuid::new_v4();
    let criteria = vec![
        EvaluationCriteria::new(
            EvaluationDimension::Correctness,
            "result-correctness",
            "Whether the result is factually correct",
            0.5,
        )
        .unwrap(),
        EvaluationCriteria::new(
            EvaluationDimension::Quality,
            "code-quality",
            "Code quality and maintainability",
            0.3,
        )
        .unwrap(),
        EvaluationCriteria::new(
            EvaluationDimension::Safety,
            "safety-check",
            "Whether sensitive info is leaked",
            0.2,
        )
        .unwrap(),
    ];

    let eval = manager
        .create_evaluation(agent_id, task_id, execution_id, criteria, "judge-agent")
        .await
        .unwrap();
    assert_eq!(eval.agent_id, agent_id);
    assert_eq!(eval.total_score.get(), 0);
    assert!(!eval.passed);

    // Record feedback
    let fb = EvaluationFeedback::new(
        EvaluationDimension::Correctness,
        Score::new(100).unwrap(),
        "Perfect correctness",
    );
    let updated = manager.record_feedback(eval.id, fb, "judge").await.unwrap();
    assert_eq!(updated.total_score.get(), 50); // 100 * 0.5 = 50

    // List evaluations
    let evals = manager
        .list(&EvaluationQuery {
            agent_id: Some(agent_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(evals.len(), 1);

    // Snapshot
    let snap = manager.snapshot(agent_id).await.unwrap();
    assert_eq!(snap.total_evaluations, 1);
    assert_eq!(snap.passed_count, 0);
    assert!(snap.average_score > 0.0);
}

#[tokio::test]
async fn evaluation_e2e_multiple_dimensions() {
    let store = Arc::new(InMemoryEvaluationStore::default());
    let manager = EvaluationManager::new(store);

    let criteria = vec![
        EvaluationCriteria::new(
            EvaluationDimension::Correctness,
            "correctness",
            "test",
            0.4,
        )
        .unwrap(),
        EvaluationCriteria::new(
            EvaluationDimension::Quality,
            "quality",
            "test",
            0.3,
        )
        .unwrap(),
        EvaluationCriteria::new(
            EvaluationDimension::Safety,
            "safety",
            "test",
            0.2,
        )
        .unwrap(),
        EvaluationCriteria::new(EvaluationDimension::Cost, "cost", "test", 0.1).unwrap(),
    ];

    let eval = manager
        .create_evaluation(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            criteria,
            "judge",
        )
        .await
        .unwrap();

    // Record all 4 dimensions
    let batch = vec![
        EvaluationFeedback::new(
            EvaluationDimension::Correctness,
            Score::new(90).unwrap(),
            "Good",
        ),
        EvaluationFeedback::new(
            EvaluationDimension::Quality,
            Score::new(80).unwrap(),
            "Decent",
        ),
        EvaluationFeedback::new(
            EvaluationDimension::Safety,
            Score::new(100).unwrap(),
            "Safe",
        ),
        EvaluationFeedback::new(EvaluationDimension::Cost, Score::new(70).unwrap(), "OK"),
    ];
    for fb in batch {
        manager.record_feedback(eval.id, fb, "judge").await.unwrap();
    }

    let final_eval = manager.find(eval.id).await.unwrap().unwrap();
    // 90*0.4 + 80*0.3 + 100*0.2 + 70*0.1 = 36 + 24 + 20 + 7 = 87
    assert_eq!(final_eval.total_score.get(), 87);
}