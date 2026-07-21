# Unknowns Report

## Metadata

- **Task / Feature:** Core-Agent P1 — Intelligence Runtime (Planner/Task/Todo/Question/Reflection)
- **Mode:** Standard
- **Date:** 2026-07-21
- **Prepared by:** Claude
- **Scope:** 5 个 P1 模块 — Planner、Task、Todo、Question、Reflection

## Intent

### User-visible problem

P0 的 Agent 是"响应式"的：用户说一句它做一句。无法理解复杂目标、拆解任务、制定计划、执行步骤、遇到问题问人、完成自检。P1 要让 Agent 从 Reactive 升级为 Proactive/Goal-driven。

### Desired behavior change

用户输入一个目标（如"帮我实现 OAuth 登录"），Agent 应该：
1. 理解目标 → 2. 生成 Plan（DAG，可审查）→ 3. 展示 Todo 列表 → 4. 逐个执行 Task → 5. 遇到不确定时问用户 → 6. 完成后自检 → 7. 接受或优化

### Affected users and workflows

- CLI 用户（agent-cli）：看到 Plan 预览 + Todo 进度 + Question 交互
- Desktop 用户：看到 Plan 图形 + 任务面板 + Question 弹窗
- EnterpriseAgent 内部：主循环从"一条消息 → LLM → 回复"变成"一条消息 → Plan → Execute → Reflect"

### Success criteria

1. 支持 `LLM + validate` 的 Planner 策略：LLM 生成 PlanDraft JSON → Plan::validate() 校验
2. Todo 列表随 Task 执行进度实时更新
3. Question 支持 CHOICE/CONFIRM/INPUT 三种类型，Agent 主循环中可暂停等待用户输入
4. Reflection 在 Task 执行完成后自动触发，输出评分/建议
5. 所有模块有单元测试 + 至少一个 E2E 测试

### Non-goals

