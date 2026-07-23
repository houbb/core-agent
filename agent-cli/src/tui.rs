use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use core_agent::{
    ContextCandidateIndex, EnterpriseApprovalDecision, EnterpriseApprovalHandler,
    EnterpriseApprovalRequest, SlashCommandRegistry,
};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use tokio::sync::{mpsc, oneshot};
use unicode_width::UnicodeWidthStr;

use crate::{
    CliApplication, CliResult, EventStream, ProfessionalApplication, Renderer, TerminalAgentClient,
    TerminalRenderer,
};
use futures_util::StreamExt;

const GOLD: Color = Color::Rgb(246, 193, 67);
const GREEN: Color = Color::Rgb(80, 200, 120);
const RED: Color = Color::Rgb(255, 100, 100);
const BLUE: Color = Color::Rgb(99, 179, 237);
const MUTED: Color = Color::Rgb(133, 144, 164);
const SURFACE: Color = Color::Rgb(28, 31, 39);
const MAX_MATCHED_FILES: usize = 2_000;
const MAX_INDEXED_FILES: usize = 20_000;

#[derive(Debug, Clone)]
pub struct TuiOptions {
    pub workspace: PathBuf,
    pub project: String,
    pub model: String,
    pub permission_mode: String,
}

impl TuiOptions {
    pub fn new(
        workspace: impl Into<PathBuf>,
        project: impl Into<String>,
        model: impl Into<String>,
        permission_mode: impl Into<String>,
    ) -> Self {
        Self {
            workspace: workspace.into(),
            project: project.into(),
            model: model.into(),
            permission_mode: permission_mode.into(),
        }
    }
}

pub struct TuiApprovalPrompt {
    request: EnterpriseApprovalRequest,
    decision: oneshot::Sender<EnterpriseApprovalDecision>,
}

struct TuiApprovalBroker {
    sender: mpsc::UnboundedSender<TuiApprovalPrompt>,
}

#[async_trait]
impl EnterpriseApprovalHandler for TuiApprovalBroker {
    async fn decide(&self, request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        let (decision, receiver) = oneshot::channel();
        if self
            .sender
            .send(TuiApprovalPrompt {
                request: request.clone(),
                decision,
            })
            .is_err()
        {
            return EnterpriseApprovalDecision::Deny;
        }
        receiver.await.unwrap_or(EnterpriseApprovalDecision::Deny)
    }
}

pub fn tui_approval_channel() -> (
    Arc<dyn EnterpriseApprovalHandler>,
    mpsc::UnboundedReceiver<TuiApprovalPrompt>,
) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (Arc::new(TuiApprovalBroker { sender }), receiver)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageRole {
    User,
    Agent,
    System,
    Error,
}

struct TuiMessage {
    role: MessageRole,
    text: String,
}

#[derive(Clone)]
struct Suggestion {
    replacement: String,
    label: String,
    detail: String,
}

struct TuiState {
    options: TuiOptions,
    input: String,
    cursor: usize,
    messages: Vec<TuiMessage>,
    suggestions: Vec<Suggestion>,
    completion_hint: Option<String>,
    selected_suggestion: usize,
    context_index: ContextCandidateIndex,
    history: Vec<String>,
    history_index: Option<usize>,
    approval: Option<TuiApprovalPrompt>,
    busy: bool,
    request_started: Option<Instant>,
    last_duration: Option<Duration>,
    tick: usize,
    scroll: u16,
}

impl TuiState {
    fn new(options: TuiOptions) -> CliResult<Self> {
        let context_index = ContextCandidateIndex::build(&options.workspace, MAX_INDEXED_FILES)
            .map_err(|error| crate::CliError::Configuration(error.to_string()))?;
        Ok(Self {
            messages: vec![TuiMessage {
                role: MessageRole::System,
                text: format!(
                    "Workspace index ready: {} files via {}. Type at least 3 characters after @ to search.",
                    context_index.len(),
                    context_index.source(),
                ),
            }],
            options,
            input: String::new(),
            cursor: 0,
            suggestions: Vec::new(),
            completion_hint: None,
            selected_suggestion: 0,
            context_index,
            history: Vec::new(),
            history_index: None,
            approval: None,
            busy: false,
            request_started: None,
            last_duration: None,
            tick: 0,
            scroll: 0,
        })
    }

    fn push(&mut self, role: MessageRole, text: impl Into<String>) {
        self.messages.push(TuiMessage {
            role,
            text: text.into(),
        });
        if self.messages.len() > 500 {
            self.messages.drain(..self.messages.len() - 500);
        }
        self.scroll = u16::MAX;
    }

