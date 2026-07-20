//! Agent Observability & Evaluation Runtime
//!
//! 提供 Agent 执行追踪、质量评估、基准测试、调试、回放、评分能力。
//!
//! # 架构
//!
//! ```text
//! TraceCollector         — 自动采集 Agent 执行事件
//! SqliteTraceStore       — SQLite 持久化存储
//! EvaluationEngine       — 多维度规则评分
//! BenchmarkEngine        — 内置基准测试任务
//! DebugEngine            — 失败根因分析
//! ReplayEngine           — 事件溯源回放
//! ```
//!
//! # 数据模型
//!
//! - AgentTrace: 一次 Agent 执行的全景追踪
//! - TraceStep: 单个步骤（规划/推理/工具调用/决策等）
//! - ToolExecution: 工具调用详情
//! - Evaluation: 多维度评分
//! - BenchmarkResult: 基准测试结果

use std::sync::Arc;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ── 数据模型 ──

/// Trace 步骤类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TraceType {
    Planning,
    Reasoning,
    Delegation,
    ToolCall,
    Observation,
    Decision,
    Reflection,
    Response,
}

impl TraceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Reasoning => "reasoning",
            Self::Delegation => "delegation",
            Self::ToolCall => "tool_call",
            Self::Observation => "observation",
            Self::Decision => "decision",
            Self::Reflection => "reflection",
            Self::Response => "response",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "planning" => Some(Self::Planning),
            "reasoning" => Some(Self::Reasoning),
            "delegation" => Some(Self::Delegation),
            "tool_call" => Some(Self::ToolCall),
            "observation" => Some(Self::Observation),
            "decision" => Some(Self::Decision),
            "reflection" => Some(Self::Reflection),
            "response" => Some(Self::Response),
            _ => None,
        }
    }
}

/// Agent 执行追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrace {
    pub trace_id: Uuid,
    pub session_id: Option<Uuid>,
    pub agent_id: String,
    pub goal: String,
    pub steps: Vec<TraceStep>,
    pub result: String,
    pub score: Option<f32>,
    pub success: bool,
    pub wall_duration_ms: u64,
    pub token_usage: u32,
    pub created_at: DateTime<Utc>,
}

/// Trace 步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub step_id: Uuid,
    pub trace_id: Uuid,
    pub step_index: u32,
    pub step_type: TraceType,
    pub agent_name: String,
    pub input: String,
    pub output: String,
    pub tool_name: Option<String>,
    pub duration_ms: u64,
    pub token_usage: u32,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 工具调用记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub execution_id: Uuid,
    pub trace_id: Uuid,
    pub step_id: Uuid,
    pub tool_name: String,
    pub arguments: Value,
    pub result: Value,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 评分维度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScoreDimension {
    Correctness,
    Safety,
    Efficiency,
    Maintainability,
}

impl ScoreDimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Safety => "safety",
            Self::Efficiency => "efficiency",
            Self::Maintainability => "maintainability",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "correctness" => Some(Self::Correctness),
            "safety" => Some(Self::Safety),
            "efficiency" => Some(Self::Efficiency),
            "maintainability" => Some(Self::Maintainability),
            _ => None,
        }
    }
}

/// 评分项目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreItem {
    pub dimension: ScoreDimension,
    pub score: f32,
    pub max_score: f32,
    pub reason: String,
}

/// 评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub evaluation_id: Uuid,
    pub trace_id: Uuid,
    pub criteria: Vec<ScoreItem>,
    pub overall: f32,
    pub feedback: String,
    pub created_at: DateTime<Utc>,
}

/// 基准测试结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub benchmark_id: Uuid,
    pub agent_id: String,
    pub task_name: String,
    pub task_category: String,
    pub success: bool,
    pub score: f32,
    pub cost_tokens: u32,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 基准测试统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub agent_id: String,
    pub total_tasks: u32,
    pub success_count: u32,
    pub average_score: f32,
    pub average_cost: f32,
    pub average_duration_ms: f64,
    pub results: Vec<BenchmarkResult>,
}

/// Agent 健康度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    pub agent_id: String,
    pub success_rate: f32,
    pub avg_score: f32,
    pub avg_cost_tokens: f32,
    pub avg_latency_ms: f64,
    pub total_traces: u32,
    pub recent_traces: u32,
}

// ── TraceCollector ──

/// Trace 采集器 — 自动采集 Agent 执行事件
pub struct TraceCollector {
    store: Arc<SqliteTraceStore>,
    current_trace: tokio::sync::Mutex<Option<ActiveTrace>>,
}

struct ActiveTrace {
    trace_id: Uuid,
    session_id: Option<Uuid>,
    agent_id: String,
    goal: String,
    steps: Vec<TraceStep>,
    result: String,
    success: bool,
    wall_duration_ms: u64,
    token_usage: u32,
    step_counter: u32,
    started_at: std::time::Instant,
}

impl TraceCollector {
    pub fn new(store: Arc<SqliteTraceStore>) -> Self {
        Self {
            store,
            current_trace: tokio::sync::Mutex::new(None),
        }
    }

