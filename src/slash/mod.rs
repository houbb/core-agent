//! Slash Command Runtime — 统一的 Slash Command 控制面
//!
//! 提供 SlashCommand trait、CommandMetadata、SlashCategory 等核心基础设施，
//! 让所有入口（CLI/TUI/Desktop/Web/API）共享同一套命令系统。
//!
//! # Architecture
//!
//! ```text
//! SlashCommandRegistry
//!     │
//!     ├── builtin: InteractionCommandRegistry  (向后兼容)
//!     └── plugins: BTreeMap<name, Arc<dyn SlashCommand>>  (插件式注册)
//!
//! 每个 SlashCommand 实现：
//!     metadata() → category() → validate() → execute()
//!
//! SlashCommandObserver 监听：
//!     on_command_start → on_command_success / on_command_failure
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::interaction::{
    InteractionCommandDefinition, InteractionCommandInvocation, InteractionCommandRegistry,
    InteractionCommandRoute, InteractionEntryAction, InteractionEntryOutcome, InteractionError,
    InteractionResult,
};

// ── SlashCategory ──

/// 命令分类体系
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SlashCategory {
    /// 系统级命令：/exit, /help
    System,
    /// 会话管理：/new, /clear, /sessions
    Session,
    /// 上下文工程：/context, /compact, /comment
    Context,
    /// 项目管理：/project, /config
    Project,
    /// 记忆系统：/memory
    Memory,
    /// Agent 命令：/plan, /review, /explain, /test, /fix, /refactor
    Agent,
    /// 认知命令：/reason, /think, /hypothesis, /critic, /reflect, /decision
    Cognitive,
    /// 检查点：/undo, /redo, /checkpoint
    Checkpoint,
    /// 治理：/permissions, /approve
    Governance,
    /// 可观测性：/trace-agent, /evaluate, /benchmark, /debug, /replay, /score
    Observability,
    /// Agent 社会系统：/agents, /delegate, /team, /roles, /collaborate
    Society,
    /// 开发者工具：/commit, /pr
    Developer,
    /// 工作流：/workflow, /trigger, /schedule, /run, /observe, /retry
    Workflow,
    /// 编排：/orchestrate, /subagent, /message
    Orchestration,
}

impl SlashCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Session => "session",
            Self::Context => "context",
            Self::Project => "project",
            Self::Memory => "memory",
            Self::Agent => "agent",
            Self::Cognitive => "cognitive",
            Self::Checkpoint => "checkpoint",
            Self::Governance => "governance",
            Self::Observability => "observability",
            Self::Society => "society",
            Self::Developer => "developer",
            Self::Workflow => "workflow",
            Self::Orchestration => "orchestration",
        }
    }
}

// ── CommandMetadata ──

/// 命令元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandMetadata {
    /// 命令名称（小写字母 + 连字符，如 "compact"）
    pub name: String,
    /// 显示名称（如 "Context Compact"）
    pub display_name: String,
    /// 简短描述（最多 256 字符）
    pub description: String,
    /// 用法示例（如 "/compact"）
    pub usage: String,
    /// 分类
    pub category: SlashCategory,
    /// 最小参数数量
    pub min_args: usize,
    /// 最大参数数量
    pub max_args: usize,
    /// 是否为只读命令
    pub read_only: bool,
    /// 是否异步执行
    pub async_exec: bool,
}

// ── CommandContext ──

/// 命令执行上下文
#[derive(Debug, Clone)]
pub struct CommandContext {
    /// 原始输入行
    pub line: String,
    /// 命令参数
    pub args: Vec<String>,
    /// 当前工作区路径
    pub workspace: String,
    /// 当前会话 ID（可选）
    pub session_id: Option<String>,
    /// 自定义数据
    pub data: std::collections::HashMap<String, String>,
}

// ── CommandOutput ──

/// 命令执行输出
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutput {
    /// 输出文本
    pub response: String,
    /// 结构化数据
    pub data: serde_json::Value,
    /// 会话操作动作
    pub action: CommandAction,
}

