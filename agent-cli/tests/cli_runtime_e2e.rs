use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use agent_cli::{
    AgentClient, AgentEvent, AgentRequest, Cli, CliApplication, CliConfig, CliError, CliResult,
    EventStream, LocalSessionState, SessionStatus, SessionSummary, SseDecoder, Submission,
    TerminalRenderer,
};
use async_trait::async_trait;
use clap::Parser;
use core_agent::{ConfigManager, UserFileConfigProvider};
use futures_util::stream;
use tempfile::tempdir;
use uuid::Uuid;

struct MockClient {
    session_id: Uuid,
    requests: Mutex<Vec<AgentRequest>>,
    streams: Mutex<VecDeque<Vec<AgentEvent>>>,
}

impl MockClient {
    fn new(session_id: Uuid, streams: Vec<Vec<AgentEvent>>) -> Self {
        Self {
            session_id,
            requests: Mutex::new(Vec::new()),
            streams: Mutex::new(streams.into()),
        }
    }

    fn next_stream(&self) -> EventStream {
        let events = self.streams.lock().unwrap().pop_front().unwrap();
        Box::pin(stream::iter(events.into_iter().map(Ok)))
    }
}

#[async_trait]
impl AgentClient for MockClient {
    async fn send(&self, request: AgentRequest) -> CliResult<Submission> {
        self.requests.lock().unwrap().push(request);
        Ok(Submission {
            session_id: self.session_id,
            accepted: true,
        })
    }

    async fn stream(&self, _session_id: Uuid) -> CliResult<EventStream> {
        Ok(self.next_stream())
    }

    async fn resume(&self, _session_id: Uuid) -> CliResult<EventStream> {
        Ok(self.next_stream())
    }

    async fn cancel(&self, _session_id: Uuid) -> CliResult<bool> {
        Ok(true)
    }

    async fn status(&self, session_id: Uuid) -> CliResult<SessionStatus> {
        Ok(SessionStatus {
            session_id,
            state: "RUNNING".into(),
            model: Some("test".into()),
            memory_items: Some(2),
        })
    }

    async fn sessions(&self) -> CliResult<Vec<SessionSummary>> {
        Ok(vec![SessionSummary {
            session_id: self.session_id,
            state: "RUNNING".into(),
            title: Some("Fix bug".into()),
        }])
    }
}

fn events(message: &str) -> Vec<AgentEvent> {
    vec![
        AgentEvent {
            kind: "tool_started".into(),
            message: "Reading files".into(),
            data: serde_json::Value::Null,
        },
        AgentEvent {
            kind: "execution_finished".into(),
            message: message.into(),
            data: serde_json::Value::Null,
        },
    ]
}

#[test]
fn command_parser_supports_script_and_resume_modes() {
    assert!(Cli::try_parse_from(["agent", "run", "fix login"]).is_ok());
    assert!(Cli::try_parse_from(["agent", "resume"]).is_ok());
    assert!(Cli::try_parse_from(["agent", "run", ""]).is_ok());
    assert!(Cli::try_parse_from(["agent", "unknown"]).is_err());
}

#[test]
fn init_creates_minimal_layout_and_refuses_overwrite() {
    let directory = tempdir().unwrap();
    let config = CliConfig::initialize(directory.path()).unwrap();
    assert_eq!(config.server.url, None);
    assert_eq!(config.permissions.mode, "risk-based");
    assert!(directory.path().join(".agent/config.yaml").is_file());
    assert!(directory.path().join(".agent/context.yaml").is_file());
    assert!(directory.path().join(".agent/memory").is_dir());
    assert!(CliConfig::initialize(directory.path()).is_err());
}