- 不重写 core-agent-plan（已有完整 Plan/Task/Step/DAG/Review）
- 不重写 core-agent-execution（已有完整 ExecutionEngine）
- 不做 Multi-Agent 编排（P2）
- 不做复杂 UI（只做 CLI 交互 + 基础 Desktop 集成）

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | core-agent-plan/src/domain.rs | 完整的 Plan/Task/Step/Goal/Action/DAG/Review 模型（1000+ 行） | High |
| Code | core-agent-plan/src/manager.rs | PlanningManager 支持 create_plan_from_draft，可直接从 PlanDraft 生成 Plan | High |
| Code | core-agent-plan/src/defaults.rs | RulePlanBuilder 是按 request_kind 模板的默认 builder，可替换为 LLMPlanBuilder | High |
| Code | core-agent-plan/src/infrastructure.rs | PlanBuilder trait 清晰，可添加新的实现 | High |
| Code | core-agent-execution/src/manager.rs | ExecutionManager 完整执行引擎，支持 execute/prepare/start/pause/resume/cancel | High |
| Code | core-agent-agent/ | AgentManager 协调 Planning + Execution 生命周期 | High |
| Code | src/enterprise.rs | EnterpriseAgent 已集成 PlanningManager + ExecutionManager（plan_mode: RwLock<bool>） | High |
| Design doc | 042-core-ablity-p1-plan.md | 完整 P1 设计，5 模块职责分明 | High |
| Design doc | 041-core-ablity-00-overview.md | 整体路线图，P1 定位清晰 | High |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition |
|---|---|---|---|---|---|---|---|---|
| LLMPlanBuilder 如何与现有 Planner 集成？ | Known unknown | core-agent-plan 有 PlanBuilder trait，但 RulePlanBuilder 是同步模板。LLM 生成需要异步调用 ModelProvider | 5 | 4 | 2 | 4 | 160 | Decision |
| Question 如何与 Agent 主循环交互？ | Known unknown | 当前 EnterpriseAgent.run() 是同步的：build context → LLM → tool loop → response。Question 需要中途暂停等用户输入 | 5 | 4 | 2 | 4 | 160 | Decision |
| Todo 如何与 Task 执行状态同步？ | Known unknown | Task 执行在 core-agent-execution 中，Todo 需要订阅 ExecutionObserver 事件 | 4 | 4 | 1 | 3 | 48 | Decision |
| Reflection 在什么时机触发？ | Known unknown | 计划完成后？还是每个 Task 完成后？设计文档提到 max retry 和 score threshold | 4 | 3 | 1 | 3 | 36 | Decision |
| CLI 如何展示 Plan/Todo/Question？ | Unknown known | agent-cli 当前是纯文本交互，P1 需要结构化展示（Plan 列表、Todo 复选、Question 选项） | 3 | 4 | 2 | 3 | 72 | Experiment |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Planner 生成的 Plan 应该让用户先审查再执行 | Claude Code Plan Mode 就是先展示 Plan 让用户确认 | 已经在 PlanStatus 中支持 Reviewing → Ready 状态机 |
| Question 应该像自然对话而不是弹窗 | Claude Code 的交互体验是内联的 | 先用 CLI 模式（y/n/数字选择），Desktop 再优化 |
| Reflection 不应该阻塞用户太久 | 设计文档明确说 "max retry / budget / score threshold" | 默认 1 轮，超过阈值直接接受 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| Reflection 触发时机 | 1. 每个 Task 完成触发 2. 整个 Plan 完成触发 3. 两者都触发 | 选项 1 更细粒度但噪音大；选项 2 更简洁；选项 3 最完整但复杂 | Architecture | 实现前 |
| Question 超时策略 | 1. 无限等待 2. 超时后自动选择默认 3. 超时后跳过 | 选项 1 用户体验好但可能死锁；选项 2 安全但可能选错；选项 3 保守 | User | 实现前 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| LLMPlanBuilder 可以用 PlanDraft 结构与现有 PlanBuilder trait 兼容 | PlanBuilder::build() 返回 PlanDraft，与 RulePlanBuilder 一致 | 只需要改 build() 方法实现，接口不变 |
| Todo 可以基于 Task 状态自动生成 | Task 的 name/description 含足够信息衍生 Todo | 可以在 Todo 中增加自定义字段扩展 |
| 用 `crossbeam_channel` 或 `tokio::sync::oneshot` 实现 Question 暂停等待 | 异步 channel 模式成熟，不会影响现有代码结构 | 可替换为 watch 或 broadcast 模式 |

## Recommended Implementation Boundary

### Implement now

1. `core-agent-question` crate — Question 模型 + runtime
2. `core-agent-todo` crate — Todo 模型 + runtime
3. `core-agent-reflection` crate — Reflection 模型 + runtime
4. `LLMPlanBuilder` 添加到 core-agent-plan 的 defaults.rs
5. 集成到 enterprise.rs — 将 Question/Todo/Reflection 接入主循环

### Do not implement now

- Desktop 端的 Plan 图形展示（先 CLI）
- 复杂的 Rule Engine 模板（现有 RulePlanBuilder 够用）

### Interfaces or data contracts to freeze

- `PlanBuilder` trait（core-agent-plan）
- `ExecutionObserver` trait（core-agent-execution）
- `EnterpriseAgent::run_with_approval_inner` 的钩子点

### Areas that must remain reversible

- Question 在 Agent 主循环中的集成点（先用 channel，后续可换 gRPC 等）
- Reflection 评分策略（先用简单规则，后续可换 LLM 评估）

## Verification Plan

### Automated

- Unit tests: 每个新 crate 的 domain/model 验证
- Integration tests: LLMPlanBuilder → create_plan_from_draft → ExecutionManager 全链路
- E2E tests: 模拟完整 P1 流程（输入目标 → Plan → 审查 → 执行 → Question → 完成 → Reflection）

### Manual

- CLI: 输入目标，观察 Plan 展示、Todo 进度、Question 交互、Reflection 结果
- Desktop: 同上

## Handoff

- [ ] Acceptance criteria
- [ ] Explicit invariants
- [ ] Data and interface contracts
- [ ] Implementation notes file