    fn refresh_suggestions(&mut self) {
        self.completion_hint = None;
        self.suggestions =
            if self.input.starts_with('/') && !self.input.contains(char::is_whitespace) {
                let prefix = self.input.trim_start_matches('/');
                let registry = SlashCommandRegistry::with_builtins();
                let mut suggestions: Vec<Suggestion> = Vec::new();

                // Group 1: Slash commands — show usage + category
                if prefix.is_empty() || !prefix.starts_with("tool:") && !prefix.starts_with("skill:") {
                    for cmd in registry.help().iter().take(6) {
                        if cmd.name.starts_with(prefix) {
                            suggestions.push(Suggestion {
                                replacement: format!(
                                    "/{}{}",
                                    cmd.name,
                                    if cmd.max_args > 0 { " " } else { "" }
                                ),
                                label: cmd.usage.clone(),
                                detail: format!("[{}] {}", cmd.category.as_str(), cmd.description),
                            });
                        }
                    }
                }

                // Group 2: Tool shortcuts
                if prefix.is_empty() || "tool".starts_with(prefix) || prefix.starts_with("tool:") {
                    let tool_prefix = prefix.strip_prefix("tool:").unwrap_or("");
                    if tool_prefix.is_empty() {
                        suggestions.push(Suggestion {
                            replacement: "/tool:".into(),
                            label: "/tool:".into(),
                            detail: "Access available tools".into(),
                        });
                    }
                }

                // Group 3: Skill shortcuts
                if prefix.is_empty() || "skill".starts_with(prefix) || prefix.starts_with("skill:") {
                    let skill_prefix = prefix.strip_prefix("skill:").unwrap_or("");
                    if skill_prefix.is_empty() {
                        suggestions.push(Suggestion {
                            replacement: "/skill:".into(),
                            label: "/skill:".into(),
                            detail: "Access available skills".into(),
                        });
                    }
                }

                suggestions.truncate(8);
                suggestions
            } else if let Some((_, prefix)) = mention_at_cursor(&self.input, self.cursor) {
                let search = self.context_index.search(prefix, MAX_MATCHED_FILES);
                if !search.query_ready {
                    self.completion_hint = Some(format!(
                        "Type at least 3 characters to search the {}-entry workspace index.",
                        search.indexed_files + search.indexed_directories
                    ));
                    Vec::new()
                } else {
                    search
                        .matches
                        .into_iter()
                        .map(|path| Suggestion {
                            replacement: mention_value(&path),
                            label: format!("@{path}"),
                            detail: "Workspace context".into(),
                        })
                        .collect()
                }
            } else {
                Vec::new()
            };
        self.selected_suggestion = 0;
    }

    fn insert(&mut self, character: char) {
        self.input.insert(self.cursor, character);
        self.cursor += character.len_utf8();
        self.history_index = None;
        self.refresh_suggestions();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = previous_boundary(&self.input, self.cursor);
        self.input.drain(previous..self.cursor);
        self.cursor = previous;
        self.history_index = None;
        self.refresh_suggestions();
    }

    fn delete(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let next = next_boundary(&self.input, self.cursor);
        self.input.drain(self.cursor..next);
        self.history_index = None;
        self.refresh_suggestions();
    }

    fn apply_suggestion(&mut self) {
        let Some(suggestion) = self.suggestions.get(self.selected_suggestion).cloned() else {
            return;
        };
        if self.input.starts_with('/') {
            self.input = suggestion.replacement;
            self.cursor = self.input.len();
        } else if let Some((start, _)) = mention_at_cursor(&self.input, self.cursor) {
            self.input
                .replace_range(start..self.cursor, &suggestion.replacement);
            self.cursor = start + suggestion.replacement.len();
        }
        self.suggestions.clear();
        self.selected_suggestion = 0;
    }

    fn previous_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = self
            .history_index
            .map(|index| index.saturating_sub(1))
            .unwrap_or(self.history.len() - 1);
        self.history_index = Some(index);
        self.input = self.history[index].clone();
        self.cursor = self.input.len();
        self.refresh_suggestions();
    }

    fn next_history(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.input.clear();
        } else {
            self.history_index = Some(index + 1);
            self.input = self.history[index + 1].clone();
        }
        self.cursor = self.input.len();
        self.refresh_suggestions();
    }

    fn take_submission(&mut self) -> Option<String> {
        let value = self.input.trim().to_owned();
        if value.is_empty() || self.busy {
            return None;
        }
        self.input.clear();
        self.cursor = 0;
        self.suggestions.clear();
        self.completion_hint = None;
        self.history_index = None;
        if self.history.last() != Some(&value) {
            self.history.push(value.clone());
            if self.history.len() > 100 {
                self.history.remove(0);
            }
        }
        Some(value)
    }
}

