# Unknowns Report

## Metadata

- **Task / Feature:** P031 强制只读 Plan 与文件 Checkpoint
- **Mode:** Standard
- **Date:** 2026-07-19
- **Prepared by:** Codex
- **Scope:** Interaction、Enterprise tool loop、workspace write、Terminal/Desktop unified commands

## Intent

### User-visible problem

计划命令当前仅依赖提示词，不能证明不会编辑；Agent 写错文件后也缺少安全的 session 级回退能力。

### Desired behavior change

只读命令由 Runtime 硬约束；Agent 文件编辑可撤销/重做，同时不覆盖用户在之后进行的手工修改。

### Success criteria

- 只读边界在工具声明和执行前双重实施。
- Checkpoint 不依赖 Git，支持 CAS、防越界、持久化与整组恢复。
- `/undo`、`/redo` 是核心命令，所有入口行为一致。

### Non-goals

- 命令、网络和远程系统副作用不可撤销。
- 本 P 不实现完整对话 rewind。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | `src/interaction.rs` | `/plan` 当前是 Agent prompt expansion | High |
| Code | `src/enterprise.rs` | 全部工具都暴露给模型，写入集中在 `write_file` | High |
| Code | `src/enterprise.rs::safe_command` | 已有严格只读命令白名单 | High |
| Tests | `tests/enterprise_agent_e2e.rs` | 可注入模型验证工具声明和真实写入 | High |
| Design reference | Claude Code checkpointing | 文件快照与权限是互补安全机制，远程副作用不可回退 | High |
| Design reference | OpenCode TUI undo/redo | 撤销包含文件变化，但 OpenCode 依赖 Git | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| 工作区已有 SHA-256 CAS | `write_workspace_file` | 可复用为撤销前并发保护 |
| `.agent` 被工具策略阻止 | `blocked_workspace_name` | checkpoint 不会被模型直接修改 |
| Runtime 操作串行化 | `operation_lock` | 可按请求聚合文件变化 |
| Terminal/Desktop 已共用命令注册表 | P029 | 不需要两套 undo/redo |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 是否使用 Git 管理 checkpoint | Known unknown | 工作区可能有用户脏改动或不是 Git 仓库 | 5 | 5 | 4 | 5 | 500 | Decision | 使用独立 session 文件快照 |
| 是否追踪 shell 命令变化 | Unknown known | 任意命令影响范围不可可靠确定 | 5 | 5 | 5 | 5 | 625 | Blocker | 明确不追踪，仍需审批 |
| undo 遇到用户后续编辑如何处理 | Unknown unknown candidate | 静默覆盖会丢数据 | 5 | 4 | 5 | 5 | 500 | Decision | SHA-256 不匹配立即拒绝 |
| checkpoint 是否跨重启 | Known unknown | 企业 Agent session 需要恢复 | 4 | 4 | 3 | 4 | 192 | Decision | Runtime 目录持久化 JSON 快照 |
| 快照可能包含敏感内容 | Unknown unknown candidate | 源文件本身可能私有 | 4 | 3 | 3 | 4 | 144 | Monitor | 仅已允许写入路径；目录本地、忽略提交、输出不展示正文 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 一次 undo 回退一轮请求 | Claude/OpenCode 的用户心智 | 多文件 E2E |
| 不破坏未受 Agent 管理的改动 | Coding Agent 安全基线 | CAS 冲突 E2E |
| 新写入后 redo 失效 | 常见 undo stack 行为 | 状态机单测 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| 同一轮重复写同一文件 | 不能错误恢复到中间状态 | 聚合单测 |
| 新建文件 undo 后父目录变化 | 删除必须限定目标文件 | 路径边界断言 |
| 快照写到一半崩溃 | 不能加载半文件继续覆盖 | tmp 恢复/损坏 fail-closed 测试 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 首版只保存 UTF-8 文本 | `write_file` 本身是文本工具 | 后续版本化增加二进制编码 |
| 每 session 保留最近 20 轮 | 有界本地存储且足够日常恢复 | 配置化上限 |

## Recommended Implementation Boundary

### Implement now

- Plan 双重只读边界。
- `write_file` 请求级 checkpoint、持久化、CAS undo/redo。
- 核心 `/undo`、`/redo` 和双入口测试。

### Do not implement now

- shell/remote 副作用回滚、Git index 操作、完整消息 rewind。

### Interfaces or data contracts to freeze

- `InteractionCommandInvocation::is_read_only()`。
- `/undo`、`/redo` 作为 Runtime 命令。
- Checkpoint 快照带版本字段，不暴露给 UI。

## Verification Plan

### Automated

- Unit tests: read-only classification、checkpoint group/stack/CAS。
- Integration tests: model tool list、臆造写拒绝、create/modify undo/redo。
- Contract tests: Terminal/Desktop command registry consistency。
- Static analysis: full workspace check/clippy。

### Manual

- Plan 请求不出现写审批。
- AI 创建文件后 `/undo`、`/redo`。
- 手工修改后 `/undo` 拒绝。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [ ] Implementation notes file
