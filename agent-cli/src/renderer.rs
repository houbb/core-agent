use crate::{AgentEvent, SessionStatus, SessionSummary};

pub trait Renderer: Send + Sync {
    fn header(&self, project: &str, model: &str) -> Vec<String>;
    fn event(&self, event: &AgentEvent) -> String;
    fn status(&self, status: &SessionStatus) -> Vec<String>;
    fn sessions(&self, sessions: &[SessionSummary]) -> Vec<String>;
}

pub struct TerminalRenderer {
    color: bool,
}

impl TerminalRenderer {
    pub fn new(color: bool) -> Self {
        Self { color }
    }

    fn gold(&self, value: &str) -> String {
        if self.color {
            format!("\u{1b}[38;5;220m{value}\u{1b}[0m")
        } else {
            value.into()
        }
    }
}

impl Renderer for TerminalRenderer {
    fn header(&self, project: &str, model: &str) -> Vec<String> {
        vec![
            self.gold("AgentOS"),
            format!("Project: {project}"),
            format!("Model: {model}"),
        ]
    }

    fn event(&self, event: &AgentEvent) -> String {
        let label = event.kind.replace('_', " ");
        if event.message.is_empty() {
            self.gold(&label)
        } else {
            format!("{}: {}", self.gold(&label), event.message)
        }
    }

    fn status(&self, status: &SessionStatus) -> Vec<String> {
        vec![
            format!("Session: {}", status.session_id),
            format!("State: {}", status.state),
            format!("Model: {}", status.model.as_deref().unwrap_or("unknown")),
            format!(
                "Memory: {} items",
                status
                    .memory_items
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".into())
            ),
        ]
    }

    fn sessions(&self, sessions: &[SessionSummary]) -> Vec<String> {
        sessions
            .iter()
            .map(|session| {
                format!(
                    "{}  {}  {}",
                    session.session_id,
                    session.state,
                    session.title.as_deref().unwrap_or("")
                )
            })
            .collect()
    }
}