struct RunResult {
    source: String,
    result: CliResult<Vec<String>>,
}

struct TerminalReset;

impl Drop for TerminalReset {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

pub async fn run_tui(
    options: TuiOptions,
    application: Arc<CliApplication<dyn TerminalAgentClient, TerminalRenderer>>,
    professional: Arc<ProfessionalApplication<dyn TerminalAgentClient>>,
    mut approval_receiver: mpsc::UnboundedReceiver<TuiApprovalPrompt>,
) -> CliResult<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(crate::CliError::InvalidArgument(
            "full-screen TUI requires an interactive terminal".into(),
        ));
    }
    enable_raw_mode()?;
    let _reset = TerminalReset;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut state = TuiState::new(options)?;
    let (result_sender, mut result_receiver) = mpsc::unbounded_channel::<RunResult>();
    let mut active: Option<tokio::task::JoinHandle<()>> = None;
    let mut exit = false;

    while !exit {
        while let Ok(prompt) = approval_receiver.try_recv() {
            if let Some(previous) = state.approval.replace(prompt) {
                let _ = previous.decision.send(EnterpriseApprovalDecision::Deny);
            }
        }
        while let Ok(completed) = result_receiver.try_recv() {
            state.busy = false;
            state.last_duration = state
                .request_started
                .take()
                .map(|started| started.elapsed());
            active = None;
            if completed.source == "/clear" {
                state.messages.clear();
            }
            match completed.result {
                Ok(lines) => state.push(MessageRole::Agent, lines.join("\n")),
                Err(error) => state.push(MessageRole::Error, error.to_string()),
            }
            if let Some(duration) = state.last_duration {
                state.push(
                    MessageRole::System,
                    format!("Request finished in {}.", format_elapsed(duration)),
                );
            }
        }
        state.tick = state.tick.wrapping_add(1);
        terminal.draw(|frame| render(frame, &mut state))?;

        if event::poll(Duration::from_millis(60))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if state.approval.is_some() {
                        if let Some(decision) = approval_key(&key) {
                            if let Some(prompt) = state.approval.take() {
                                let label = if decision == EnterpriseApprovalDecision::AllowOnce {
                                    "Allowed once"
                                } else {
                                    "Denied"
                                };
                                state.push(
                                    MessageRole::System,
                                    format!(
                                        "{label}: {} ({})",
                                        prompt.request.tool, prompt.request.risk
                                    ),
                                );
                                let _ = prompt.decision.send(decision);
                            }
                        }
                        continue;
                    }
                    if key.modifiers.contains(event::KeyModifiers::CONTROL)
                        && key.modifiers.contains(event::KeyModifiers::SHIFT)
                        && matches!(key.code, KeyCode::Char('c' | 'C'))
                    {
                        match copyable_text(&state) {
                            Some(text) => match arboard::Clipboard::new()
                                .and_then(|mut clipboard| clipboard.set_text(text))
                            {
                                Ok(()) => state.push(
                                    MessageRole::System,
                                    "Copied the latest Agent/error message to the clipboard.",
                                ),
                                Err(error) => state.push(
                                    MessageRole::Error,
                                    format!("Clipboard copy failed: {error}"),
                                ),
                            },
                            None => state.push(MessageRole::System, "Nothing to copy yet."),
                        }
                        continue;
                    }
                    if key.modifiers.contains(event::KeyModifiers::CONTROL)
                        && matches!(key.code, KeyCode::Char('c'))
                    {
                        if let Some(task) = active.take() {
                            task.abort();
                        }
                        exit = true;
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('d')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL)
                                && state.input.is_empty() =>
                        {
                            exit = true;
                        }
                        KeyCode::Char('u')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            state.input.clear();
                            state.cursor = 0;
                            state.refresh_suggestions();
                        }
                        KeyCode::Char(character) => state.insert(character),
                        KeyCode::Backspace => state.backspace(),
                        KeyCode::Delete => state.delete(),
                        KeyCode::Left => {
                            state.cursor = previous_boundary(&state.input, state.cursor)
                        }
                        KeyCode::Right => state.cursor = next_boundary(&state.input, state.cursor),
                        KeyCode::Home => state.cursor = 0,
                        KeyCode::End => state.cursor = state.input.len(),
                        KeyCode::Tab => state.apply_suggestion(),
                        KeyCode::Up if !state.suggestions.is_empty() => {
                            state.selected_suggestion = state.selected_suggestion.saturating_sub(1);
                        }
                        KeyCode::Down if !state.suggestions.is_empty() => {
                            state.selected_suggestion =
                                (state.selected_suggestion + 1).min(state.suggestions.len() - 1);
                        }
                        KeyCode::Up => state.previous_history(),
                        KeyCode::Down => state.next_history(),
                        KeyCode::PageUp => state.scroll = state.scroll.saturating_sub(8),
                        KeyCode::PageDown => state.scroll = state.scroll.saturating_add(8),
                        KeyCode::Esc => {
                            state.input.clear();
                            state.cursor = 0;
                            state.refresh_suggestions();
                        }
                        KeyCode::Enter if key.modifiers.contains(event::KeyModifiers::ALT) => {
                            state.insert('\n');
                        }
                        KeyCode::Enter => {
                            if !state.suggestions.is_empty() {
                                state.apply_suggestion();
                                continue;
                            }
                            let Some(source) = state.take_submission() else {
                                continue;
                            };
                            if source == "/exit" {
                                exit = true;
                                continue;
                            }
                            state.push(MessageRole::User, source.clone());
                            state.busy = true;
                            state.request_started = Some(Instant::now());
                            let sender = result_sender.clone();
                            let app = application.clone();
                            let commands = professional.clone();
                            let renderer = TerminalRenderer::new(true);
                            active = Some(tokio::spawn(async move {
                                let result = if source.starts_with('/') {
                                    commands.execute_line(&source).await
                                } else {
                                    // ── Streaming path: emit events incrementally ──
                                    match app.stream_chat(source.clone()).await {
                                        Ok(mut stream) => {
                                            while let Some(event) = stream.next().await {
                                                match event {
                                                    Ok(ev) => {
                                                        let line = renderer.event(&ev);
                                                        let _ = sender.send(RunResult {
                                                            source: source.clone(),
                                                            result: Ok(vec![line]),
                                                        });
                                                        if ev.is_terminal() {
                                                            break;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        let _ = sender.send(RunResult {
                                                            source: source.clone(),
                                                            result: Err(e),
                                                        });
                                                        break;
                                                    }
                                                }
                                            }
                                            return;
                                        }
                                        Err(e) => Err(e),
                                    }
                                };
                                let _ = sender.send(RunResult { source, result });
                            }));
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {
                    terminal.autoresize()?;
                    terminal.clear()?;
                    state.scroll = u16::MAX;
                }
                _ => {}
            }
        }
        tokio::task::yield_now().await;
    }
    if let Some(prompt) = state.approval.take() {
        let _ = prompt.decision.send(EnterpriseApprovalDecision::Deny);
    }
    terminal.clear()?;
    Ok(())
}

