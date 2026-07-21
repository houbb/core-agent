use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use uuid::Uuid;

use crate::{
    AgentClient, AgentRequest, CliConfig, CliError, CliResult, EventStream, LocalSessionState, Renderer,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub lines: Vec<String>,
    pub session_id: Option<Uuid>,
}

pub struct CliApplication<C: AgentClient + ?Sized, R: Renderer> {
    root: PathBuf,
    config: CliConfig,
    client: Arc<C>,
    renderer: R,
}

impl<C: AgentClient + ?Sized, R: Renderer> CliApplication<C, R> {
    pub fn new(root: impl Into<PathBuf>, config: CliConfig, client: Arc<C>, renderer: R) -> Self {
        Self {
            root: root.into(),
            config,
            client,
            renderer,
        }
    }

    pub fn header(&self) -> Vec<String> {
        self.renderer.header(
            project_name(&self.root),
            self.config.model.provider.as_str(),
        )
    }

    pub async fn run(&self, goal: impl Into<String>) -> CliResult<CommandOutput> {
        self.submit(goal.into(), None).await
    }

    pub async fn chat(&self, goal: impl Into<String>) -> CliResult<CommandOutput> {
        let state = LocalSessionState::load(&self.root)?;
        self.submit(goal.into(), state.current_session_id).await
    }

    /// Stream chat: submit a goal and return the raw EventStream for real-time consumption.
    /// Skips the batch `collect()` step — the caller polls events incrementally.
    pub async fn stream_chat(&self, goal: impl Into<String>) -> CliResult<EventStream> {
        let state = LocalSessionState::load(&self.root)?;
        let workspace = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .to_string_lossy()
            .into_owned();
        let request = AgentRequest {
            session_id: state.current_session_id,
            message: goal.into(),
            workspace,
        };
        request.validate()?;
        let submission = self.client.send(request).await?;
        if !submission.accepted {
            return Err(CliError::Api("Agent API did not accept the goal".into()));
        }
        let mut session_state = LocalSessionState::load(&self.root)?;
        session_state.record(&self.root, submission.session_id)?;
        self.client.stream(submission.session_id).await
    }

    pub fn begin_chat(&self) -> CliResult<()> {
        if !self.config.session.resume_last {
            LocalSessionState::start_new(&self.root)?;
        }
        Ok(())
    }

    pub async fn resume(&self, explicit: Option<Uuid>) -> CliResult<CommandOutput> {
        let mut state = LocalSessionState::load(&self.root)?;
        let session_id = state.resolve(explicit)?;
        let stream = self.client.resume(session_id).await?;
        let lines = self.collect(stream).await?;
        state.record(&self.root, session_id)?;
        Ok(CommandOutput {
            lines,
            session_id: Some(session_id),
        })
    }

    pub async fn cancel(&self, explicit: Option<Uuid>) -> CliResult<CommandOutput> {
        let state = LocalSessionState::load(&self.root)?;
        let session_id = state.resolve(explicit)?;
        let cancelled = self.client.cancel(session_id).await?;
        Ok(CommandOutput {
            lines: vec![if cancelled {
                format!("Cancellation requested for {session_id}")
            } else {
                format!("Session {session_id} is already terminal")
            }],
            session_id: Some(session_id),
        })
    }

    pub async fn status(&self, explicit: Option<Uuid>) -> CliResult<CommandOutput> {
        let state = LocalSessionState::load(&self.root)?;
        let session_id = state.resolve(explicit)?;
        let status = self.client.status(session_id).await?;
        Ok(CommandOutput {
            lines: self.renderer.status(&status),
            session_id: Some(session_id),
        })
    }

    pub async fn sessions(&self) -> CliResult<CommandOutput> {
        let sessions = self.client.sessions().await?;
        Ok(CommandOutput {
            lines: self.renderer.sessions(&sessions),
            session_id: None,
        })
    }

    pub fn config(&self) -> CliResult<CommandOutput> {
        Ok(CommandOutput {
            lines: serde_yaml::to_string(&self.config.redacted())?
                .lines()
                .map(str::to_owned)
                .collect(),
            session_id: None,
        })
    }

    async fn submit(&self, goal: String, session_id: Option<Uuid>) -> CliResult<CommandOutput> {
        let workspace = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone())
            .to_string_lossy()
            .into_owned();
        let request = AgentRequest {
            session_id,
            message: goal,
            workspace,
        };
        request.validate()?;
        let submission = self.client.send(request).await?;
        if !submission.accepted {
            return Err(CliError::Api("Agent API did not accept the goal".into()));
        }
        let stream = self.client.stream(submission.session_id).await?;
        let lines = self.collect(stream).await?;
        let mut state = LocalSessionState::load(&self.root)?;
        state.record(&self.root, submission.session_id)?;
        Ok(CommandOutput {
            lines,
            session_id: Some(submission.session_id),
        })
    }

    async fn collect(&self, mut stream: crate::EventStream) -> CliResult<Vec<String>> {
        let mut lines = Vec::new();
        let mut terminal = false;
        while let Some(event) = stream.next().await {
            let event = event?;
            terminal |= event.is_terminal();
            lines.push(self.renderer.event(&event));
        }
        if !terminal {
            return Err(CliError::Stream(
                "event stream ended before a terminal event".into(),
            ));
        }
        Ok(lines)
    }
}

fn project_name(root: &Path) -> &str {
    root.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("workspace")
}