impl CommandOutput {
    pub fn new(response: impl Into<String>) -> Self {
        Self {
            response: response.into(),
            data: serde_json::Value::Null,
            action: CommandAction::None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    pub fn with_action(mut self, action: CommandAction) -> Self {
        self.action = action;
        self
    }
}

/// 命令执行后的动作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandAction {
    None,
    NewSession,
    ClearView,
    Exit,
}

// ── SlashCommand trait ──

/// 统一的 Slash Command 接口
///
/// 所有 slash 命令都需要实现此 trait，以支持插件式注册和生命周期管理。
#[async_trait]
pub trait SlashCommand: Send + Sync {
    /// 返回命令元数据
    fn metadata(&self) -> CommandMetadata;

    /// 返回命令分类
    fn category(&self) -> SlashCategory;

    /// 验证参数
    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        let meta = self.metadata();
        if args.len() < meta.min_args || args.len() > meta.max_args {
            return Err(SlashError::InvalidArgument(format!(
                "usage: {}",
                meta.usage
            )));
        }
        Ok(())
    }

    /// 执行命令
    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput>;

    /// 注册时回调
    async fn on_register(&self) -> SlashResult<()> {
        Ok(())
    }

    /// 销毁时回调
    async fn on_destroy(&self) -> SlashResult<()> {
        Ok(())
    }
}

// ── SlashCommandObserver trait ──

/// 命令观察者
#[async_trait]
pub trait SlashCommandObserver: Send + Sync {
    /// 命令开始执行
    async fn on_command_start(&self, ctx: &CommandContext);
    /// 命令执行成功
    async fn on_command_success(&self, ctx: &CommandContext, output: &CommandOutput);
    /// 命令执行失败
    async fn on_command_failure(&self, ctx: &CommandContext, error: &SlashError);
}

// ── SlashError ──