#[test]
fn permission_mode_configuration_is_fail_closed() {
    let mut config = CliConfig::default();
    for mode in ["strict", "risk-based", "auto"] {
        config.permissions.mode = mode.into();
        config.validate().unwrap();
    }
    config.permissions.mode = "unknown".into();
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn global_configuration_opens_an_uninitialized_project_and_redacts_the_key() {
    let user = tempdir().unwrap();
    let workspace = tempdir().unwrap();
    std::fs::write(
        user.path().join("core-agent-config.yaml"),
        "model:\n  apiKey: never-print-this-key\n  name: configured-model\npermissions:\n  mode: strict\n",
    )
    .unwrap();
    let manager = ConfigManager::builder()
        .provider(Arc::new(UserFileConfigProvider::new(user.path())))
        .build()
        .unwrap();

    let config = CliConfig::resolve(workspace.path(), &manager)
        .await
        .unwrap();

    assert_eq!(config.model.name, "configured-model");
    assert_eq!(config.permissions.mode, "strict");
    assert!(!workspace.path().join(".agent/config.yaml").exists());
    assert!(!format!("{config:?}").contains("never-print-this-key"));
    assert!(!config
        .redacted()
        .to_string()
        .contains("never-print-this-key"));
}

#[test]
fn a_new_chat_does_not_silently_reuse_a_previous_run_session() {
    let directory = tempdir().unwrap();
    let previous = Uuid::new_v4();
    let mut state = LocalSessionState::default();
    state.record(directory.path(), previous).unwrap();
    let application = CliApplication::new(
        directory.path(),
        CliConfig::default(),
        Arc::new(MockClient::new(Uuid::new_v4(), Vec::new())),
        TerminalRenderer::new(false),
    );

    application.begin_chat().unwrap();

    let state = LocalSessionState::load(directory.path()).unwrap();
    assert_eq!(state.current_session_id, None);
    assert_eq!(state.recent_session_ids, vec![previous]);
}

#[test]
fn sse_decoder_handles_utf8_and_frames_split_across_chunks() {
    let mut decoder = SseDecoder::default();
    assert!(decoder
        .push(b"event: tool_started\r\ndata: {\"message\":\"")
        .unwrap()
        .is_empty());
    let mut bytes = "读取\"}\r\n\r\nevent: execution_finished\ndata: {\"message\":\"done\"}\n\n"
        .as_bytes()
        .to_vec();
    let split = bytes.split_off(1);
    assert!(decoder.push(&bytes).unwrap().is_empty());
    let frames = decoder.push(&split).unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].clone().into_event().unwrap().message, "读取");
    assert!(frames[1].clone().into_event().unwrap().is_terminal());
}

#[tokio::test]
async fn run_persists_session_and_resume_streams_terminal_result() {
    let directory = tempdir().unwrap();
    let session_id = Uuid::new_v4();
    let client = Arc::new(MockClient::new(
        session_id,
        vec![events("first complete"), events("resume complete")],
    ));
    let application = CliApplication::new(
        directory.path(),
        CliConfig::default(),
        client.clone(),
        TerminalRenderer::new(false),
    );

    let first = application.run("fix login").await.unwrap();
    assert_eq!(first.session_id, Some(session_id));
    assert!(first
        .lines
        .iter()
        .any(|line| line.contains("first complete")));
    assert_eq!(
        LocalSessionState::load(directory.path())
            .unwrap()
            .current_session_id,
        Some(session_id)
    );
    let resumed = application.resume(None).await.unwrap();
    assert!(resumed
        .lines
        .iter()
        .any(|line| line.contains("resume complete")));
    assert_eq!(client.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn premature_stream_end_is_an_error_and_does_not_persist_session() {
    let directory = tempdir().unwrap();
    let client = Arc::new(MockClient::new(
        Uuid::new_v4(),
        vec![vec![AgentEvent {
            kind: "tool_started".into(),
            message: "Reading".into(),
            data: serde_json::Value::Null,
        }]],
    ));
    let application = CliApplication::new(
        directory.path(),
        CliConfig::default(),
        client,
        TerminalRenderer::new(false),
    );
    assert!(matches!(
        application.run("goal").await,
        Err(CliError::Stream(_))
    ));
    assert_eq!(
        LocalSessionState::load(directory.path())
            .unwrap()
            .current_session_id,
        None
    );
}
