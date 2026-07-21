use std::collections::HashMap;

use tokio::sync::{RwLock, oneshot};

use crate::domain::*;

/// Manages Question lifecycle — create, answer, cancel, and await.
pub struct QuestionManager {
    pending: RwLock<HashMap<uuid::Uuid, PendingQuestion>>,
    history: RwLock<Vec<Question>>,
}

struct PendingQuestion {
    question: Question,
    answer_tx: oneshot::Sender<QuestionResult<String>>,
}

impl QuestionManager {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            history: RwLock::new(Vec::new()),
        }
    }

    /// Ask a question and wait for the user's answer.
    /// Returns the answer string.
    pub async fn ask(&self, question: Question) -> QuestionResult<String> {
        let id = question.id;
        let (tx, rx) = oneshot::channel();
        self.pending.write().await.insert(
            id,
            PendingQuestion {
                question,
                answer_tx: tx,
            },
        );
        rx.await
            .map_err(|_| QuestionError::Runtime("question channel closed".into()))?
    }

    /// Answer a pending question.
    pub async fn answer(&self, id: uuid::Uuid, answer: impl Into<String>) -> QuestionResult<()> {
        let mut pending = self.pending.write().await;
        let mut entry = pending.remove(&id).ok_or_else(|| {
            QuestionError::NotFound(format!("question {id} not found or already answered"))
        })?;
        let answer = answer.into();
        entry.question.answer(answer.clone())?;
        // Record in history
        self.history.write().await.push(entry.question.clone());
        // Send the answer through the channel
        entry
            .answer_tx
            .send(Ok(answer))
            .map_err(|_| QuestionError::Runtime("answer receiver dropped".into()))
    }

    /// Cancel a pending question.
    pub async fn cancel(&self, id: uuid::Uuid) -> QuestionResult<()> {
        let mut pending = self.pending.write().await;
        let mut entry = pending.remove(&id).ok_or_else(|| {
            QuestionError::NotFound(format!("question {id} not found or already answered"))
        })?;
        entry.question.cancel()?;
        self.history.write().await.push(entry.question);
        entry
            .answer_tx
            .send(Err(QuestionError::InvalidState("cancelled".into())))
            .map_err(|_| QuestionError::Runtime("answer receiver dropped".into()))
    }

    /// List all pending questions.
    pub async fn list_pending(&self) -> Vec<Question> {
        self.pending
            .read()
            .await
            .values()
            .map(|entry| entry.question.clone())
            .collect()
    }

    /// Get question history.
    pub async fn list_history(&self, session_id: uuid::Uuid) -> Vec<Question> {
        self.history
            .read()
            .await
            .iter()
            .filter(|q| q.session_id == session_id)
            .cloned()
            .collect()
    }

    /// Check if there are pending questions for a session.
    pub async fn has_pending(&self, session_id: uuid::Uuid) -> bool {
        self.pending
            .read()
            .await
            .values()
            .any(|entry| entry.question.session_id == session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ask_and_answer() {
        let manager = QuestionManager::new();
        let session_id = uuid::Uuid::new_v4();

        let question = Question::new_choice(
            session_id,
            "Which database?",
            vec![
                QuestionOption {
                    label: "MySQL".into(),
                    description: "Relational DB".into(),
                    is_default: true,
                },
                QuestionOption {
                    label: "PostgreSQL".into(),
                    description: "Advanced relational DB".into(),
                    is_default: false,
                },
            ],
        )
        .unwrap();

        let id = question.id;

        let answer_fut = manager.ask(question);
        let answer_it = async {
            // Simulate user answering after a short delay
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            manager.answer(id, "PostgreSQL").await.unwrap();
        };

        let (result, _) = tokio::join!(answer_fut, answer_it);
        assert_eq!(result.unwrap(), "PostgreSQL");
    }

    #[tokio::test]
    async fn test_confirm_question() {
        let manager = QuestionManager::new();
        let session_id = uuid::Uuid::new_v4();
        let question = Question::new_confirm(session_id, "Deploy to production?").unwrap();
        let id = question.id;

        let answer_fut = manager.ask(question);
        let answer_it = async {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            manager.answer(id, "yes").await.unwrap();
        };

        let (result, _) = tokio::join!(answer_fut, answer_it);
        assert_eq!(result.unwrap(), "yes");
    }

    #[test]
    fn test_question_validation() {
        let session_id = uuid::Uuid::new_v4();
        // Choice without options should fail
        let q = Question {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Choice,
            content: "Pick one".into(),
            options: vec![],
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        assert!(q.validate().is_err());

        // Confirm with options should fail
        let q = Question {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Confirm,
            content: "Confirm?".into(),
            options: vec![QuestionOption {
                label: "Yes".into(),
                description: "".into(),
                is_default: true,
            }],
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        assert!(q.validate().is_err());
    }
}