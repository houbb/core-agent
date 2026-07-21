use std::collections::HashMap;

use tokio::sync::RwLock;

use crate::domain::*;

/// Manages Todo items — create, update, list, sync with execution.
pub struct TodoManager {
    todos: RwLock<HashMap<uuid::Uuid, Todo>>,
}

impl TodoManager {
    pub fn new() -> Self {
        Self {
            todos: RwLock::default(),
        }
    }

    /// Add a new todo item.
    pub async fn add(&self, todo: Todo) -> TodoResult<()> {
        todo.validate()?;
        let mut guard = self.todos.write().await;
        if guard.contains_key(&todo.id) {
            return Err(TodoError::Validation(format!("todo {} already exists", todo.id)));
        }
        guard.insert(todo.id, todo);
        Ok(())
    }

    /// Add multiple todo items at once.
    pub async fn add_all(&self, items: Vec<Todo>) -> TodoResult<()> {
        for item in items {
            self.add(item).await?;
        }
        Ok(())
    }

    /// Update a todo's status.
    pub async fn update_status(
        &self,
        id: uuid::Uuid,
        status: TodoStatus,
    ) -> TodoResult<()> {
        let mut guard = self.todos.write().await;
        let todo = guard.get_mut(&id).ok_or_else(|| {
            TodoError::NotFound(format!("todo {id}"))
        })?;
        match status {
            TodoStatus::InProgress => todo.mark_in_progress(),
            TodoStatus::Completed => todo.mark_completed(),
            TodoStatus::Cancelled => todo.mark_cancelled(),
            TodoStatus::Pending => {
                todo.status = TodoStatus::Pending;
                todo.updated_at = chrono::Utc::now();
            }
        }
        Ok(())
    }

    /// Get a single todo.
    pub async fn get(&self, id: uuid::Uuid) -> TodoResult<Todo> {
        self.todos
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| TodoError::NotFound(id.to_string()))
    }

    /// List all todos for a session, sorted by order.
    pub async fn list(&self, session_id: uuid::Uuid) -> TodoList {
        let guard = self.todos.read().await;
        let todos: Vec<Todo> = guard
            .values()
            .filter(|t| t.session_id == session_id)
            .cloned()
            .collect();
        TodoList::new(todos)
    }

    /// Remove all todos for a session.
    pub async fn clear(&self, session_id: uuid::Uuid) {
        let mut guard = self.todos.write().await;
        guard.retain(|_, t| t.session_id != session_id);
    }

    /// Generate todos from a list of task names.
    pub async fn from_task_names(
        &self,
        session_id: uuid::Uuid,
        task_names: Vec<&str>,
        start_order: u32,
    ) -> TodoResult<Vec<uuid::Uuid>> {
        let mut ids = Vec::new();
        for (i, name) in task_names.iter().enumerate() {
            let todo = Todo::new(session_id, *name, start_order + i as u32)?;
            let id = todo.id;
            self.add(todo).await?;
            ids.push(id);
        }
        Ok(ids)
    }

    /// Sync todo status from a step ID (task completion).
    pub async fn sync_from_step(
        &self,
        step_id: uuid::Uuid,
        completed: bool,
    ) -> TodoResult<()> {
        let mut guard = self.todos.write().await;
        for todo in guard.values_mut() {
            if todo.step_id == Some(step_id) {
                if completed {
                    todo.mark_completed();
                } else {
                    todo.mark_in_progress();
                }
                return Ok(());
            }
        }
        Err(TodoError::NotFound(format!("todo for step {step_id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_todo_crud() {
        let manager = TodoManager::new();
        let session_id = uuid::Uuid::new_v4();

        let todo = Todo::new(session_id, "Analyze requirements", 1).unwrap();
        let id = todo.id;
        manager.add(todo).await.unwrap();

        let got = manager.get(id).await.unwrap();
        assert_eq!(got.content, "Analyze requirements");
        assert_eq!(got.status, TodoStatus::Pending);

        manager.update_status(id, TodoStatus::Completed).await.unwrap();
        let updated = manager.get(id).await.unwrap();
        assert_eq!(updated.status, TodoStatus::Completed);
    }

    #[tokio::test]
    async fn test_todo_list() {
        let manager = TodoManager::new();
        let session_id = uuid::Uuid::new_v4();

        let ids = manager
            .from_task_names(session_id, vec!["A", "B", "C"], 0)
            .await
            .unwrap();
        assert_eq!(ids.len(), 3);

        let list = manager.list(session_id).await;
        assert_eq!(list.total_count(), 3);
        assert_eq!(list.completed_count(), 0);

        // Complete one
        manager.update_status(ids[0], TodoStatus::Completed).await.unwrap();
        let list = manager.list(session_id).await;
        assert_eq!(list.completed_count(), 1);
    }

    #[test]
    fn test_todo_validation() {
        let session_id = uuid::Uuid::new_v4();
        // Empty content should fail
        let todo = Todo {
            id: uuid::Uuid::new_v4(),
            session_id,
            step_id: None,
            task_id: None,
            order: 0,
            content: "".into(),
            status: TodoStatus::Pending,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        assert!(todo.validate().is_err());
    }
}