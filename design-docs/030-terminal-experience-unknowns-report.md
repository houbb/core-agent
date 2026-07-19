# Unknowns Report

## Metadata

- **Task / Feature:** P030 Terminal 产品体验与安全交互
- **Mode:** Standard
- **Date:** 2026-07-19
- **Prepared by:** Codex
- **Scope:** `agent chat`、统一交互注册表、Terminal 审批适配器

## Intent

### User-visible problem

当前 `agent chat` 只有逐行输入输出，没有品牌首屏、稳定布局、输入框、候选面板和内嵌审批，虽然链路可运行，但不具备 OpenCode / Claude Code 级终端产品体验。

### Desired behavior change

用户打开任意文件夹后，获得一个自适应全屏 Agent TUI；命令、文件引用、执行状态和人的审核都在同一视觉界面完成。

### Affected users and workflows

- Terminal 交互用户：启动、输入、命令/文件补全、审批、查看结果。
- 自动化用户：继续使用 `run`、其他子命令和非 TTY 输入。
- Desktop 用户：不改变核心语义，继续复用同一 Runtime。

### Success criteria

- TTY 使用全屏布局；非 TTY 保持纯文本兼容。
- `/`、`@`、审批全部复用核心实现或稳定接口。
- TUI 有渲染和状态断言；真实模型端到端链路通过。

### Non-goals

- 本 P 不实现 MCP、LSP、远程协作或多 Agent UI。
- 本 P 不把 Desktop 前端改造成终端风格。
- 本 P 不实现 token 级流式协议重构。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | `agent-cli/src/main.rs` | Chat 当前是 `stdin.lines()` 裸循环 | High |
| Code | `src/interaction.rs` | `/` 已有核心注册表与统一路由 | High |
| Code | `src/enterprise.rs` | `@`、模型工具循环和权限策略已统一 | High |
| Code | `agent-cli/src/embedded.rs` | 审批仍直接读写 stdin/stderr，会与 raw TUI 冲突 | High |
| Tests | `agent-cli/tests/cli_runtime_e2e.rs` | 脚本、会话和配置路径已有覆盖 | High |
| Design reference | OpenCode TUI / permissions official docs | 全屏会话、`/`、`@`、命令面板和审批是基准交互 | High |
| Design reference | Claude Code CLI / permissions official docs | 模式切换、只读 Plan、人的审批和 checkpoint 是安全基线 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Runtime 已是同进程组合 | `EnterpriseAgent` composition root | TUI 不需要管理多个子进程 |
| 命令定义已有单一来源 | `InteractionCommandRegistry` | 命令面板不得维护第二份列表 |
| `@` 最终解析已有边界限制 | `ContextMentionResolver` | TUI 只做候选，不复制解析逻辑 |
| Terminal 审批当前阻塞 stdin | `TerminalApprovalHandler` | 必须改为 channel + TUI modal |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| TUI 是否替换所有输出模式 | Known unknown | 脚本依赖稳定文本输出 | 5 | 5 | 3 | 5 | 625 | Decision | 仅 `chat` + TTY 进入 TUI |
| 审批如何在模型运行期间交互 | Unknown known | 当前 `send()` 内同步等待模型 | 5 | 5 | 3 | 5 | 625 | Experiment | 后台任务执行，审批用 oneshot 通道回传 |
| 自由文本历史是否落盘 | Unknown unknown candidate | 提示中可能含密钥或私有上下文 | 4 | 4 | 3 | 4 | 192 | Decision | 仅内存保存；斜杠命令沿用已有安全历史 |
| 首版是否需要 token 流式渲染 | Known unknown | 当前 Embedded API 在完成后返回事件 | 4 | 5 | 2 | 4 | 160 | Defer | 本 P 显示 busy/工具事件结果，流式协议列入 P1 |
| 小尺寸终端如何处理 | Unknown unknown candidate | 固定布局会 panic 或不可用 | 4 | 3 | 2 | 4 | 96 | Experiment | 自适应约束 + TestBackend 小尺寸渲染 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 启动即有产品识别 | 用户明确提到 icon / ASCII | TestBackend 首屏断言 |
| 输入和选择无需记命令 | OpenCode / Claude Code 使用命令面板 | `/`、`@` 键盘补全测试 |
| 审批属于会话而非跳出终端 | 用户要求严格 HITL | 审批 modal 状态测试 + E2E |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| panic 后终端仍处于 raw mode | 会破坏用户 shell | RAII cleanup + manual launch/exit |
| Unicode 路径与中文输入 | Windows 开发环境常见 | UTF-8 字符边界编辑单测 |
| 大量文件导致候选卡顿 | 大仓库常见 | 2000 文件硬上限、跳过构建目录 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| TUI 框架 | Ratatui / 手写 ANSI / Web terminal | Ratatui 提供成熟布局与 TestBackend | Architecture | 已决定：Ratatui |
| 审批桥接 | stdin 阻塞 / channel modal | channel 保持单一权限决策接口 | Security | 已决定：channel modal |

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| 布局在不同终端尺寸是否稳定 | Ratatui TestBackend | 40x12 与 120x36 都能渲染 | Low | Implementation |
| 审批是否可在后台执行中响应 | deterministic E2E | request 到达 modal，decision 返回 Runtime | Medium | Implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 首版单会话串行执行 | 当前 Runtime 已有 operation lock | 后续把任务列表扩展为并发 tabs |
| `Esc` 默认拒绝审批 | 安全且可逆 | 后续支持配置化快捷键 |
| `Alt+Enter` 表示换行 | 不改变 Enter 提交行为 | 后续开放 keymap 配置 |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| token/tool 实时流式协议 | 需要跨 Embedded/HTTP/Desktop 协议改造 | P1 统一事件流 |
| 跨进程恢复 TUI 草稿 | 可能持久化敏感数据 | 默认不实现，后续显式 opt-in |

## Recommended Implementation Boundary

### Implement now

- Ratatui 全屏布局、消息区、输入区、状态栏。
- 核心命令和工作区文件候选。
- 后台请求与内嵌审批 modal。
- TTY/非 TTY 双路径和终端恢复保护。

### Do not implement now

- 独立终端服务、第二套命令系统、持久化自由文本历史。
- MCP、LSP、Hooks、复杂主题市场。

### Interfaces or data contracts to freeze

- `InteractionCommandRegistry` 是 `/` 唯一来源。
- `EnterpriseApprovalHandler` 是所有 UI 的审批端口。
- `ContextMentionResolver` 是 `@` 最终语义来源。

### Areas that must remain reversible

- TUI 主题、ASCII 标识、快捷键和候选排序。

## Verification Plan

### Automated

- Unit tests: 输入编辑、命令/文件候选、审批默认拒绝。
- Integration tests: TUI 状态到 Agent/命令应用层。
- Contract tests: `/` 列表与核心注册表一致。
- Static analysis: workspace check + clippy warnings-as-errors。

### Manual

- Happy path: 打开真实项目并请求 AI 分析/编辑。
- Failure path: 模型错误显示在消息区且终端可继续使用。
- Recovery path: 正常退出/中断后 shell 恢复。
- Permission boundaries: 写文件审批允许/拒绝。

### Observability

- Logs: 复用 Enterprise events，不记录 key。
- Audit trail: 复用工具和审批事件。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [ ] Implementation notes file