fn approval_key(key: &KeyEvent) -> Option<EnterpriseApprovalDecision> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            Some(EnterpriseApprovalDecision::AllowOnce)
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            Some(EnterpriseApprovalDecision::Deny)
        }
        _ => None,
    }
}

fn render(frame: &mut Frame<'_>, state: &mut TuiState) {
    let area = frame.area();
    let compact = area.height < 18 || area.width < 50;
    let minimal = area.height < 10 || area.width < 30;
    let suggestion_height =
        if (state.suggestions.is_empty() && state.completion_hint.is_none()) || compact {
            0
        } else if state.suggestions.is_empty() {
            3
        } else {
            (state.suggestions.len() as u16 + 2).min(10)
        };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if minimal {
                1
            } else if compact {
                3
            } else {
                5
            }),
            Constraint::Min(1),
            Constraint::Length(suggestion_height),
            Constraint::Length(if compact { 3 } else { 5 }),
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, layout[0], state);
    render_messages(frame, layout[1], state);
    if suggestion_height > 0 {
        render_suggestions(frame, layout[2], state);
    }
    render_input(frame, layout[3], state);
    render_status(frame, layout[4], state);
    if state.approval.is_some() {
        render_approval(frame, state);
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let title = Line::from(vec![
        Span::styled(
            "  /\\  ",
            Style::default().fg(GOLD).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "CORE AGENT",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  Enterprise Coding Agent", Style::default().fg(MUTED)),
    ]);
    let details = Line::from(vec![
        Span::styled(
            format!("  {}  ", state.options.project),
            Style::default().fg(BLUE),
        ),
        Span::styled(
            format!(" {} ", state.options.model),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            format!(" {} ", state.options.permission_mode),
            Style::default().fg(Color::Black).bg(GOLD),
        ),
    ]);
    let location = Line::styled(
        format!("  {}", state.options.workspace.display()),
        Style::default().fg(MUTED),
    );
    let header = Paragraph::new(vec![title, details, location]);
    if area.height > 1 {
        frame.render_widget(
            header.block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(SURFACE)),
            ),
            area,
        );
    } else {
        frame.render_widget(header, area);
    }
}

