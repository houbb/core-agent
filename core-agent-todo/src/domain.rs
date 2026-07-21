/// User-visible task status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::InProgress => "IN_PROGRESS",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }
}

/// A user-visible task item. Lightweight — does NOT drive execution.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Todo {
    /// Unique identifier
    pub id: uuid::Uuid,
    /// Session this todo belongs to
    pub session_id: uuid::Uuid,
    /// Linked execution step (optional, for tracking)
    pub step_id: Option<uuid::Uuid>,
    /// Linked plan task (optional)
    pub task_id: Option<uuid::Uuid>,
    /// Display order
    pub order: u32,
    /// Human-readable content
    pub content: String,
    /// Current status
    pub status: TodoStatus,
    /// When this todo was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When this todo was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Todo {
    pub fn new(
        session_id: uuid::Uuid,
        content: impl Into<String>,
        order: u32,
    ) -> TodoResult<Self> {
        let now = chrono::Utc::now();
        let todo = Self {
            id: uuid::Uuid::new_v4(),
            session_id,
            step_id: None,
            task_id: None,
            order,
            content: content.into(),
            status: TodoStatus::Pending,
            created_at: now,
            updated_at: now,
        };
        todo.validate()?;
        Ok(todo)
    }

    pub fn validate(&self) -> Result<(), TodoError> {
        if self.content.trim().is_empty() || self.content.len() > 512 {
            return Err(TodoError::Validation(
                "todo content must be 1..=512 chars".into(),
            ));
        }
        Ok(())
    }

    pub fn mark_in_progress(&mut self) {
        self.status = TodoStatus::InProgress;
        self.updated_at = chrono::Utc::now();
    }

    pub fn mark_completed(&mut self) {
        self.status = TodoStatus::Completed;
        self.updated_at = chrono::Utc::now();
    }

    pub fn mark_cancelled(&mut self) {
        self.status = TodoStatus::Cancelled;
        self.updated_at = chrono::Utc::now();
    }
}

/// A list of Todos for display
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TodoList {
    pub todos: Vec<Todo>,
}

impl TodoList {
    pub fn new(todos: Vec<Todo>) -> Self {
        Self { todos }
    }

    pub fn sorted(&self) -> Vec<&Todo> {
        let mut sorted: Vec<&Todo> = self.todos.iter().collect();
        sorted.sort_by_key(|t| t.order);
        sorted
    }

    pub fn completed_count(&self) -> usize {
        self.todos.iter().filter(|t| t.status == TodoStatus::Completed).count()
    }

    pub fn total_count(&self) -> usize {
        self.todos.len()
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TodoError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

pub type TodoResult<T> = Result<T, TodoError>;
