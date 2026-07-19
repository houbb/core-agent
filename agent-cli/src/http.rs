use std::collections::VecDeque;

use async_stream::try_stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    AgentClient, AgentEvent, AgentRequest, CliError, CliResult, EventStream,
    ProfessionalAgentClient, ProfessionalRequest, ProfessionalResponse, ProjectSnapshot,
    SessionStatus, SessionSummary, Submission,
};

pub struct HttpAgentClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpAgentClient {
    pub fn new(base_url: impl Into<String>) -> CliResult<Self> {
        let base_url = base_url.into().trim_end_matches('/').to_owned();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://"))
            || base_url.len() > 2048
        {
            return Err(CliError::Configuration("Agent API URL is invalid".into()));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|error| CliError::Api(error.to_string()))?;
        Ok(Self { base_url, client })
    }

    async fn checked(response: reqwest::Response) -> CliResult<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        Err(CliError::Api(format!(
            "server returned {status}: {}",
            bounded(&body, 1024)
        )))
    }

    async fn event_stream(&self, session_id: Uuid) -> CliResult<EventStream> {
        let response = Self::checked(
            self.client
                .get(format!("{}/api/session/{session_id}/events", self.base_url))
                .header(ACCEPT, "text/event-stream")
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?;
        let mut bytes = response.bytes_stream();
        let output = try_stream! {
            let mut decoder = SseDecoder::default();
            while let Some(chunk) = bytes.next().await {
                let chunk = chunk.map_err(|error| CliError::Stream(error.to_string()))?;
                for frame in decoder.push(&chunk)? {
                    yield frame.into_event()?;
                }
            }
            for frame in decoder.finish()? {
                yield frame.into_event()?;
            }
        };
        Ok(Box::pin(output))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionResponse {
    session_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct CancelResponse {
    cancelled: bool,
}

#[async_trait]
impl AgentClient for HttpAgentClient {
    async fn send(&self, request: AgentRequest) -> CliResult<Submission> {
        request.validate()?;
        let session_id = if let Some(session_id) = request.session_id {
            session_id
        } else {
            Self::checked(
                self.client
                    .post(format!("{}/api/session", self.base_url))
                    .json(&json!({"workspace": request.workspace}))
                    .send()
                    .await
                    .map_err(|error| CliError::Api(error.to_string()))?,
            )
            .await?
            .json::<CreateSessionResponse>()
            .await
            .map_err(|error| CliError::Api(format!("invalid session response: {error}")))?
            .session_id
        };
        Self::checked(
            self.client
                .post(format!(
                    "{}/api/session/{session_id}/message",
                    self.base_url
                ))
                .json(&json!({
                    "sessionId": session_id,
                    "message": request.message,
                    "workspace": request.workspace,
                }))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?;
        Ok(Submission {
            session_id,
            accepted: true,
        })
    }

    async fn stream(&self, session_id: Uuid) -> CliResult<EventStream> {
        self.event_stream(session_id).await
    }

    async fn resume(&self, session_id: Uuid) -> CliResult<EventStream> {
        Self::checked(
            self.client
                .post(format!("{}/api/session/{session_id}/resume", self.base_url))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?;
        self.event_stream(session_id).await
    }

    async fn cancel(&self, session_id: Uuid) -> CliResult<bool> {
        Ok(Self::checked(
            self.client
                .post(format!("{}/api/session/{session_id}/cancel", self.base_url))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?
        .json::<CancelResponse>()
        .await
        .map_err(|error| CliError::Api(format!("invalid cancel response: {error}")))?
        .cancelled)
    }

    async fn status(&self, session_id: Uuid) -> CliResult<SessionStatus> {
        Self::checked(
            self.client
                .get(format!("{}/api/session/{session_id}", self.base_url))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?
        .json()
        .await
        .map_err(|error| CliError::Api(format!("invalid status response: {error}")))
    }

    async fn sessions(&self) -> CliResult<Vec<SessionSummary>> {
        Self::checked(
            self.client
                .get(format!("{}/api/sessions", self.base_url))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?
        .json()
        .await
        .map_err(|error| CliError::Api(format!("invalid sessions response: {error}")))
    }
}

#[async_trait]
impl ProfessionalAgentClient for HttpAgentClient {
    async fn index_project(
        &self,
        project: ProjectSnapshot,
        profile: &str,
    ) -> CliResult<ProfessionalResponse> {
        Self::checked(
            self.client
                .post(format!("{}/api/project/index", self.base_url))
                .json(&json!({"project": project, "profile": profile}))
                .send()
                .await
                .map_err(|error| CliError::Api(error.to_string()))?,
        )
        .await?
        .json()
        .await
        .map_err(|error| CliError::Api(format!("invalid project response: {error}")))
    }

    async fn execute_professional(
        &self,
        request: ProfessionalRequest,
    ) -> CliResult<ProfessionalResponse> {
        let name = request.invocation.name.as_str();
        let response = match name {
            "review" => {
                self.client
                    .post(format!("{}/api/project/review", self.base_url))
                    .json(&request)
                    .send()
                    .await
            }
            "history" => {
                let query = request.invocation.arguments.join(" ");
                self.client
                    .get(format!("{}/api/project/history", self.base_url))
                    .query(&[
                        ("profile", request.profile.as_str()),
                        ("workspace", request.project.root.as_str()),
                        ("query", query.as_str()),
                    ])
                    .send()
                    .await
            }
            "memory" => {
                self.client
                    .get(format!("{}/api/project/memory", self.base_url))
                    .query(&[
                        ("profile", request.profile.as_str()),
                        ("workspace", request.project.root.as_str()),
                    ])
                    .send()
                    .await
            }
            "tasks" | "tools" => {
                self.client
                    .get(format!("{}/api/{name}", self.base_url))
                    .query(&[("workspace", request.project.root.as_str())])
                    .send()
                    .await
            }
            _ => {
                self.client
                    .post(format!("{}/api/command/{name}", self.base_url))
                    .json(&request)
                    .send()
                    .await
            }
        }
        .map_err(|error| CliError::Api(error.to_string()))?;
        Self::checked(response)
            .await?
            .json()
            .await
            .map_err(|error| CliError::Api(format!("invalid command response: {error}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: Option<String>,
    pub id: Option<String>,
    pub data: String,
}

impl SseFrame {
    pub fn into_event(self) -> CliResult<AgentEvent> {
        let data = if self.data.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&self.data).unwrap_or_else(|_| Value::String(self.data.clone()))
        };
        let kind = data
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or(self.event)
            .unwrap_or_else(|| "message".into());
        if kind.len() > 128 || kind.is_empty() || kind.chars().any(char::is_control) {
            return Err(CliError::Stream("SSE event type is invalid".into()));
        }
        let message = data
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| data.as_str().map(str::to_owned))
            .unwrap_or_default();
        Ok(AgentEvent {
            kind,
            message,
            data,
        })
    }
}

#[derive(Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
    pending: VecDeque<SseFrame>,
}

impl SseDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> CliResult<Vec<SseFrame>> {
        self.buffer.extend_from_slice(bytes);
        if self.buffer.len() > 1024 * 1024 {
            return Err(CliError::Stream("SSE frame exceeds 1 MiB".into()));
        }
        while let Some((position, delimiter_length)) = delimiter(&self.buffer) {
            let frame = self.buffer.drain(..position).collect::<Vec<_>>();
            self.buffer.drain(..delimiter_length);
            if let Some(frame) = parse_frame(&frame)? {
                self.pending.push_back(frame);
            }
        }
        Ok(self.pending.drain(..).collect())
    }

    pub fn finish(&mut self) -> CliResult<Vec<SseFrame>> {
        if !self.buffer.is_empty() {
            if let Some(frame) = parse_frame(&std::mem::take(&mut self.buffer))? {
                self.pending.push_back(frame);
            }
        }
        Ok(self.pending.drain(..).collect())
    }
}

fn delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer
        .windows(2)
        .position(|window| window == b"\n\n")
        .map(|at| (at, 2));
    let crlf = buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| (at, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn parse_frame(bytes: &[u8]) -> CliResult<Option<SseFrame>> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| CliError::Stream(format!("SSE is not UTF-8: {error}")))?;
    let mut event = None;
    let mut id = None;
    let mut data = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        match field {
            "event" => event = Some(value.to_owned()),
            "id" if !value.contains('\0') => id = Some(value.to_owned()),
            "data" => data.push(value),
            _ => {}
        }
    }
    if event.is_none() && id.is_none() && data.is_empty() {
        return Ok(None);
    }
    Ok(Some(SseFrame {
        event,
        id,
        data: data.join("\n"),
    }))
}

fn bounded(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
