//! End-to-end test for P1 intelligence runtime.
//!
//! Covers: Todo creation → Question interaction → Execution → Reflection

use core_agent_question::{Question, QuestionManager, QuestionOption, QuestionType};
use core_agent_reflection::{ReflectionManager, ReflectionRequest};
use core_agent_todo::{Todo, TodoManager, TodoStatus};

#[tokio::test]
async fn p1_todo_lifecycle_e2e() {
    let manager = TodoManager::new();
    let session_id = uuid::Uuid::new_v4();

    // Phase 1: Create todos from a plan
    let task_names = vec![
        "Design OAuth database schema",
        "Implement OAuth callback endpoint",
        "Create frontend login page",
        "Write integration tests",
    ];
    let ids = manager
        .from_task_names(session_id, task_names, 0)
        .await
        .unwrap();
    assert_eq!(ids.len(), 4);

    // Phase 2: Mark in-progress and complete tasks
    manager
        .update_status(ids[0], TodoStatus::InProgress)
        .await
        .unwrap();
    let todo = manager.get(ids[0]).await.unwrap();
    assert_eq!(todo.status, TodoStatus::InProgress);

    manager
        .update_status(ids[0], TodoStatus::Completed)
        .await
        .unwrap();
    let todo = manager.get(ids[0]).await.unwrap();
    assert_eq!(todo.status, TodoStatus::Completed);

    // Phase 3: Verify todo list reflects progress
    let list = manager.list(session_id).await;
    assert_eq!(list.total_count(), 4);
    assert_eq!(list.completed_count(), 1);
}

#[tokio::test]
async fn p1_question_e2e() {
    let manager = QuestionManager::new();
    let session_id = uuid::Uuid::new_v4();

    // Phase 1: Ask a choice question
    let question = Question::new_choice(
        session_id,
        "Which cache solution to use?",
        vec![
            QuestionOption {
                label: "Redis".into(),
                description: "Remote cache, suitable for distributed systems".into(),
                is_default: true,
            },
            QuestionOption {
                label: "Caffeine".into(),
                description: "In-memory cache, fast but not distributed".into(),
                is_default: false,
            },
        ],
    )
    .unwrap();
    let id = question.id;

    let answer_fut = manager.ask(question);
    let answer_it = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        manager.answer(id, "Redis").await.unwrap();
    };

    let (result, _) = tokio::join!(answer_fut, answer_it);
    assert_eq!(result.unwrap(), "Redis");

    // Phase 2: Ask a confirm question
    let confirm = Question::new_confirm(session_id, "Deploy to production?").unwrap();
    let confirm_id = confirm.id;

    let answer_fut = manager.ask(confirm);
    let answer_it = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        manager.answer(confirm_id, "yes").await.unwrap();
    };

    let (result, _) = tokio::join!(answer_fut, answer_it);
    assert_eq!(result.unwrap(), "yes");

    // Phase 3: Cancel a question
    let cancel_q = Question::new_input(session_id, "Enter the database URL").unwrap();
    let cancel_id = cancel_q.id;

    let cancel_fut = manager.ask(cancel_q);
    let cancel_it = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        manager.cancel(cancel_id).await.unwrap();
    };

    let (result, _) = tokio::join!(cancel_fut, cancel_it);
    assert!(result.is_err());
}

#[tokio::test]
async fn p1_reflection_e2e() {
    let manager = ReflectionManager::new();
    let execution_id = uuid::Uuid::new_v4();

    // Phase 1: Evaluate a good result
    let request = ReflectionRequest {
        execution_id,
        result_summary: "Successfully implemented OAuth login with Google provider.\n\
            - Created oauth_tokens table\n\
            - Implemented GET /auth/login endpoint\n\
            - Implemented GET /auth/callback endpoint\n\
            - Added session management\n\
            - Added unit tests and integration tests\n\
            - Updated API documentation"
            .into(),
        goal_description: "Implement OAuth login with Google".into(),
        criteria: vec![
            "correctness".into(),
            "completeness".into(),
            "test_coverage".into(),
        ],
        max_retries: 3,
        min_score_threshold: 70,
    };

    let reflection = manager.evaluate(&request).await.unwrap();
    assert!(reflection.score >= 50);
    assert!(reflection.score <= 100);
    assert!(reflection.criteria.len() == 3);

    // Phase 2: Check threshold
    if ReflectionManager::passes_threshold(&reflection, 70) {
        assert!(reflection.issues.is_empty() || reflection.score >= 70);
    }

    // Phase 3: Verify retry limits
    assert!(ReflectionManager::can_retry(&request, 0));
    assert!(ReflectionManager::can_retry(&request, 1));
    assert!(ReflectionManager::can_retry(&request, 2));
    assert!(!ReflectionManager::can_retry(&request, 3));
}

#[tokio::test]
async fn p1_full_workflow_simulation() {
    // Simulate a full P1 workflow:
    // User goal → Plan → Todos → Execution → Question → Answer → Reflection

    let session_id = uuid::Uuid::new_v4();
    let execution_id = uuid::Uuid::new_v4();

    let todo_mgr = TodoManager::new();
    let question_mgr = QuestionManager::new();
    let reflection_mgr = ReflectionManager::new();

    // Step 1: Create todos from plan tasks
    let plan_tasks = vec!["Analyze OAuth requirements", "Implement OAuth", "Run tests"];
    let todo_ids = todo_mgr
        .from_task_names(session_id, plan_tasks, 0)
        .await
        .unwrap();

    // Step 2: Complete first task
    todo_mgr
        .update_status(todo_ids[0], TodoStatus::Completed)
        .await
        .unwrap();

    // Step 3: Middle of second task, ask user a question
    let question = Question::new_choice(
        session_id,
        "Which OAuth provider to implement first?",
        vec![
            QuestionOption {
                label: "Google".into(),
                description: "Most popular".into(),
                is_default: true,
            },
            QuestionOption {
                label: "GitHub".into(),
                description: "Developer friendly".into(),
                is_default: false,
            },
        ],
    )
    .unwrap();
    let q_id = question.id;

    let answer_fut = question_mgr.ask(question);
    let answer_it = async {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        question_mgr.answer(q_id, "Google").await.unwrap();
    };
    let (answer, _) = tokio::join!(answer_fut, answer_it);
    assert_eq!(answer.unwrap(), "Google");

    // Step 4: Complete remaining tasks
    todo_mgr
        .update_status(todo_ids[1], TodoStatus::Completed)
        .await
        .unwrap();
    todo_mgr
        .update_status(todo_ids[2], TodoStatus::Completed)
        .await
        .unwrap();

    // Step 5: Check todo list
    let list = todo_mgr.list(session_id).await;
    assert_eq!(list.total_count(), 3);
    assert_eq!(list.completed_count(), 3);

    // Step 6: Reflection after execution
    let request = ReflectionRequest {
        execution_id,
        result_summary: "OAuth implementation complete. Google provider integrated, tests passing."
            .into(),
        goal_description: "Implement OAuth login".into(),
        criteria: vec!["correctness".into()],
        max_retries: 1,
        min_score_threshold: 70,
    };
    let reflection = reflection_mgr.evaluate(&request).await.unwrap();
    assert!(reflection.score >= 50);
    assert!(!reflection.criteria.is_empty());
}