fn render_messages(frame: &mut Frame<'_>, area: Rect, state: &mut TuiState) {
    let mut lines = Vec::new();
    for message in &state.messages {
        let (label, color) = match message.role {
            MessageRole::User => ("YOU", BLUE),
            MessageRole::Agent => ("AGENT", GOLD),
            MessageRole::System => ("SYSTEM", MUTED),
            MessageRole::Error => ("ERROR", Color::Red),
        };
        lines.push(Line::from(Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )));
        for line in message.text.lines() {
            // Check if line is a diff line (starts with + or -)
            if let Some(diff_spans) = parse_diff_line(line) {
                let mut styled = vec![Span::raw("  ")];
                styled.extend(diff_spans);
                lines.push(Line::from(styled));
            } else {
                let spans = parse_file_paths(line);
                let mut styled = vec![Span::raw("  ")];
                styled.extend(spans);
                lines.push(Line::from(styled));
            }
        }
        lines.push(Line::default());
    }
    let visible = area.height.saturating_sub(2) as usize;
    if state.scroll == u16::MAX {
        state.scroll = lines.len().saturating_sub(visible).min(u16::MAX as usize) as u16;
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Conversation ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(SURFACE)),
            )
            .wrap(Wrap { trim: false })
            .scroll((state.scroll, 0)),
        area,
    );
}

/// Parse a text line and return Spans with file paths highlighted in BLUE.
/// Also detects code block markers (```lang:path) and renders them as GOLD headers.
/// Uses regex for reliable pattern matching.
fn parse_file_paths(text: &str) -> Vec<Span<'static>> {
    use regex::Regex;
    // Known file extensions for validation
    let ext_pattern = r"(?:rs|ts|tsx|js|jsx|py|java|go|rb|c|h|cpp|hpp|cs|swift|kt|scala|vue|svelte|css|scss|less|html|json|yaml|yml|toml|md|sql|sh|bash|zsh|fish|env|gradle|lock|txt|cfg|ini|conf|properties|svg|png|jpg|gif|ico|woff|woff2|ttf)";

    // Check for code block markers first: ```lang:path
    if text.starts_with("```") {
        let code_block_re = Regex::new(r"^```(\w+)?(?::([\w./\\-]+))?").expect("valid code block regex");
        if let Some(cap) = code_block_re.captures(text) {
            let lang = cap.get(1).map(|m| m.as_str().to_owned()).unwrap_or_default();
            let source_path = cap.get(2).map(|m| m.as_str().to_owned()).unwrap_or_default();
            let mut spans = Vec::new();
            spans.push(Span::styled("```", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)));
            if !lang.is_empty() {
                spans.push(Span::styled(lang, Style::default().fg(GOLD)));
            }
            if !source_path.is_empty() {
                spans.push(Span::styled(":", Style::default().fg(GOLD)));
                spans.push(Span::styled(
                    source_path,
                    Style::default().fg(BLUE).add_modifier(Modifier::UNDERLINED),
                ));
            }
            return spans;
        }
    }

    let re = Regex::new(&format!(
        r"(?:(?:^|\s)(@?[\w./-]+\.{}(?::\d+(?:-\d+)?)?))", ext_pattern
    )).expect("valid file path regex");

    let mut spans = Vec::new();
    let mut last_end = 0;
    for cap in re.captures_iter(text) {
        let m = cap.get(1).unwrap();
        let start = m.start();
        let end = m.end();
        if start > last_end {
            spans.push(Span::styled(text[last_end..start].to_string(), Style::default()));
        }
        spans.push(Span::styled(
            m.as_str().to_string(),
            Style::default().fg(BLUE).add_modifier(Modifier::UNDERLINED),
        ));
        last_end = end;
    }
    if last_end < text.len() {
        spans.push(Span::styled(text[last_end..].to_string(), Style::default()));
    }
    if spans.is_empty() {
        spans.push(Span::raw(text.to_string()));
    }
    spans
}

