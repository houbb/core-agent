# Unknowns Report

## Metadata

- **Task / Feature:** 041-core-ablity-p0-basic.md 澄清后实现
- **Mode:** Standard
- **Date:** 2026-07-21
- **Scope:** 对比设计文档与实际代码库，识别差距并确定实现方案

## Intent

### User-visible problem

设计文档 `041-core-ablity-p0-basic.md` 定义了 P0 基础能力（Agent Runtime、LLM、Tool、Context、Memory、Permission），但项目实际代码已经远超 P0 阶段（0.38.4 版本，25 个模块）。需要澄清设计文档与当前实现的差距，明确哪些是已有的、哪些是缺失的，然后决定是补充缺失部分还是重新审视设计。

### Desired behavior change

搞清楚当前代码库在 P0 设计文档中定义的 6 大模块上的实现现状，识别真正的差距，决定下一步做什么。

### Affected users and workflows

- 开发团队（需要清楚当前实现与设计文档的映射关系）
- 新加入的开发者（需要理解设计文档与实际代码的差异）

### Success criteria

1. 清晰对比设计文档 6 大模块 vs 当前代码库
2. 识别真正的差距（不是命名差异，而是功能缺失）
3. 确定是否需要实现新功能

### Non-goals

- 不要修改代码库现有功能
- 不需要对设计文档做重构

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|--------|----------|-----------------|:----------:|
| 设计文档 | `design-docs/041-core-ablity-p0-basic.md` | P0 架构定义（6 大模块） | High |
| 概览文档 | `design-docs/041-core-ablity-00-overview.md` | P0-P5 路线图 | High |
| 工作区定义 | `Cargo.toml` | 25 个 workspace members | High |
| Agent 运行时 | `core-agent-agent/` | AgentProfile/Policy/Snapshot/Lifecycle | High |
| LLM 运行时 | `core-agent-model/` | ModelManager/Provider/Router/Usage | High |
| Tool 运行时 | `core-agent-tool/` | 75 builtin tools/ToolManager | High |
| Context 运行时 | `core-agent-context/` | ContextRuntime/Builder/Composer | High |
| Memory 运行时 | `core-agent-memory/` | MemoryManager/Classifier/Retriever | High |
| 权限系统 | `core-agent-platform/` + `src/enterprise.rs` | RBAC/ABAC/PolicyEngine/Approval | High |
| 主入口 | `src/enterprise.rs` | EnterpriseAgent 组合根 | High |
| 现状分析 | `docs/current/current-components-v20260720.md` | 组件对标分析 | High |
| 现状分析 | `docs/current/current-agent-v20260720.md` | Agent 类型对标分析 | High |
| 现状分析 | `docs/current/current-plan-vs-cc-codex.md` | Plan Mode 对标分析 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|------|----------|-----------|
| 项目是 Rust + Cargo Workspace，不是 Java | `Cargo.toml` 定义 25 个 Rust crate | 设计文档用 Java 伪代码描述，需映射到 Rust 实现 |
| 25 个模块远超 P0 定义的 6 个 | `Cargo.toml` members 列表 | 项目已进入 P1-P8 阶段 |
| 所有 6 个 P0 模块都有对应的 Rust crate | 各 crate 的 lib.rs 和实际代码 | 核心功能已实现，但命名不同 |
| 支持 Streaming 仅通过 EnterpriseAgentEvent 事件收集 | `src/enterprise.rs` 的 run_with_approval_inner 函数 | 设计文档要求 token/tool/status streaming，当前是事件收集器 + 批量返回，不是实时流式 |
| Plan Mode 已实现基础模式切换 | `EnterpriseAgent.plan_mode` 字段 + `set_plan_mode()` | 支持只读模式，但缺少展示层和审批流 |
| 工具权限系统完善 | EnterpriseApprovalLedger + ManagedPolicy + PlatformManager | 设计文档的 Permission 模块已实现 |
| 已实现 75 个内置工具 | `core-agent-tool/src/builtin/` | 远超设计文档的 6 个内置工具 |
| subagent_runtime 已扩展 9 个 Profile | `src/subagent_runtime.rs` | 已超出设计文档的 Agent 概念 |
| 设计文档的 Agent Session 模型对应 core-agent-session | `core-agent-session/` 的 SessionRuntime | 已实现但命名不同 |
| 设计文档的 Agent Loop 对应 EnterpriseAgent.run_with_approval_inner | `src/enterprise.rs` 2478-2707 行 | 8 轮 tool-call 循环，功能完整 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---------|----------|---------------------|:------:|:-----------:|:----------------:|:-------------------:|:--------:|:--------|:-----------|
| 用户所说的"澄清后实现"具体指什么？ | Known unknown | 设计文档 P0 vs 实际代码差异巨大，不确定用户是想"补差距"还是"重新理解设计" | 5 | 5 | 3 | 5 | 375 | **Blocker** | 问用户 |
| 当前是否缺少 Streaming Runtime？ | Unknown unknown candidate | 设计文档要求 token/tool/status streaming，但当前 EnterpriseAgent 是批量同步模式 | 4 | 3 | 2 | 4 | 96 | **Decision** | 问用户是否需要 streaming |
| 需要补充 E2E 测试吗？ | Known unknown | 设计文档要求"尽可能端到端测试"，但当前项目已有测试结构 | 2 | 3 | 1 | 1 | 6 | Accept | 先看用户需求 |
| 权限系统是否需要增强？ | Unknown known | 设计文档要求单独 core-permission，当前权限分散在多个模块 | 3 | 2 | 3 | 3 | 54 | **Monitor** | 确认当前权限模型是否满足需求 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|-------------|-----------------|-------------------|
| 设计文档中的 Java 伪代码应当被理解为 Rust 实现的参考 | 设计文档是方案层，不是实现层 | 确认用户是否接受 Rust 映射 |
| 用户可能没意识到项目已经远超 P0 | 设计文档是早期规划，项目已迭代到 0.38.4 | 展示当前项目状态，让用户确认 |
| 用户可能想补充"缺失的"P0 功能 | 设计文档中的某些功能可能确实未实现 | 列出具体差距让用户确认 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|----------|---------|------------|:-----------------:|:------------------:|
| 当前 P0 设计文档与实际代码差异巨大，是做"对照实现"还是"更新设计文档"？ | (A) 对照设计文档补当前缺失功能 / (B) 更新设计文档反映当前状态 / (C) 选择具体功能点实现 | A 可能导致重复工作，B 偏文档，C 最务实 | 用户 | 立即 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|------------|---------------|---------------|
| 设计文档是早期规划，当前代码已经远超 P0 | 代码库 0.38.4 版本，25 个模块，功能完整 | 如果用户要求严格按设计文档实现，可以回退 |
| 项目的 Rust 模块命名与设计文档不同但功能对应 | 每个 crate 的文档和代码已验证 | 功能映射关系已记录在报告中 |

## Recommended Implementation Boundary

### Implement now

- 先澄清用户意图：究竟是要"补差距"还是"重新理解设计文档"

### Do not implement now

- 不要直接修改代码
- 不要创建新的模块

### Areas that must remain reversible

- N/A — 当前阶段不需要修改代码

## Verification Plan

### Manual

- 用户确认后进入实现阶段