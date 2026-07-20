//! Cognitive Runtime — Agent 认知引擎
//!
//! 提供认知相关的 Prompt 模板、结构化输出格式、ADR 生成等能力。
//! 所有认知命令（/reason, /think, /hypothesis, /critic, /reflect, /decision）
//! 通过 Agent 路由调用模型，这里提供 Prompt 构建和结果后处理。
//!
//! # 设计原则
//!
//! - 不暴露 Chain of Thought（CoT）
//! - 输出 Reasoning Summary、Evidence、Decision、Confidence 等结构化摘要
//! - /decision 自动生成 ADR 文件到 docs/adr/

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// 认知命令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CognitiveCommand {
    /// 问题分析
    Reason,
    /// 复杂任务分析
    Think,
    /// 假设管理
    Hypothesis,
    /// 自我批判
    Critic,
    /// 反思学习
    Reflect,
    /// 决策记录
    Decision,
}

impl CognitiveCommand {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reason => "reason",
            Self::Think => "think",
            Self::Hypothesis => "hypothesis",
            Self::Critic => "critic",
            Self::Reflect => "reflect",
            Self::Decision => "decision",
        }
    }

    /// 是否为只读命令（/reflect 和 /decision 有副作用）
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::Reason | Self::Think | Self::Hypothesis | Self::Critic)
    }

    /// 构建模型提示词
    pub fn model_prompt(&self, args: &[String]) -> String {
        let arguments = if args.is_empty() {
            String::new()
        } else {
            serde_json::to_string(args).unwrap_or_else(|_| args.join(" "))
        };

        let (instruction, output_format) = self.prompt_template();

        format!(
            r#"Execute the built-in /{name} command.

Arguments: {arguments}

## Instruction
{instruction}

## Output Format
{output_format}

## Rules
- Do NOT output your internal chain-of-thought or reasoning process.
- Only output the structured result as specified above.
- Be concise and actionable.
"#,
            name = self.as_str(),
            arguments = arguments,
            instruction = instruction,
            output_format = output_format,
        )
    }

    fn prompt_template(&self) -> (&'static str, &'static str) {
        match self {
            Self::Reason => (
                "Analyze the problem. Collect evidence, identify possible causes, and produce a reasoning summary. Do NOT output internal chain-of-thought.",
                r#"```reasoning
Problem: <problem statement>

Evidence:
1. <evidence 1>
2. <evidence 2>

Possible Causes:
A. <cause A>
B. <cause B>

Confidence: <0.0-1.0>
```"#,
            ),
            Self::Think => (
                "Analyze the complex task. Identify constraints, generate options, evaluate trade-offs, and recommend a solution.",
                r#"```thinking
Goal: <task goal>

Constraints:
- <constraint 1>
- <constraint 2>

Options:
A. <option A> — <pros/cons>
B. <option B> — <pros/cons>

Recommendation: <recommended option>

Confidence: <0.0-1.0>
```"#,
            ),
            Self::Hypothesis => (
                "Manage hypotheses for root cause analysis, debugging, or architecture decisions. List current hypotheses with supporting and contradicting evidence.",
                r#"```hypothesis
H1: <hypothesis statement>
  Confidence: <0-100%>
  Supporting Evidence: <evidence>
  Against: <contradicting evidence>

H2: <hypothesis statement>
  Confidence: <0-100%>
  Supporting Evidence: <evidence>
  Against: <contradicting evidence>
```"#,
            ),
            Self::Critic => (
                "Critique the current solution or plan. Find weaknesses, security issues, architecture problems, and areas for improvement. Score the solution.",
                r#"```critique
Current Solution: <description>

Issues:
1. <issue 1>
2. <issue 2>
3. <issue 3>

Score: <0-10>/10

Recommendations:
- <recommendation 1>
- <recommendation 2>
```"#,
            ),
            Self::Reflect => (
                "Reflect on the completed task. Identify what worked, what problems occurred, what was learned, and whether the experience should be saved to memory.",
                r#"```reflection
Task: <task description>

Result: <Success / Partial / Failed>

What worked:
- <point 1>

Problems:
- <problem 1>

Learned:
- <lesson 1>

Save to memory? <Yes / No>
```"#,
            ),
            Self::Decision => (
                "Record an architectural or technical decision. Include the decision, reason, alternatives considered, rejected options, and confidence level.",
                r#"```decision
Decision: <decision statement>

Reason:
- <reason 1>

Alternatives Considered:
- <alternative 1>

Rejected:
- <rejected option> — <reason>

Confidence: <0.0-1.0>

ADR Title: <short-kebab-case-title>
```"#,
            ),
        }
    }
}

/// ADR（Architecture Decision Record）条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdrEntry {
    /// ADR 序号
    pub number: usize,
    /// 标题
    pub title: String,
    /// 决策内容
    pub decision: String,
    /// 理由
    pub reason: String,
    /// 备选方案
    pub alternatives: Vec<String>,
    /// 被拒绝的方案
    pub rejected: Vec<String>,
    /// 置信度
    pub confidence: f32,
    /// 状态
    pub status: AdrStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdrStatus {
    Proposed,
    Accepted,
    Deprecated,
    Superseded,
}

impl AdrEntry {
    /// 生成 ADR 文件名
    pub fn filename(&self) -> String {
        format!("{:04}-{}.md", self.number, self.title)
    }

    /// 生成 ADR Markdown 内容
    pub fn render(&self) -> String {
        let alternatives = if self.alternatives.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = self
                .alternatives
                .iter()
                .map(|a| format!("- {a}"))
                .collect();
            format!("\n## Alternatives Considered\n\n{}\n", items.join("\n"))
        };

        let rejected = if self.rejected.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = self
                .rejected
                .iter()
                .map(|r| format!("- {r}"))
                .collect();
            format!("\n## Rejected Options\n\n{}\n", items.join("\n"))
        };

        format!(
            r#"# ADR {number}: {title}

## Status

{status}

## Decision

{decision}

## Reason

{reason}

## Confidence

{confidence:.0}%
{alternatives}
{rejected}
---
*Generated by core-agent Cognitive Runtime*
"#,
            number = self.number,
            title = self.title,
            status = match self.status {
                AdrStatus::Proposed => "Proposed",
                AdrStatus::Accepted => "Accepted",
                AdrStatus::Deprecated => "Deprecated",
                AdrStatus::Superseded => "Superseded",
            },
            decision = self.decision,
            reason = self.reason,
            confidence = self.confidence * 100.0,
            alternatives = alternatives,
            rejected = rejected,
        )
    }

    /// 写入 ADR 文件
    pub fn write(&self, adr_dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(adr_dir)
            .map_err(|e| format!("failed to create ADR directory: {e}"))?;

        let file_path = adr_dir.join(self.filename());
        std::fs::write(&file_path, self.render())
            .map_err(|e| format!("failed to write ADR file: {e}"))?;

        Ok(file_path)
    }

    /// 从模型响应中解析 Decision 内容并生成 ADR
    pub fn parse_from_response(response: &str, adr_dir: &Path) -> Result<Option<Self>, String> {
        // 查找 ```decision ... ``` 块
        let start = response.find("```decision")
            .ok_or_else(|| "no decision block found in model response".to_string())?;
        let content_start = response[start..].find('\n')
            .map(|i| start + i + 1)
            .ok_or_else(|| "decision block has no content".to_string())?;
        let end = response[content_start..].find("```")
            .map(|i| content_start + i)
            .ok_or_else(|| "decision block is not properly closed".to_string())?;
        let block = &response[content_start..end].trim();

        // 解析字段
        let decision = extract_field(block, "Decision:")
            .ok_or_else(|| "missing 'Decision:' field in decision block".to_string())?;
        let reason = extract_field(block, "Reason:")
            .ok_or_else(|| "missing 'Reason:' field in decision block".to_string())?;
        let title = extract_field(block, "ADR Title:")
            .or_else(|| extract_field(block, "Title:"))
            .unwrap_or_else(|| "untitled-decision".to_string());

        // 解析置信度
        let confidence = extract_field(block, "Confidence:")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(0.8);

        // 确定 ADR 序号
        let number = next_adr_number(adr_dir);

        let entry = Self {
            number,
            title: title.to_ascii_lowercase().replace(' ', "-"),
            decision,
            reason,
            alternatives: Vec::new(), // 简单解析暂不处理多行列表
            rejected: Vec::new(),
            confidence,
            status: AdrStatus::Proposed,
        };

        Ok(Some(entry))
    }
}

fn extract_field<'a>(block: &'a str, field: &str) -> Option<String> {
    for line in block.lines() {
        if let Some(value) = line.strip_prefix(field) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn next_adr_number(adr_dir: &Path) -> usize {
    let max = std::fs::read_dir(adr_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.ends_with(".md") {
                        name.split('-').next().and_then(|s| s.parse::<usize>().ok())
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    max + 1
}

/// 解析认知命令的标准输出结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveOutput {
    /// 命令类型
    pub command: String,
    /// 原始响应
    pub raw_response: String,
    /// 生成的 ADR 文件路径（仅 /decision）
    pub adr_path: Option<PathBuf>,
}

/// 处理认知命令的完整响应
pub fn process_cognitive_response(
    command: CognitiveCommand,
    response: &str,
    workspace: &Path,
) -> CognitiveOutput {
    let mut output = CognitiveOutput {
        command: command.as_str().to_string(),
        raw_response: response.to_string(),
        adr_path: None,
    };

    // 仅 /decision 生成 ADR
    if matches!(command, CognitiveCommand::Decision) {
        let adr_dir = workspace.join("docs").join("adr");
        match AdrEntry::parse_from_response(response, &adr_dir) {
            Ok(Some(entry)) => match entry.write(&adr_dir) {
                Ok(path) => {
                    output.adr_path = Some(path);
                }
                Err(e) => {
                    // ADR 写入失败返回错误信息，不影响主流程
                    output.raw_response = format!("{response}\n\n⚠️ ADR generation failed: {e}");
                }
            },
            Ok(None) => {
                // 模型未返回 structured decision 块，不生成 ADR
            }
            Err(e) => {
                output.raw_response =
                    format!("{response}\n\n⚠️ ADR parsing failed: {e}");
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cognitive_command_is_read_only() {
        assert!(CognitiveCommand::Reason.is_read_only());
        assert!(CognitiveCommand::Think.is_read_only());
        assert!(CognitiveCommand::Hypothesis.is_read_only());
        assert!(CognitiveCommand::Critic.is_read_only());
        assert!(!CognitiveCommand::Reflect.is_read_only());
        assert!(!CognitiveCommand::Decision.is_read_only());
    }

    #[test]
    fn model_prompt_contains_instruction_and_output_format() {
        let prompt = CognitiveCommand::Reason.model_prompt(&["Why API latency increased?".into()]);
        assert!(prompt.contains("Execute the built-in /reason command"));
        assert!(prompt.contains("Evidence"));
        assert!(prompt.contains("Possible Causes"));
        assert!(prompt.contains("Do NOT output your internal chain-of-thought"));
    }

    #[test]
    fn think_prompt_includes_constraints_and_options() {
        let prompt = CognitiveCommand::Think.model_prompt(&["redesign auth module".into()]);
        assert!(prompt.contains("Constraints"));
        assert!(prompt.contains("Options"));
        assert!(prompt.contains("Recommendation"));
    }

    #[test]
    fn hypothesis_prompt_includes_evidence() {
        let prompt = CognitiveCommand::Hypothesis.model_prompt(&[]);
        assert!(prompt.contains("H1:"));
        assert!(prompt.contains("Supporting Evidence"));
        assert!(prompt.contains("Against"));
    }

    #[test]
    fn critic_prompt_includes_score() {
        let prompt = CognitiveCommand::Critic.model_prompt(&[]);
        assert!(prompt.contains("Issues"));
        assert!(prompt.contains("Score"));
    }

    #[test]
    fn reflect_prompt_includes_learning() {
        let prompt = CognitiveCommand::Reflect.model_prompt(&[]);
        assert!(prompt.contains("What worked"));
        assert!(prompt.contains("Learned"));
        assert!(prompt.contains("Save to memory"));
    }

    #[test]
    fn decision_prompt_includes_adr_title() {
        let prompt = CognitiveCommand::Decision.model_prompt(&[]);
        assert!(prompt.contains("Decision:"));
        assert!(prompt.contains("Alternatives Considered"));
        assert!(prompt.contains("ADR Title"));
    }

    #[test]
    fn adr_entry_renders_valid_markdown() {
        let entry = AdrEntry {
            number: 1,
            title: "use-sqlite".into(),
            decision: "Use SQLite as the primary database".into(),
            reason: "Simple deployment, no external dependencies".into(),
            alternatives: vec!["PostgreSQL".into(), "MySQL".into()],
            rejected: vec!["MongoDB — too complex for MVP".into()],
            confidence: 0.9,
            status: AdrStatus::Proposed,
        };
        let markdown = entry.render();
        assert!(markdown.contains("ADR 1: use-sqlite"));
        assert!(markdown.contains("Use SQLite as the primary database"));
        assert!(markdown.contains("Alternatives Considered"));
        assert!(markdown.contains("Rejected Options"));
        assert!(markdown.contains("90%"));
    }

    #[test]
    fn adr_filename_is_zero_padded() {
        let entry = AdrEntry {
            number: 1,
            title: "use-sqlite".into(),
            decision: "test".into(),
            reason: "test".into(),
            alternatives: vec![],
            rejected: vec![],
            confidence: 0.8,
            status: AdrStatus::Accepted,
        };
        assert_eq!(entry.filename(), "0001-use-sqlite.md");
    }

    #[test]
    fn adr_write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let entry = AdrEntry {
            number: 1,
            title: "test-decision".into(),
            decision: "Test decision".into(),
            reason: "Test reason".into(),
            alternatives: vec![],
            rejected: vec![],
            confidence: 0.8,
            status: AdrStatus::Proposed,
        };
        let path = entry.write(dir.path()).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("ADR 1: test-decision"));
    }

    #[test]
    fn process_cognitive_response_decision_creates_adr() {
        let dir = tempfile::tempdir().unwrap();
        let response = r#"```decision
Decision: Use SQLite for local storage

Reason: Simple deployment, no external deps

Confidence: 0.85

ADR Title: use-sqlite-local
```"#;
        let output = process_cognitive_response(
            CognitiveCommand::Decision,
            response,
            dir.path(),
        );
        assert_eq!(output.command, "decision");
        assert!(output.adr_path.is_some());
        let adr_path = output.adr_path.unwrap();
        assert!(adr_path.exists());
        let content = std::fs::read_to_string(&adr_path).unwrap();
        assert!(content.contains("Use SQLite for local storage"));
    }

    #[test]
    fn process_cognitive_response_non_decision_does_not_create_adr() {
        let dir = tempfile::tempdir().unwrap();
        let response = "Some analysis result";
        let output = process_cognitive_response(
            CognitiveCommand::Reason,
            response,
            dir.path(),
        );
        assert_eq!(output.command, "reason");
        assert!(output.adr_path.is_none());
    }

    #[test]
    fn next_adr_number_increments_from_existing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("0001-first.md"), "").unwrap();
        std::fs::write(dir.path().join("0003-third.md"), "").unwrap();
        assert_eq!(next_adr_number(dir.path()), 4);
    }

    #[test]
    fn next_adr_number_returns_one_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(next_adr_number(dir.path()), 1);
    }

    #[test]
    fn extract_field_from_block() {
        let block = "Decision: Use SQLite\nReason: Simple\n";
        assert_eq!(
            extract_field(block, "Decision:").as_deref(),
            Some("Use SQLite")
        );
        assert_eq!(
            extract_field(block, "Reason:").as_deref(),
            Some("Simple")
        );
        assert_eq!(extract_field(block, "Missing:"), None);
    }

    #[test]
    fn adr_parse_from_missing_block_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let result = AdrEntry::parse_from_response("no decision block here", dir.path());
        assert!(result.is_err());
    }
}