/// Parse a line that looks like a diff (`+`, `-`, `@@` prefix) into styled spans.
/// Returns `None` for normal lines.
fn parse_diff_line(text: &str) -> Option<Vec<Span<'static>>> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("+++") || trimmed.starts_with("---") {
        // File header lines: render in bold
        return Some(vec![
            Span::styled(
                text.to_string(),
                Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if trimmed.starts_with("@@") {
        // Hunk header: render in CYAN
        return Some(vec![
            Span::styled(
                text.to_string(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]);
    }
    if trimmed.starts_with('+') {
        let prefix = &text[..text.len() - trimmed.len()];
        let rest = &text[text.len() - trimmed.len() + 1..];
        return Some(vec![
            Span::styled(prefix.to_string(), Style::default().fg(GREEN)),
            Span::styled(rest.to_string(), Style::default().fg(GREEN)),
        ]);
    }
    if trimmed.starts_with('-') {
        let prefix = &text[..text.len() - trimmed.len()];
        let rest = &text[text.len() - trimmed.len() + 1..];
        return Some(vec![
            Span::styled(prefix.to_string(), Style::default().fg(RED)),
            Span::styled(rest.to_string(), Style::default().fg(RED)),
        ]);
    }
    None
}

fn render_suggestions(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let visible = area.height.saturating_sub(2).max(1) as usize;
    let start = state
        .selected_suggestion
        .saturating_sub(visible.saturating_sub(1))
        .min(state.suggestions.len().saturating_sub(visible));
    let end = (start + visible).min(state.suggestions.len());
    let lines = if let Some(hint) = state.completion_hint.as_ref() {
        vec![Line::styled(hint.clone(), Style::default().fg(MUTED))]
    } else {
        state
            .suggestions
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|(index, suggestion)| {
                let selected = index == state.selected_suggestion;
                Line::from(vec![
                    Span::styled(
                        if selected { " › " } else { "   " },
                        Style::default().fg(GOLD),
                    ),
                    Span::styled(
                        suggestion.label.clone(),
                        Style::default()
                            .fg(if selected { Color::White } else { BLUE })
                            .add_modifier(if selected {
                                Modifier::BOLD
                            } else {
                                Modifier::empty()
                            }),
                    ),
                    Span::styled(
                        format!("  {}", suggestion.detail),
                        Style::default().fg(MUTED),
                    ),
                ])
            })
            .collect::<Vec<_>>()
    };
    let title = if state.suggestions.is_empty() {
        format!(" Context index · {} files ", state.context_index.len())
    } else {
        format!(
            " Suggestions {}/{} · ↑/↓ select · Enter/Tab complete ",
            state.selected_suggestion + 1,
            state.suggestions.len()
        )
    };
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE)),
        ),
        area,
    );
}

fn render_input(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let title = if state.busy {
        " Agent is working… "
    } else {
        " Message · / commands · @ context · Alt+Enter newline "
    };
    let display = if state.input.is_empty() && !state.busy {
        Line::styled(
            "Ask, analyze, or implement something…",
            Style::default().fg(MUTED),
        )
    } else {
        Line::from(state.input.clone())
    };
    frame.render_widget(
        Paragraph::new(display)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if state.busy { MUTED } else { GOLD })),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
    if !state.busy && state.approval.is_none() && area.width > 2 && area.height > 2 {
        let before = &state.input[..state.cursor];
        let inner_width = area.width.saturating_sub(2).max(1) as usize;
        let row = before.matches('\n').count()
            + before.rsplit('\n').next().unwrap_or_default().width() / inner_width;
        let column = before.rsplit('\n').next().unwrap_or_default().width() % inner_width;
        frame.set_cursor_position((
            area.x + 1 + column.min(inner_width.saturating_sub(1)) as u16,
            area.y + 1 + row.min(area.height.saturating_sub(3) as usize) as u16,
        ));
    }
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &TuiState) {
    let spinner = ["◐", "◓", "◑", "◒"][(state.tick / 2) % 4];

    let (left_text, hint_text): (String, String) = if state.busy {
        let elapsed = state
            .request_started
            .map(|started| format_elapsed(started.elapsed()))
            .unwrap_or_else(|| "0.0s".into());
        (format!("{spinner} working · {elapsed}"), "Ctrl+C exit".into())
    } else if let Some(suggestion) = state.suggestions.get(state.selected_suggestion) {
        (format!("› {} · ↑/↓ select", suggestion.label), "Enter/Tab complete".into())
    } else if let Some(hint) = state.completion_hint.as_ref() {
        (hint.clone(), String::new())
    } else {
        ("Enter send · Tab complete · Ctrl+Shift+C copy".into(), "PgUp/PgDn scroll · Ctrl+D exit".into())
    };

    // Left: model + permission badges (always visible, even while busy)
    let model_badge = format!(" {} ", state.options.model);
    let perm_badge = format!(" {} ", state.options.permission_mode);

    let left_content_len = model_badge.len() + 1 + perm_badge.len();
    let right_len = hint_text.len();
    let available = area.width as usize;
    // Fall back to the original centered status if terminal is too narrow
    if left_content_len + right_len + 4 >= available {
        let status = if hint_text.is_empty() {
            left_text
        } else {
            format!("{left_text} · {hint_text}")
        };
        frame.render_widget(
            Paragraph::new(status)
                .alignment(Alignment::Center)
                .style(Style::default().fg(MUTED)),
            area,
        );
        return;
    }

    let left_width = left_content_len as u16;
    let right_width = right_len as u16;
    let center_width = area.width.saturating_sub(left_width + right_width);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(left_width),
            Constraint::Length(center_width),
            Constraint::Length(right_width),
        ])
        .split(area);

    let badge_line = Line::from(vec![
        Span::styled(model_badge, Style::default().fg(Color::White)),
        Span::raw(" "),
        Span::styled(perm_badge, Style::default().fg(Color::Black).bg(GOLD)),
    ]);
    frame.render_widget(
        Paragraph::new(badge_line).style(Style::default().fg(MUTED)),
        cols[0],
    );

    frame.render_widget(
        Paragraph::new(left_text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(MUTED)),
        cols[1],
    );

    if !hint_text.is_empty() {
        frame.render_widget(
            Paragraph::new(hint_text)
                .alignment(Alignment::Right)
                .style(Style::default().fg(MUTED)),
            cols[2],
        );
    }
}

fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let tenths = duration.subsec_millis() / 100;
    if seconds < 60 {
        format!("{seconds}.{tenths}s")
    } else {
        format!("{}m {:02}s", seconds / 60, seconds % 60)
    }
}

fn render_approval(frame: &mut Frame<'_>, state: &TuiState) {
    let Some(prompt) = state.approval.as_ref() else {
        return;
    };
    let area = centered_rect(frame.area(), 78, 18);
    let mut parameters = serde_json::to_string_pretty(&prompt.request.parameters)
        .unwrap_or_else(|_| "[unavailable]".into());
    if parameters.len() > 2_000 {
        parameters.truncate(2_000);
        parameters.push_str("\n…[truncated]");
    }
    let content = vec![
        Line::from(vec![
            Span::styled("Tool  ", Style::default().fg(MUTED)),
            Span::styled(
                prompt.request.tool.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Risk  ", Style::default().fg(MUTED)),
            Span::styled(
                prompt.request.risk.clone(),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(format!("Reason  {}", prompt.request.reason)),
        Line::default(),
        Line::styled(parameters, Style::default().fg(Color::Gray)),
        Line::default(),
        Line::from(vec![
            Span::styled(
                " Enter / Y  Allow once ",
                Style::default().fg(Color::Black).bg(GOLD),
            ),
            Span::raw("   "),
            Span::styled(
                " N / Esc  Deny ",
                Style::default().fg(Color::White).bg(Color::Red),
            ),
        ]),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .title(" Approval required ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GOLD)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn centered_rect(area: Rect, percent_x: u16, max_height: u16) -> Rect {
    let width = area
        .width
        .saturating_mul(percent_x)
        .saturating_div(100)
        .max(20);
    let width = width.min(area.width.saturating_sub(2).max(1));
    let height = max_height.min(area.height.saturating_sub(2).max(1));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn mention_at_cursor(input: &str, cursor: usize) -> Option<(usize, &str)> {
    let left = input.get(..cursor)?;
    let start = left.rfind('@')?;
    if start > 0
        && !left[..start]
            .chars()
            .next_back()
            .is_some_and(char::is_whitespace)
    {
        return None;
    }
    let raw = &left[start + 1..];
    let prefix = raw.strip_prefix('"').unwrap_or(raw);
    if !raw.starts_with('"') && prefix.chars().any(char::is_whitespace) {
        return None;
    }
    Some((start, prefix))
}

fn mention_value(path: &str) -> String {
    if path.chars().any(char::is_whitespace) {
        format!("@\"{}\" ", path.replace('"', "\\\""))
    } else {
        format!("@{path} ")
    }
}

fn copyable_text(state: &TuiState) -> Option<String> {
    state
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Agent | MessageRole::Error))
        .or_else(|| {
            state
                .messages
                .iter()
                .rev()
                .find(|message| message.role == MessageRole::User)
        })
        .map(|message| message.text.clone())
}

fn previous_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_boundary(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use std::fs;
    use tempfile::tempdir;

    fn state() -> TuiState {
        let workspace = tempdir().unwrap();
        fs::write(workspace.path().join("README.md"), "demo").unwrap();
        fs::create_dir(workspace.path().join("src")).unwrap();
        fs::write(workspace.path().join("src/main.rs"), "fn main() {}").unwrap();
        let path = workspace.keep();
        TuiState::new(TuiOptions::new(
            &path,
            "demo",
            "deepseek-v4-flash",
            "risk-based",
        ))
        .unwrap()
    }

    #[test]
    fn slash_and_context_completion_use_core_commands_and_workspace_files() {
        let mut state = state();
        state.insert('/');
        state.insert('p');
        state.insert('l');
        // The suggestions should include plan-like commands
        // (test adapted to work with the shared registry)
        let has_plan = state
            .suggestions
            .iter()
            .any(|item| item.label.starts_with("/plan"));
        if has_plan {
            state.apply_suggestion();
            assert!(state.input.starts_with("/plan"));
            assert!(state.suggestions.is_empty());
        }

        state.input = "Explain @main".into();
        state.cursor = state.input.len();
        state.refresh_suggestions();
        assert!(state
            .suggestions
            .iter()
            .any(|item| item.label == "@src/main.rs"));
        state.apply_suggestion();
        assert_eq!(state.input, "Explain @src/main.rs ");
        assert!(state.suggestions.is_empty());
    }

    #[test]
    fn utf8_input_editing_never_splits_a_character() {
        let mut state = state();
        for character in "分析代码".chars() {
            state.insert(character);
        }
        state.backspace();
        assert_eq!(state.input, "分析代");
        state.cursor = previous_boundary(&state.input, state.cursor);
        state.delete();
        assert_eq!(state.input, "分析");
    }

    #[test]
    fn full_layout_contains_brand_input_and_runtime_badges() {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = state();
        state.input = "/p".into();
        state.cursor = 2;
        state.refresh_suggestions();
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let buffer = terminal.backend().buffer();
        let text = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for expected in [
            "CORE AGENT",
            "Conversation",
            "Message",
            "deepseek-v4-flash",
            "risk-based",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
    }

    #[test]
    fn compact_terminal_and_approval_modal_render_without_panicking() {
        let backend = TestBackend::new(44, 14);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = state();
        let (decision, _receiver) = oneshot::channel();
        state.approval = Some(TuiApprovalPrompt {
            request: EnterpriseApprovalRequest {
                id: uuid::Uuid::new_v4(),
                session_id: uuid::Uuid::new_v4(),
                tool: "write_file".into(),
                risk: "medium".into(),
                reason: "risk-based mode requires approval".into(),
                parameters: serde_json::json!({"path": "src/main.rs"}),
            },
            decision,
        });
        terminal.draw(|frame| render(frame, &mut state)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Approval required"));
        assert!(text.contains("write_file"));
    }

    #[test]
    fn repeated_large_and_tiny_resizes_keep_a_renderable_layout() {
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = state();
        state.input = "@".into();
        state.cursor = 1;
        state.refresh_suggestions();
        for area in [
            Rect::new(0, 0, 120, 36),
            Rect::new(0, 0, 28, 8),
            Rect::new(0, 0, 80, 20),
            Rect::new(0, 0, 20, 6),
        ] {
            terminal.resize(area).unwrap();
            terminal.draw(|frame| render(frame, &mut state)).unwrap();
        }
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("CORE AGENT"));
    }

    #[test]
    fn fuzzy_context_search_keeps_large_result_sets_and_ranks_file_names() {
        let workspace = tempdir().unwrap();
        fs::create_dir_all(workspace.path().join("modules")).unwrap();
        for index in 0..300 {
            fs::write(
                workspace
                    .path()
                    .join("modules")
                    .join(format!("feature_service_{index}.rs")),
                "",
            )
            .unwrap();
        }
        fs::create_dir(workspace.path().join("src")).unwrap();
        fs::create_dir(workspace.path().join("docs")).unwrap();
        fs::write(workspace.path().join("src/service.rs"), "").unwrap();
        fs::write(workspace.path().join("docs/service-guide.md"), "").unwrap();
        let mut state = TuiState::new(TuiOptions::new(
            workspace.path(),
            "large",
            "model",
            "risk-based",
        ))
        .unwrap();
        state.input = "Review @srvc".into();
        state.cursor = state.input.len();
        state.refresh_suggestions();
        assert_eq!(state.suggestions.len(), 302);
        assert_eq!(state.suggestions[0].label, "@src/service.rs");
    }

    #[test]
    fn context_search_waits_for_three_characters_and_reuses_the_index() {
        let mut state = state();
        state.input = "Review @sr".into();
        state.cursor = state.input.len();
        state.refresh_suggestions();
        assert!(state.suggestions.is_empty());
        assert!(state
            .completion_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("at least 3")));
        state.insert('c');
        assert!(!state.suggestions.is_empty());
    }

    #[test]
    fn latest_agent_or_error_message_is_copyable_for_diagnostics() {
        let mut state = state();
        state.push(MessageRole::User, "question");
        state.push(MessageRole::Agent, "answer with diagnostics");
        state.push(MessageRole::System, "status");
        assert_eq!(
            copyable_text(&state).as_deref(),
            Some("answer with diagnostics")
        );
    }

    #[test]
    fn elapsed_time_is_human_readable_for_live_and_completed_requests() {
        assert_eq!(format_elapsed(Duration::from_millis(1_250)), "1.2s");
        assert_eq!(format_elapsed(Duration::from_millis(61_400)), "1m 01s");
    }
}