    /// 开始追踪
    pub async fn start_trace(&self, session_id: Option<Uuid>, agent_id: &str, goal: &str) -> Uuid {
        let trace_id = Uuid::new_v4();
        let mut guard = self.current_trace.lock().await;
        *guard = Some(ActiveTrace {
            trace_id,
            session_id,
            agent_id: agent_id.to_string(),
            goal: goal.to_string(),
            steps: Vec::new(),
            result: String::new(),
            success: true,
            wall_duration_ms: 0,
            token_usage: 0,
            step_counter: 0,
            started_at: std::time::Instant::now(),
        });
        trace_id
    }

    /// 记录步骤
    pub async fn record_step(
        &self,
        step_type: TraceType,
        agent_name: &str,
        input: &str,
        output: &str,
        tool_name: Option<&str>,
        duration_ms: u64,
        token_usage: u32,
        error: Option<&str>,
    ) {
        let mut guard = self.current_trace.lock().await;
        if let Some(trace) = guard.as_mut() {
            trace.step_counter += 1;
            trace.token_usage = trace.token_usage.saturating_add(token_usage);
            trace.wall_duration_ms = trace
                .wall_duration_ms
                .saturating_add(duration_ms);
            trace.steps.push(TraceStep {
                step_id: Uuid::new_v4(),
                trace_id: trace.trace_id,
                step_index: trace.step_counter,
                step_type,
                agent_name: agent_name.to_string(),
                input: truncate(input, 4096),
                output: truncate(output, 4096),
                tool_name: tool_name.map(String::from),
                duration_ms,
                token_usage,
                error: error.map(String::from),
                created_at: Utc::now(),
            });
        }
    }

    /// 结束追踪并保存
    pub async fn finish_trace(&self, result: &str, success: bool) -> Option<AgentTrace> {
        let mut guard = self.current_trace.lock().await;
        if let Some(trace) = guard.take() {
            let trace_record = AgentTrace {
                trace_id: trace.trace_id,
                session_id: trace.session_id,
                agent_id: trace.agent_id,
                goal: trace.goal,
                steps: trace.steps.clone(),
                result: result.to_string(),
                score: None,
                success,
                wall_duration_ms: trace.wall_duration_ms,
                token_usage: trace.token_usage,
                created_at: Utc::now(),
            };
            // 异步保存
            let store = self.store.clone();
            let record = trace_record.clone();
            tokio::spawn(async move {
                let _ = store.save_trace(&record);
            });
            Some(trace_record)
        } else {
            None
        }
    }

    /// 获取当前 trace_id
    pub async fn current_trace_id(&self) -> Option<Uuid> {
        self.current_trace.lock().await.as_ref().map(|t| t.trace_id)
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}... [truncated {} bytes]", &text[..max], text.len() - max)
    }
}

// ── SQLite 存储 ──

/// SQLite Trace 存储
pub struct SqliteTraceStore {
    db: Mutex<rusqlite::Connection>,
}