/// 统一的 Slash 命令错误类型
#[derive(Debug, thiserror::Error)]
pub enum SlashError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("command execution failed: {0}")]
    Execution(String),
    #[error("command not found: {0}")]
    NotFound(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type SlashResult<T> = Result<T, SlashError>;

// ── SlashCommandRegistry ──

/// 插件式 Slash Command 注册表
///
/// 在保留 `InteractionCommandRegistry` 向后兼容的基础上，
/// 新增插件式 `SlashCommand` 注册能力和 `SlashCommandObserver` 观察者机制。
pub struct SlashCommandRegistry {
    /// 保留原有命令注册表（用于兼容）
    builtin: InteractionCommandRegistry,
    /// 插件式命令注册表
    plugins: BTreeMap<String, Arc<dyn SlashCommand>>,
    /// 命令观察者列表
    observers: Vec<Arc<dyn SlashCommandObserver>>,
}

impl SlashCommandRegistry {
    /// 创建包含内置命令的注册表
    pub fn with_builtins() -> Self {
        Self {
            builtin: InteractionCommandRegistry::with_builtins(),
            plugins: BTreeMap::new(),
            observers: Vec::new(),
        }
    }

    /// 注册一个插件命令
    pub fn register(&mut self, command: Arc<dyn SlashCommand>) -> SlashResult<()> {
        let meta = command.metadata();
        if self.plugins.contains_key(&meta.name) || self.builtin_contains(&meta.name) {
            return Err(SlashError::InvalidArgument(format!(
                "command /{} is already registered",
                meta.name
            )));
        }
        // 同时也注册到 InteractionCommandRegistry 保持兼容
        let route = if meta.read_only {
            InteractionCommandRoute::Agent
        } else {
            InteractionCommandRoute::Runtime
        };
        let _ = self.builtin.register(InteractionCommandDefinition {
            name: meta.name.clone(),
            summary: meta.description.clone(),
            usage: meta.usage.clone(),
            minimum_arguments: meta.min_args,
            maximum_arguments: meta.max_args,
            route,
        });
        self.plugins.insert(meta.name.clone(), command);
        Ok(())
    }

    /// 添加观察者
    pub fn add_observer(&mut self, observer: Arc<dyn SlashCommandObserver>) {
        self.observers.push(observer);
    }

    /// 解析命令
    pub fn parse(&self, line: &str) -> SlashResult<ParsedCommand> {
        if !line.starts_with('/') {
            return Err(SlashError::InvalidArgument(
                "command must start with /".into(),
            ));
        }
        let parts = tokenize(&line[1..])?;
        let name = parts.first().ok_or_else(|| {
            SlashError::InvalidArgument("command name is required".into())
        })?;
        let args = parts[1..].to_vec();

        // 先检查插件
        if let Some(command) = self.plugins.get(name) {
            return Ok(ParsedCommand {
                name: name.clone(),
                args,
                handler: CommandHandler::Plugin(command.clone()),
            });
        }

        // 再检查内置命令
        let invocation = self.builtin.parse(line).map_err(|e| {
            SlashError::NotFound(format!("unknown command /{name}: {e}"))
        })?;

        let route = invocation.route;
        Ok(ParsedCommand {
            name: name.clone(),
            args,
            handler: CommandHandler::Builtin { invocation, route },
        })
    }

    /// 获取命令补全列表（三组：Slash 命令 / Tool 快捷入口 / Skill 快捷入口）
    ///
    /// 输入 `/` 时显示所有分组，默认选中 Slash 命令组。
    /// 输入 `/tool:` 或 `/skill:` 可快速切换到工具/技能组。
    pub fn complete(&self, prefix: &str, tools: &[String], skills: &[String]) -> Vec<String> {
        let prefix = prefix.trim_start_matches('/');
        let mut names: Vec<String> = Vec::new();

        // Group 1: Slash commands (default)
        let slash_prefix = prefix;
        let slash_commands: Vec<String> = self
            .builtin
            .help()
            .into_iter()
            .map(|d| d.name)
            .chain(self.plugins.keys().cloned())
            .filter(|name| name.starts_with(slash_prefix))
            .map(|name| format!("/{name}"))
            .collect();
        names.extend(slash_commands);

        // Group 2: Tool shortcuts (prefix: /tool:)
        if prefix.is_empty() || "tool".starts_with(prefix) || prefix.starts_with("tool:") {
            let tool_prefix = prefix.strip_prefix("tool:").unwrap_or("");
            for tool in tools {
                if tool.starts_with(tool_prefix) {
                    names.push(format!("/tool:{tool}"));
                }
            }
            // When user types just "/", show the tool: prefix as a discoverable entry point
            if prefix.is_empty() || prefix == "tool" {
                names.push("/tool:".to_string());
            }
        }

        // Group 3: Skill shortcuts (prefix: /skill:)
        if prefix.is_empty() || "skill".starts_with(prefix) || prefix.starts_with("skill:") {
            let skill_prefix = prefix.strip_prefix("skill:").unwrap_or("");
            for skill in skills {
                if skill.starts_with(skill_prefix) {
                    names.push(format!("/skill:{skill}"));
                }
            }
            if prefix.is_empty() || prefix == "skill" {
                names.push("/skill:".to_string());
            }
        }

        names.sort();
        names.dedup();
        names
    }

    /// 获取帮助列表
    pub fn help(&self) -> Vec<CommandMetadata> {
        let mut commands: Vec<CommandMetadata> = self
            .builtin
            .help()
            .into_iter()
            .map(|d| {
                let name = d.name.clone();
                CommandMetadata {
                    name: d.name,
                    display_name: name,
                    description: d.summary,
                    usage: d.usage,
                    category: SlashCategory::Agent, // 默认分类
                    min_args: d.minimum_arguments,
                    max_args: d.maximum_arguments,
                    read_only: d.route == InteractionCommandRoute::Agent,
                    async_exec: d.route == InteractionCommandRoute::Agent,
                }
            })
            .collect();
        commands.extend(self.plugins.values().map(|cmd| cmd.metadata()));
        commands.sort_by(|a, b| a.name.cmp(&b.name));
        commands
    }

    /// 执行命令
    pub async fn execute(&self, parsed: ParsedCommand, ctx: CommandContext) -> SlashResult<CommandOutput> {
        // 通知观察者
        for observer in &self.observers {
            observer.on_command_start(&ctx).await;
        }

        let result = match &parsed.handler {
            CommandHandler::Plugin(command) => {
                command.validate(&parsed.args).await?;
                command.execute(ctx.clone()).await
            }
            CommandHandler::Builtin { invocation, route } => {
                match route {
                    InteractionCommandRoute::Entry => {
                        let outcome = self.builtin.execute_entry(invocation).map_err(|e| {
                            SlashError::Execution(e.to_string())
                        })?;
                        match outcome {
                            Some(outcome) => {
                                let action = match outcome.action {
                                    InteractionEntryAction::None => CommandAction::None,
                                    InteractionEntryAction::NewSession => CommandAction::NewSession,
                                    InteractionEntryAction::ClearView => CommandAction::ClearView,
                                    InteractionEntryAction::Exit => CommandAction::Exit,
                                    InteractionEntryAction::Profile(_) => CommandAction::None,
                                };
                                Ok(CommandOutput {
                                    response: outcome.response,
                                    data: serde_json::Value::Null,
                                    action,
                                })
                            }
                            None => Err(SlashError::Internal("entry command returned no outcome".into())),
                        }
                    }
                    _ => {
                        // Runtime 和 Agent 路由的命令由外部处理
                        Err(SlashError::NotFound(format!(
                            "/{} must be handled by EnterpriseAgent",
                            parsed.name
                        )))
                    }
                }
            }
        };

        // 通知观察者
        match &result {
            Ok(output) => {
                for observer in &self.observers {
                    observer.on_command_success(&ctx, output).await;
                }
            }
            Err(error) => {
                for observer in &self.observers {
                    observer.on_command_failure(&ctx, error).await;
                }
            }
        }

        result
    }

    fn builtin_contains(&self, name: &str) -> bool {
        self.builtin
            .help()
            .iter()
            .any(|d| d.name == name)
    }
}

// ── ParsedCommand ──

/// 解析后的命令
pub struct ParsedCommand {
    pub name: String,
    pub args: Vec<String>,
    pub handler: CommandHandler,
}

/// 命令处理器
pub enum CommandHandler {
    /// 插件式命令
    Plugin(Arc<dyn SlashCommand>),
    /// 内置命令（通过 InteractionCommandRegistry）
    Builtin {
        invocation: InteractionCommandInvocation,
        route: InteractionCommandRoute,
    },
}

// ── Tokenizer ──

fn tokenize(value: &str) -> SlashResult<Vec<String>> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            if !current.is_empty() {
                output.push(std::mem::take(&mut current));
            }
        } else if character.is_control() {
            return Err(SlashError::InvalidArgument(
                "command contains control characters".into(),
            ));
        } else {
            current.push(character);
        }
    }
    if quoted || escaped {
        return Err(SlashError::InvalidArgument(
            "command has an unterminated quote".into(),
        ));
    }
    if !current.is_empty() {
        output.push(current);
    }
    Ok(output)
}

// ── NoopSlashCommandObserver ──

/// 无操作观察者（默认实现）
pub struct NoopSlashCommandObserver;

#[async_trait]
impl SlashCommandObserver for NoopSlashCommandObserver {
    async fn on_command_start(&self, _ctx: &CommandContext) {}
    async fn on_command_success(&self, _ctx: &CommandContext, _output: &CommandOutput) {}
    async fn on_command_failure(&self, _ctx: &CommandContext, _error: &SlashError) {}
}

// ── 命令模块导出 ──

pub mod agent_plugin;
pub mod commands;
pub mod p2_plugin;
pub mod society_plugin;