impl SqliteTraceStore {
    /// 打开或创建 trace 数据库
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = rusqlite::Connection::open(path)
            .map_err(|e| format!("cannot open trace database: {e}"))?;
        let store = Self { db: Mutex::new(db) };
        store.init_tables()?;
        Ok(store)
    }

    fn init_tables(&self) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        db.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS agent_trace (
                    trace_id TEXT PRIMARY KEY,
                    session_id TEXT,
                    agent_id TEXT NOT NULL,
                    goal TEXT NOT NULL,
                    result TEXT NOT NULL,
                    score REAL,
                    success INTEGER NOT NULL DEFAULT 1,
                    wall_duration_ms INTEGER NOT NULL DEFAULT 0,
                    token_usage INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS trace_step (
                    step_id TEXT PRIMARY KEY,
                    trace_id TEXT NOT NULL REFERENCES agent_trace(trace_id),
                    step_index INTEGER NOT NULL,
                    step_type TEXT NOT NULL,
                    agent_name TEXT NOT NULL,
                    input TEXT NOT NULL DEFAULT '',
                    output TEXT NOT NULL DEFAULT '',
                    tool_name TEXT,
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    token_usage INTEGER NOT NULL DEFAULT 0,
                    error TEXT,
                    created_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_trace_step_trace_id ON trace_step(trace_id);
                CREATE INDEX IF NOT EXISTS idx_trace_step_type ON trace_step(step_type);

                CREATE TABLE IF NOT EXISTS tool_execution (
                    execution_id TEXT PRIMARY KEY,
                    trace_id TEXT NOT NULL REFERENCES agent_trace(trace_id),
                    step_id TEXT NOT NULL REFERENCES trace_step(step_id),
                    tool_name TEXT NOT NULL,
                    arguments TEXT NOT NULL DEFAULT '{}',
                    result TEXT NOT NULL DEFAULT '{}',
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    success INTEGER NOT NULL DEFAULT 1,
                    error TEXT,
                    created_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_tool_exec_trace_id ON tool_execution(trace_id);

                CREATE TABLE IF NOT EXISTS evaluation (
                    evaluation_id TEXT PRIMARY KEY,
                    trace_id TEXT NOT NULL REFERENCES agent_trace(trace_id),
                    criteria TEXT NOT NULL DEFAULT '[]',
                    overall REAL NOT NULL DEFAULT 0,
                    feedback TEXT NOT NULL DEFAULT '',
                    created_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_evaluation_trace_id ON evaluation(trace_id);

                CREATE TABLE IF NOT EXISTS benchmark_result (
                    benchmark_id TEXT PRIMARY KEY,
                    agent_id TEXT NOT NULL,
                    task_name TEXT NOT NULL,
                    task_category TEXT NOT NULL,
                    success INTEGER NOT NULL DEFAULT 0,
                    score REAL NOT NULL DEFAULT 0,
                    cost_tokens INTEGER NOT NULL DEFAULT 0,
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    error TEXT,
                    created_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_benchmark_agent ON benchmark_result(agent_id);
                ",
            )
            .map_err(|e| format!("cannot initialize trace tables: {e}"))?;
        Ok(())
    }

    /// 保存 Trace
    pub fn save_trace(&self, trace: &AgentTrace) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        db.execute(
            "INSERT OR REPLACE INTO agent_trace (trace_id, session_id, agent_id, goal, result, score, success, wall_duration_ms, token_usage, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                trace.trace_id.to_string(),
                trace.session_id.map(|id| id.to_string()),
                trace.agent_id,
                trace.goal,
                trace.result,
                trace.score,
                trace.success as i32,
                trace.wall_duration_ms,
                trace.token_usage,
                trace.created_at.to_rfc3339(),
            ],
        ).map_err(|e| format!("cannot insert trace: {e}"))?;

        for step in &trace.steps {
            db.execute(
                "INSERT OR REPLACE INTO trace_step (step_id, trace_id, step_index, step_type, agent_name, input, output, tool_name, duration_ms, token_usage, error, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    step.step_id.to_string(),
                    step.trace_id.to_string(),
                    step.step_index,
                    step.step_type.as_str(),
                    step.agent_name,
                    step.input,
                    step.output,
                    step.tool_name,
                    step.duration_ms,
                    step.token_usage,
                    step.error,
                    step.created_at.to_rfc3339(),
                ],
            ).map_err(|e| format!("cannot insert trace step: {e}"))?;
        }

        Ok(())
    }

    /// 保存工具调用记录
    pub fn save_tool_execution(&self, exec: &ToolExecution) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        db.execute(
            "INSERT OR REPLACE INTO tool_execution (execution_id, trace_id, step_id, tool_name, arguments, result, duration_ms, success, error, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                exec.execution_id.to_string(),
                exec.trace_id.to_string(),
                exec.step_id.to_string(),
                exec.tool_name,
                exec.arguments.to_string(),
                exec.result.to_string(),
                exec.duration_ms,
                exec.success as i32,
                exec.error,
                exec.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| format!("cannot insert tool execution: {e}"))?;
        Ok(())
    }

    /// 保存评估结果
    pub fn save_evaluation(&self, eval: &Evaluation) -> Result<(), String> {
        let criteria =
            serde_json::to_string(&eval.criteria).map_err(|e| format!("serialize criteria: {e}"))?;
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        db.execute(
            "INSERT OR REPLACE INTO evaluation (evaluation_id, trace_id, criteria, overall, feedback, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                eval.evaluation_id.to_string(),
                eval.trace_id.to_string(),
                criteria,
                eval.overall,
                eval.feedback,
                eval.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| format!("cannot insert evaluation: {e}"))?;
        Ok(())
    }

    /// 保存基准测试结果
    pub fn save_benchmark_result(&self, result: &BenchmarkResult) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        db.execute(
            "INSERT OR REPLACE INTO benchmark_result (benchmark_id, agent_id, task_name, task_category, success, score, cost_tokens, duration_ms, error, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                result.benchmark_id.to_string(),
                result.agent_id,
                result.task_name,
                result.task_category,
                result.success as i32,
                result.score,
                result.cost_tokens,
                result.duration_ms,
                result.error,
                result.created_at.to_rfc3339(),
            ],
        )
        .map_err(|e| format!("cannot insert benchmark result: {e}"))?;
        Ok(())
    }

    /// 查询 Trace 列表
    pub fn list_traces(&self, limit: u32, offset: u32) -> Result<Vec<AgentTrace>, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db
            .prepare(
                "SELECT trace_id, session_id, agent_id, goal, result, score, success, wall_duration_ms, token_usage, created_at FROM agent_trace ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![limit, offset], |row| {
                let trace_id: String = row.get(0)?;
                let session_id: Option<String> = row.get(1)?;
                let agent_id: String = row.get(2)?;
                let goal: String = row.get(3)?;
                let result: String = row.get(4)?;
                let score: Option<f32> = row.get(5)?;
                let success: i32 = row.get(6)?;
                let wall_duration_ms: u64 = row.get(7)?;
                let token_usage: u32 = row.get(8)?;
                let created_at: String = row.get(9)?;
                Ok(AgentTrace {
                    trace_id: Uuid::parse_str(&trace_id).unwrap_or_default(),
                    session_id: session_id.and_then(|s| Uuid::parse_str(&s).ok()),
                    agent_id,
                    goal,
                    steps: Vec::new(),
                    result,
                    score,
                    success: success != 0,
                    wall_duration_ms,
                    token_usage,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| format!("cannot query traces: {e}"))?;

        let mut traces = Vec::new();
        for row in rows {
            if let Ok(trace) = row {
                traces.push(trace);
            }
        }
        Ok(traces)
    }

    /// 查询单个 Trace（含步骤）
    pub fn get_trace(&self, trace_id: Uuid) -> Result<Option<AgentTrace>, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db
            .prepare(
                "SELECT trace_id, session_id, agent_id, goal, result, score, success, wall_duration_ms, token_usage, created_at FROM agent_trace WHERE trace_id = ?1",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let mut rows = stmt
            .query_map(rusqlite::params![trace_id.to_string()], |row| {
                let trace_id: String = row.get(0)?;
                let session_id: Option<String> = row.get(1)?;
                let agent_id: String = row.get(2)?;
                let goal: String = row.get(3)?;
                let result: String = row.get(4)?;
                let score: Option<f32> = row.get(5)?;
                let success: i32 = row.get(6)?;
                let wall_duration_ms: u64 = row.get(7)?;
                let token_usage: u32 = row.get(8)?;
                let created_at: String = row.get(9)?;
                Ok(AgentTrace {
                    trace_id: Uuid::parse_str(&trace_id).unwrap_or_default(),
                    session_id: session_id.and_then(|s| Uuid::parse_str(&s).ok()),
                    agent_id,
                    goal,
                    steps: Vec::new(),
                    result,
                    score,
                    success: success != 0,
                    wall_duration_ms,
                    token_usage,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| format!("cannot query trace: {e}"))?;

        if let Some(row) = rows.next() {
            let mut trace = row.map_err(|e| format!("cannot read trace: {e}"))?;
            // 加载步骤
            trace.steps = self.get_trace_steps(trace_id)?;
            Ok(Some(trace))
        } else {
            Ok(None)
        }
    }

    fn get_trace_steps(&self, trace_id: Uuid) -> Result<Vec<TraceStep>, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db.prepare(
                "SELECT step_id, trace_id, step_index, step_type, agent_name, input, output, tool_name, duration_ms, token_usage, error, created_at FROM trace_step WHERE trace_id = ?1 ORDER BY step_index ASC",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![trace_id.to_string()], |row| {
                let step_id: String = row.get(0)?;
                let trace_id: String = row.get(1)?;
                let step_index: u32 = row.get(2)?;
                let step_type: String = row.get(3)?;
                let agent_name: String = row.get(4)?;
                let input: String = row.get(5)?;
                let output: String = row.get(6)?;
                let tool_name: Option<String> = row.get(7)?;
                let duration_ms: u64 = row.get(8)?;
                let token_usage: u32 = row.get(9)?;
                let error: Option<String> = row.get(10)?;
                let created_at: String = row.get(11)?;
                Ok(TraceStep {
                    step_id: Uuid::parse_str(&step_id).unwrap_or_default(),
                    trace_id: Uuid::parse_str(&trace_id).unwrap_or_default(),
                    step_index,
                    step_type: TraceType::from_str(&step_type).unwrap_or(TraceType::Reasoning),
                    agent_name,
                    input,
                    output,
                    tool_name,
                    duration_ms,
                    token_usage,
                    error,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| format!("cannot query steps: {e}"))?;

        let mut steps = Vec::new();
        for row in rows {
            if let Ok(step) = row {
                steps.push(step);
            }
        }
        Ok(steps)
    }

    /// 查询最近 N 次 Trace 的评分
    pub fn recent_scores(&self, agent_id: &str, limit: u32) -> Result<Vec<f32>, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db.prepare(
                "SELECT score FROM agent_trace WHERE agent_id = ?1 AND score IS NOT NULL ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![agent_id, limit], |row| {
                let score: f32 = row.get(0)?;
                Ok(score)
            })
            .map_err(|e| format!("cannot query scores: {e}"))?;

        let mut scores = Vec::new();
        for row in rows {
            if let Ok(score) = row {
                scores.push(score);
            }
        }
        Ok(scores)
    }

    /// 查询 Agent 统计
    pub fn agent_stats(&self, agent_id: &str) -> Result<AgentHealth, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db.prepare(
                "SELECT COUNT(*), COALESCE(AVG(CASE WHEN success=1 THEN 100.0 ELSE 0.0 END), 0), COALESCE(AVG(score), 0), COALESCE(AVG(token_usage), 0), COALESCE(AVG(wall_duration_ms), 0) FROM agent_trace WHERE agent_id = ?1",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let stats = stmt
            .query_row(rusqlite::params![agent_id], |row| {
                let total: u32 = row.get(0)?;
                let success_rate: f32 = row.get(1)?;
                let avg_score: f32 = row.get(2)?;
                let avg_cost: f32 = row.get(3)?;
                let avg_latency: f64 = row.get(4)?;
                Ok((total, success_rate, avg_score, avg_cost, avg_latency))
            })
            .map_err(|e| format!("cannot query stats: {e}"))?;

        // 最近 24 小时追踪数
        let recent = db
            .query_row(
                "SELECT COUNT(*) FROM agent_trace WHERE agent_id = ?1 AND created_at > datetime('now', '-1 day')",
                rusqlite::params![agent_id],
                |row| {
                    let count: u32 = row.get(0)?;
                    Ok(count)
                },
            )
            .map_err(|e| format!("cannot query recent count: {e}"))?;

        Ok(AgentHealth {
            agent_id: agent_id.to_string(),
            success_rate: stats.1,
            avg_score: stats.2,
            avg_cost_tokens: stats.3,
            avg_latency_ms: stats.4,
            total_traces: stats.0,
            recent_traces: recent,
        })
    }

    /// 查询基准测试结果
    pub fn list_benchmark_results(&self, agent_id: &str) -> Result<Vec<BenchmarkResult>, String> {
        let db = self.db.lock().map_err(|e| format!("lock error: {e}"))?;
        let mut stmt = db.prepare(
                "SELECT benchmark_id, agent_id, task_name, task_category, success, score, cost_tokens, duration_ms, error, created_at FROM benchmark_result WHERE agent_id = ?1 ORDER BY created_at DESC",
            )
            .map_err(|e| format!("cannot prepare query: {e}"))?;

        let rows = stmt
            .query_map(rusqlite::params![agent_id], |row| {
                let benchmark_id: String = row.get(0)?;
                let agent_id: String = row.get(1)?;
                let task_name: String = row.get(2)?;
                let task_category: String = row.get(3)?;
                let success: i32 = row.get(4)?;
                let score: f32 = row.get(5)?;
                let cost_tokens: u32 = row.get(6)?;
                let duration_ms: u64 = row.get(7)?;
                let error: Option<String> = row.get(8)?;
                let created_at: String = row.get(9)?;
                Ok(BenchmarkResult {
                    benchmark_id: Uuid::parse_str(&benchmark_id).unwrap_or_default(),
                    agent_id,
                    task_name,
                    task_category,
                    success: success != 0,
                    score,
                    cost_tokens,
                    duration_ms,
                    error,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_default(),
                })
            })
            .map_err(|e| format!("cannot query benchmark results: {e}"))?;

        let mut results = Vec::new();
        for row in rows {
            if let Ok(result) = row {
                results.push(result);
            }
        }
        Ok(results)
    }
}

// ── EvaluationEngine ──

/// 多维度规则评分引擎
pub struct EvaluationEngine;

impl EvaluationEngine {
    /// 基于 Trace 数据进行评分
    pub fn evaluate(trace: &AgentTrace) -> Evaluation {
        let criteria = vec![
            Self::score_correctness(trace),
            Self::score_safety(trace),
            Self::score_efficiency(trace),
            Self::score_maintainability(trace),
        ];

        let overall = if criteria.is_empty() {
            0.0
        } else {
            criteria.iter().map(|c| c.score).sum::<f32>() / criteria.len() as f32 * 10.0
        };

        let feedback = Self::generate_feedback(&criteria, overall);

        Evaluation {
            evaluation_id: Uuid::new_v4(),
            trace_id: trace.trace_id,
            criteria,
            overall: (overall * 10.0).round() / 10.0, // 保留一位小数
            feedback,
            created_at: Utc::now(),
        }
    }

    /// 正确性评分：基于是否有错误、成功率
    fn score_correctness(trace: &AgentTrace) -> ScoreItem {
        let mut score = 8.0_f32;

        // 执行失败扣分
        if !trace.success {
            score -= 4.0;
        }

        // 步骤中有错误扣分
        let error_count = trace
            .steps
            .iter()
            .filter(|s| s.error.is_some())
            .count();
        score -= error_count as f32 * 1.5;

        // 有工具调用失败扣分
        let tool_failures = trace
            .steps
            .iter()
            .filter(|s| s.step_type == TraceType::ToolCall && s.error.is_some())
            .count();
        score -= tool_failures as f32 * 1.0;

        ScoreItem {
            dimension: ScoreDimension::Correctness,
            score: score.clamp(0.0, 10.0),
            max_score: 10.0,
            reason: format!(
                "success={}, errors={}, tool_failures={}",
                trace.success, error_count, tool_failures
            ),
        }
    }

    /// 安全性评分：基于工具调用和操作类型
    fn score_safety(trace: &AgentTrace) -> ScoreItem {
        let mut score = 9.0_f32;

        // 检查是否有文件写入操作（视为高风险）
        let write_ops = trace
            .steps
            .iter()
            .filter(|s| {
                s.tool_name
                    .as_deref()
                    .map(|t| t.contains("write") || t.contains("delete") || t.contains("exec"))
                    .unwrap_or(false)
            })
            .count();
        if write_ops > 0 {
            score -= 1.0;
        }

        // 检查是否有执行命令操作
        let exec_ops = trace
            .steps
            .iter()
            .filter(|s| {
                s.tool_name
                    .as_deref()
                    .map(|t| t.contains("exec") || t.contains("shell") || t.contains("bash"))
                    .unwrap_or(false)
            })
            .count();
        if exec_ops > 0 {
            score -= 1.5;
        }

        ScoreItem {
            dimension: ScoreDimension::Safety,
            score: score.clamp(0.0, 10.0),
            max_score: 10.0,
            reason: format!("write_ops={}, exec_ops={}", write_ops, exec_ops),
        }
    }

    /// 效率评分：基于耗时和 token 使用
    fn score_efficiency(trace: &AgentTrace) -> ScoreItem {
        let mut score = 8.0_f32;

        // 耗时过长扣分
        if trace.wall_duration_ms > 120_000 {
            // > 2 分钟
            score -= 3.0;
        } else if trace.wall_duration_ms > 60_000 {
            score -= 1.5;
        }

        // 步骤过多扣分
        if trace.steps.len() > 20 {
            score -= 2.0;
        } else if trace.steps.len() > 10 {
            score -= 1.0;
        }

        // token 使用过多扣分
        if trace.token_usage > 10_000 {
            score -= 2.0;
        } else if trace.token_usage > 5_000 {
            score -= 1.0;
        }

        ScoreItem {
            dimension: ScoreDimension::Efficiency,
            score: score.clamp(0.0, 10.0),
            max_score: 10.0,
            reason: format!(
                "duration={}ms, steps={}, tokens={}",
                trace.wall_duration_ms,
                trace.steps.len(),
                trace.token_usage
            ),
        }
    }

    /// 可维护性评分：基于步骤结构和可读性
    fn score_maintainability(trace: &AgentTrace) -> ScoreItem {
        let mut score = 7.0_f32;

        // 有反思步骤加分（说明 Agent 在自我修正）
        let reflections = trace
            .steps
            .iter()
            .filter(|s| s.step_type == TraceType::Reflection)
            .count();
        if reflections > 0 {
            score += 1.0;
        }

        // 有规划步骤加分
        let planning = trace
            .steps
            .iter()
            .filter(|s| s.step_type == TraceType::Planning)
            .count();
        if planning > 0 {
            score += 1.0;
        }

        // 步骤太少可能不够完整
        if trace.steps.len() < 3 {
            score -= 1.0;
        }

        // 有错误步骤扣分
        let error_steps = trace
            .steps
            .iter()
            .filter(|s| s.error.is_some())
            .count();
        if error_steps > 2 {
            score -= 2.0;
        }

        ScoreItem {
            dimension: ScoreDimension::Maintainability,
            score: score.clamp(0.0, 10.0),
            max_score: 10.0,
            reason: format!(
                "reflections={}, planning={}, total_steps={}, errors={}",
                reflections,
                planning,
                trace.steps.len(),
                error_steps
            ),
        }
    }

    fn generate_feedback(criteria: &[ScoreItem], overall: f32) -> String {
        let mut feedback = String::new();

        if overall >= 8.0 {
            feedback.push_str("Overall quality is high. ");
        } else if overall >= 5.0 {
            feedback.push_str("Overall quality is acceptable. ");
        } else {
            feedback.push_str("Overall quality needs improvement. ");
        }

        for c in criteria {
            let normalized = c.score / c.max_score;
            if normalized < 0.5 {
                feedback.push_str(&format!(
                    "{} is low ({:.1}/10). ",
                    c.dimension.as_str(),
                    c.score
                ));
            } else if normalized >= 0.8 {
                feedback.push_str(&format!(
                    "{} is good ({:.1}/10). ",
                    c.dimension.as_str(),
                    c.score
                ));
            }
        }

        feedback
    }
}

// ── BenchmarkEngine ──

/// 内置基准测试任务
#[derive(Debug, Clone)]
pub struct BenchmarkTask {
    pub name: String,
    pub category: String,
    pub description: String,
    pub prompt: String,
    pub expected_success_criteria: Vec<String>,
}

/// 基准测试引擎
pub struct BenchmarkEngine;

impl BenchmarkEngine {
    /// 获取内置任务集
    pub fn builtin_tasks() -> Vec<BenchmarkTask> {
        vec![
            BenchmarkTask {
                name: "fix-bug".into(),
                category: "coding".into(),
                description: "Fix a bug in a simple function".into(),
                prompt: "Fix the bug in this function: fn add(a: i32, b: i32) -> i32 { a - b }".into(),
                expected_success_criteria: vec!["correct implementation".into()],
            },
            BenchmarkTask {
                name: "generate-doc".into(),
                category: "documentation".into(),
                description: "Generate documentation for a function".into(),
                prompt: "Write documentation for a REST API endpoint that returns user data".into(),
                expected_success_criteria: vec!["doc generated".into()],
            },
            BenchmarkTask {
                name: "analyze-arch".into(),
                category: "architecture".into(),
                description: "Analyze a simple architecture".into(),
                prompt: "Analyze the pros and cons of microservices vs monolith architecture".into(),
                expected_success_criteria: vec!["analysis provided".into()],
            },
            BenchmarkTask {
                name: "security-review".into(),
                category: "security".into(),
                description: "Review code for security issues".into(),
                prompt: "Review this code for security issues: \nfn get_user(id: &str) -> String { format!(\"SELECT * FROM users WHERE id = '{id}'\") }".into(),
                expected_success_criteria: vec!["sql injection identified".into()],
            },
            BenchmarkTask {
                name: "generate-test".into(),
                category: "testing".into(),
                description: "Generate unit tests for a function".into(),
                prompt: "Generate unit tests for a function that validates email addresses".into(),
                expected_success_criteria: vec!["tests generated".into()],
            },
        ]
    }

    /// 计算基准测试统计
    pub fn summarize(agent_id: &str, results: &[BenchmarkResult]) -> BenchmarkSummary {
        let total = results.len() as u32;
        let success = results.iter().filter(|r| r.success).count() as u32;
        let avg_score = if total > 0 {
            results.iter().map(|r| r.score).sum::<f32>() / total as f32
        } else {
            0.0
        };
        let avg_cost = if total > 0 {
            results.iter().map(|r| r.cost_tokens as f32).sum::<f32>() / total as f32
        } else {
            0.0
        };
        let avg_duration = if total > 0 {
            results.iter().map(|r| r.duration_ms as f64).sum::<f64>() / total as f64
        } else {
            0.0
        };

        BenchmarkSummary {
            agent_id: agent_id.to_string(),
            total_tasks: total,
            success_count: success,
            average_score: (avg_score * 10.0).round() / 10.0,
            average_cost: (avg_cost * 10.0).round() / 10.0,
            average_duration_ms: (avg_duration * 10.0).round() / 10.0,
            results: results.to_vec(),
        }
    }
}

// ── DebugEngine ──

/// 调试引擎 — 分析失败根因
pub struct DebugEngine;

impl DebugEngine {
    /// 分析失败根因
    pub fn analyze(trace: &AgentTrace) -> DebugAnalysis {
        let mut failure_points = Vec::new();
        let mut root_causes = Vec::new();
        let mut recommendations = Vec::new();

        for step in &trace.steps {
            if let Some(ref error) = step.error {
                failure_points.push(DebugFailurePoint {
                    step_index: step.step_index,
                    agent_name: step.agent_name.clone(),
                    step_type: step.step_type,
                    problem: error.clone(),
                });

                // 根因分析
                let (cause, rec) = Self::classify_error(error, &step.step_type, step.tool_name.as_deref());
                if !root_causes.contains(&cause) {
                    root_causes.push(cause);
                }
                if !recommendations.contains(&rec) {
                    recommendations.push(rec);
                }
            }
        }

        // 如果没有失败点，检查是否有警告信号
        if failure_points.is_empty() {
            if trace.steps.is_empty() {
                root_causes.push("No steps recorded".into());
                recommendations.push("Ensure the Agent executed at least one step".into());
            } else if trace.steps.len() == 1 {
                root_causes.push("Only one step — insufficient exploration".into());
                recommendations.push("Increase the model's reasoning depth or tool access".into());
            }
        }

        DebugAnalysis {
            trace_id: trace.trace_id,
            success: trace.success,
            total_steps: trace.steps.len() as u32,
            failure_points,
            root_causes,
            recommendations,
        }
    }

    fn classify_error(error: &str, step_type: &TraceType, tool_name: Option<&str>) -> (String, String) {
        let error_lower = error.to_lowercase();

        if error_lower.contains("permission") || error_lower.contains("denied") || error_lower.contains("forbidden") {
            ("Permission denied — operation blocked by policy".into(), "Check tool permissions and approval settings".into())
        } else if error_lower.contains("not found") || error_lower.contains("does not exist") {
            ("Resource not found — target file or tool missing".into(), "Verify the resource path or tool name".into())
        } else if error_lower.contains("timeout") || error_lower.contains("timed out") {
            ("Operation timed out — took too long to complete".into(), "Reduce scope or increase timeout limits".into())
        } else if error_lower.contains("invalid") || error_lower.contains("bad request") {
            ("Invalid input or request — parameters rejected".into(), "Check parameter format and constraints".into())
        } else if error_lower.contains("rate") || error_lower.contains("limit") || error_lower.contains("quota") {
            ("Rate limit or quota exceeded".into(), "Reduce request frequency or increase quota".into())
        } else if error_lower.contains("connection") || error_lower.contains("network") {
            ("Network or connection error".into(), "Check network connectivity and endpoint availability".into())
        } else {
            match step_type {
                TraceType::ToolCall => {
                    let tool = tool_name.unwrap_or("unknown");
                    (format!("Tool {tool} failed: {error}"), format!("Review the {tool} tool's implementation and error handling"))
                }
                _ => {
                    ("Unknown execution error".into(), "Enable verbose logging and re-run to capture details".into())
                }
            }
        }
    }
}

/// 调试分析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugAnalysis {
    pub trace_id: Uuid,
    pub success: bool,
    pub total_steps: u32,
    pub failure_points: Vec<DebugFailurePoint>,
    pub root_causes: Vec<String>,
    pub recommendations: Vec<String>,
}

/// 失败点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugFailurePoint {
    pub step_index: u32,
    pub agent_name: String,
    pub step_type: TraceType,
    pub problem: String,
}

// ── ReplayEngine ──

/// 回放引擎 — 基于事件溯源
pub struct ReplayEngine;

impl ReplayEngine {
    /// 构建回放报告
    pub fn build_replay(trace: &AgentTrace) -> ReplayReport {
        let mut events = Vec::new();

        for step in &trace.steps {
            let event = ReplayEvent {
                sequence: step.step_index,
                event_type: step.step_type.as_str().to_string(),
                agent: step.agent_name.clone(),
                input: step.input.clone(),
                output: step.output.clone(),
                tool: step.tool_name.clone(),
                duration_ms: step.duration_ms,
                error: step.error.clone(),
            };
            events.push(event);
        }

        // 检测差异点（如果有错误则标记）
        let differences: Vec<u32> = trace
            .steps
            .iter()
            .filter(|s| s.error.is_some())
            .map(|s| s.step_index)
            .collect();

        ReplayReport {
            trace_id: trace.trace_id,
            total_events: events.len() as u32,
            events,
            differences,
            result: trace.result.clone(),
            success: trace.success,
        }
    }
}

/// 回放事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub sequence: u32,
    pub event_type: String,
    pub agent: String,
    pub input: String,
    pub output: String,
    pub tool: Option<String>,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// 回放报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub trace_id: Uuid,
    pub total_events: u32,
    pub events: Vec<ReplayEvent>,
    pub differences: Vec<u32>,
    pub result: String,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trace() -> AgentTrace {
        AgentTrace {
            trace_id: Uuid::new_v4(),
            session_id: Some(Uuid::new_v4()),
            agent_id: "test-agent".into(),
            goal: "Fix the bug".into(),
            steps: vec![
                TraceStep {
                    step_id: Uuid::new_v4(),
                    trace_id: Uuid::new_v4(),
                    step_index: 1,
                    step_type: TraceType::Planning,
                    agent_name: "planner".into(),
                    input: "Fix the bug".into(),
                    output: "Plan created".into(),
                    tool_name: None,
                    duration_ms: 100,
                    token_usage: 50,
                    error: None,
                    created_at: Utc::now(),
                },
                TraceStep {
                    step_id: Uuid::new_v4(),
                    trace_id: Uuid::new_v4(),
                    step_index: 2,
                    step_type: TraceType::ToolCall,
                    agent_name: "coder".into(),
                    input: "read file".into(),
                    output: "file content".into(),
                    tool_name: Some("read_file".into()),
                    duration_ms: 200,
                    token_usage: 100,
                    error: None,
                    created_at: Utc::now(),
                },
                TraceStep {
                    step_id: Uuid::new_v4(),
                    trace_id: Uuid::new_v4(),
                    step_index: 3,
                    step_type: TraceType::ToolCall,
                    agent_name: "coder".into(),
                    input: "write file".into(),
                    output: "file written".into(),
                    tool_name: Some("write_file".into()),
                    duration_ms: 150,
                    token_usage: 80,
                    error: None,
                    created_at: Utc::now(),
                },
                TraceStep {
                    step_id: Uuid::new_v4(),
                    trace_id: Uuid::new_v4(),
                    step_index: 4,
                    step_type: TraceType::Response,
                    agent_name: "assistant".into(),
                    input: "".into(),
                    output: "Bug fixed".into(),
                    tool_name: None,
                    duration_ms: 50,
                    token_usage: 30,
                    error: None,
                    created_at: Utc::now(),
                },
            ],
            result: "Bug fixed successfully".into(),
            score: None,
            success: true,
            wall_duration_ms: 500,
            token_usage: 260,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_evaluation_engine() {
        let trace = sample_trace();
        let eval = EvaluationEngine::evaluate(&trace);
        assert_eq!(eval.criteria.len(), 4);
        assert!(eval.overall > 0.0);
        assert!(!eval.feedback.is_empty());
    }

    #[test]
    fn test_debug_engine_success() {
        let trace = sample_trace();
        let analysis = DebugEngine::analyze(&trace);
        assert!(analysis.failure_points.is_empty());
        assert!(analysis.success);
    }

    #[test]
    fn test_debug_engine_failure() {
        let mut trace = sample_trace();
        trace.success = false;
        trace.steps[1].error = Some("Permission denied".into());
        let analysis = DebugEngine::analyze(&trace);
        assert!(!analysis.failure_points.is_empty());
        assert!(analysis.root_causes.iter().any(|c| c.contains("Permission")));
    }

    #[test]
    fn test_replay_engine() {
        let trace = sample_trace();
        let report = ReplayEngine::build_replay(&trace);
        assert_eq!(report.total_events, 4);
        assert!(report.differences.is_empty());
    }

    #[test]
    fn test_benchmark_tasks() {
        let tasks = BenchmarkEngine::builtin_tasks();
        assert_eq!(tasks.len(), 5);
    }

    #[test]
    fn test_benchmark_summary() {
        let results = vec![
            BenchmarkResult {
                benchmark_id: Uuid::new_v4(),
                agent_id: "test".into(),
                task_name: "task1".into(),
                task_category: "coding".into(),
                success: true,
                score: 8.0,
                cost_tokens: 1000,
                duration_ms: 5000,
                error: None,
                created_at: Utc::now(),
            },
            BenchmarkResult {
                benchmark_id: Uuid::new_v4(),
                agent_id: "test".into(),
                task_name: "task2".into(),
                task_category: "coding".into(),
                success: false,
                score: 4.0,
                cost_tokens: 2000,
                duration_ms: 10000,
                error: Some("timeout".into()),
                created_at: Utc::now(),
            },
        ];
        let summary = BenchmarkEngine::summarize("test", &results);
        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.success_count, 1);
        assert!((summary.average_score - 6.0).abs() < 0.01);
    }

    #[test]
    fn test_trace_collector() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.db");
        let store = Arc::new(SqliteTraceStore::open(&path).unwrap());
        let collector = TraceCollector::new(store);

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let trace_id = collector.start_trace(None, "test-agent", "Test goal").await;
            collector
                .record_step(TraceType::Planning, "planner", "input", "output", None, 100, 50, None)
                .await;
            collector
                .record_step(
                    TraceType::ToolCall,
                    "coder",
                    "read input",
                    "file content",
                    Some("read_file"),
                    200,
                    100,
                    None,
                )
                .await;
            let result = collector.finish_trace("Done", true).await;
            assert!(result.is_some());
            let trace = result.unwrap();
            assert_eq!(trace.agent_id, "test-agent");
            assert_eq!(trace.steps.len(), 2);
        });
    }
}