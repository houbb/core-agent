# Unknowns Report

## Metadata

- **Task / Feature:** Plan(计划+询问模式) 模块对标 opencode/claude-code/chatGPT(codex) 核心能力分析
- **Mode:** Standard
- **Date:** 2026-07-20
- **Prepared by:** Core Agent
- **Scope:** `core-agent-plan` + `core-agent-execution` + `ask.*` tools + `cognitive.*` + `/plan` slash command 的现状 vs Claude Code Plan Mode / Codex update_plan / OpenCode 对标分析

---

## Intent

### User-visible problem

当前 Agent 的 Plan 模块已有完整的数据模型（Goal → Task → Step → Action 四层结构 + DAG 依赖图 + 快照/恢复/审查），但**用户侧缺少 Plan Mode 交互体验**：没有"先规划、后执行"的模式切换，用户无法在计划执行前审查和确认计划。Claude Code 的 Plan Mode 让用户先看到完整计划再决定是否执行，Codex 的 `update_plan` 以 Todo 列表形式展示进度。我们的 Plan 目前是后端自动生成+自动审查，用户感知不到"计划"这个环节的存在。

### Desired behavior change

让 Plan 从**后端自动机制**升级为**用户可感知、可交互、可审批的 Plan Mode**：

- 用户输入目标 → Agent 进入 Plan Mode → 生成计划 → 展示给用户 → 用户审批/修改 → 执行计划
- 计划执行过程中，用户可看到 Todo 进度（已完成/进行中/待办）
- 遇到不确定或高风险操作时，Agent 主动询问用户
- 执行完成后，Agent 自我检查（Reflection）并生成执行总结

### Affected users and workflows

- **所有用户**：每次使用 `/plan` 命令或 Agent 自动规划时
- Desktop 用户：看到计划面板、Todo 列表、审批对话框
- CLI TUI 用户：看到计划文本预览、确认提示、进度条

### Success criteria

1. Agent 能够在执行前生成计划并展示给用户（Plan Mode）
2. 用户可以审查计划、提出修改、批准或拒绝
3. 计划执行过程中显示 Todo 进度
4. Agent 能在不确定时主动询问用户
5. 执行完成后有 Reflection 总结

### Non-goals

- 不涉及 Multi-Agent 协作规划（P2 范围）
- 不涉及 Workflow Engine 自动调度
- 不涉及 LLM Planning 自动生成（当前 Rule Builder 可用）
- 不涉及 Tree Search / Reflection / Auto Replan 等高级规划算法

---

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|--------|----------|-----------------|------------|
| Code | `core-agent-plan/src/domain.rs` | Goal/Plan/Task/Step/Action 完整领域模型，PlanningGraph 含 DAG 依赖校验，PlanReview 含 ReviewDecision | High |
| Code | `core-agent-plan/src/manager.rs` | PlanningManager 完整：create_goal/plan, update, cancel, resume, snapshot/restore, 乐观版本控制 | High |
| Code | `core-agent-plan/src/defaults.rs` | RulePlanBuilder（确定性规则生成）、StructuralPlanReviewer（自动审查）、DefaultPlanningLifecycle | High |
| Code | `core-agent-execution/src/manager.rs` | ExecutionManager 消费 Plan：prepare → execute, 支持 dispatch/retry/rollback/checkpoint | High |
| Code | `core-agent-tool/src/builtin/plan/` | `plan.create`/`plan.update`/`plan.review` 三个 builtin tools | High |
| Code | `core-agent-tool/src/builtin/ask/` | `ask.user`/`ask.select` 两个询问工具 | High |
| Code | `src/interaction.rs` | `/plan` slash command 路由到 Agent 处理 | High |
| Code | `src/cognitive.rs` | `/reason`/`/think`/`/hypothesis`/`/critic`/`/reflect`/`/decision` 认知命令 | High |
| Design doc | `design-docs/006-planning.md` | Planning Runtime 核心设计文档（五层结构、Graph、Review、Snapshot） | High |
| Design doc | `design-docs/006-planning-unknowns-report.md` | P5 Planning 实现前的 Unknowns 分析 | High |
| Design doc | `design-docs/006-planning-post-implementation-review.md` | P5 实现后 Review（通过，含剩余风险） | High |
| Design doc | `design-docs/042-core-ablity-p1-plan.md` | P1 智能增强设计：Planner/Task/Todo/Question/Reflection 五模块 | High |
| Design doc | `design-docs/034-vs-cc-gpt-opt.md` | 整体对标 Claude Code 和 ChatGPT 的分析（含 Plan Mode） | High |
| Design doc | `design-docs/035-tools-margin-to-claude-code.md` | Tools 对标 Claude Code（含 Plan Mode 分析） | High |
| Design doc | `design-docs/035-tools-margin-to-codex.md` | Tools 对标 Codex（含 update_plan 分析） | High |
| Doc | `docs/current/current-agent-v20260720.md` | Agent/SubAgent 现状 + 缺失 Agent 类型分析 | High |
| Doc | `docs/current/current-components-v20260720.md` | 核心组件分层现状 vs 预期对比 | High |
| Code | `src/subagent_runtime.rs` | SubAgent 三种 Profile（General/Explore/Review），最大 4 轮只读 | High |

---

## Confirmed Facts

| Fact | Evidence | Relevance |
|------|----------|-----------|
| `core-agent-plan` 已实现完整的计划数据模型（Goal→Task→Step→Action）+ PlanningGraph + PlanReview + Snapshot | `core-agent-plan/src/domain.rs` + `manager.rs` | 核心 Plan 能力就绪 |
| Plan 生命周期完整：Created→Planning→Reviewing→Ready→Executing→Completed | `core-agent-plan/src/domain.rs:PlanStatus` + `can_transition_to()` | 状态机就绪 |
| Plan 自动审查：StructuralPlanReviewer 在 Plan 创建时自动生成 Review | `core-agent-plan/src/defaults.rs` | 审查机制就绪但无人工参与 |
| Execution 可消费 Plan：从 approved READY Plan 开始执行 | `core-agent-execution/src/manager.rs` + `domain.rs` | 执行链路就绪 |
| 询问工具已存在：`ask.user` 和 `ask.select` 两个 builtin tools | `core-agent-tool/src/builtin/ask/` | 基础询问能力就绪 |
| 认知命令已存在：6 个认知命令（reason/think/hypothesis/critic/reflect/decision） | `src/cognitive.rs` | 基础认知能力就绪 |
| 缺少 Plan Mode 模式切换：无 `EnterPlanMode`/`ExitPlanMode` 概念 | 全库搜索无匹配 | 核心缺失 |
| 缺少 Question Runtime：`core-agent-question` 设计文档存在但未实现 | `design-docs/042-core-ablity-p1-plan.md` 设计但无 crate | 询问模块缺失 |
| 缺少 Todo Runtime：`core-agent-todo` 设计文档存在但未实现 | 同上 | Todo 模块缺失 |
| 缺少 Reflection Runtime：`core-agent-reflection` 设计文档存在但未实现 | 同上 | Reflection 模块缺失 |
| 缺少 Plan 可视化：Desktop 和 CLI 均无 Plan 面板/Todo 列表 | `agent-desktop/src/App.vue` + `agent-cli/src/tui.rs` | UX 交互缺失 |
| 缺少 Human-in-the-loop 审批：Plan 创建后自动审查通过，无用户确认环节 | `core-agent-plan/src/manager.rs` 自动 review → Ready | 审批流程缺失 |

---

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---------|----------|---------------------|:------:|:-----------:|:---------------:|:-------------------:|:--------:|:------------|-----------|
| Plan Mode 的触发方式是什么？ | Known unknown | Claude Code 有 `/plan` 命令进入 Plan Mode，也有自动检测场景进入。我们的 `/plan` 目前只是创建计划，不改变 Agent 行为模式 | 5 | 5 | 3 | 4 | 300 | **Decision** | 需确定：`/plan` 进入 Plan Mode 还是自动检测？Plan Mode 中 Agent 行为如何变化？ |
| 用户如何审批/修改计划？ | Known unknown | 当前 Plan 创建后自动审查通过，用户无感知。需要确定审批 UI 和交互流程 | 5 | 5 | 3 | 4 | 300 | **Decision** | 需确定：Desktop 弹窗审批？CLI 文本确认？支持修改计划还是仅批准/拒绝？ |
| Question Runtime 与现有 `ask.user` 工具的关系？ | Known unknown | 已有 `ask.user`/`ask.select` 工具，但无 Question 模块管理生命周期。是扩展工具还是新增 crate？ | 4 | 4 | 3 | 3 | 144 | **Decision** | 需确定：保留工具模式还是新增 Question crate？ |
| Todo 如何与 Plan 执行联动？ | Known unknown | Plan 的 Task/Step 执行时，Todo 需要实时更新进度。当前 Execution 无 Todo 集成 | 4 | 5 | 2 | 3 | 120 | **Experiment** | 需要原型验证：Todo 作为 Plan 执行的观察者还是独立模块？ |
| Reflection 如何与 Plan 执行结果集成？ | Known unknown | 执行完成后，Reflection 需要分析结果并可能触发重规划。当前 Execution 无 Reflection 钩子 | 4 | 3 | 3 | 3 | 108 | **Experiment** | 需要原型验证：Reflection 结果如何影响 Plan 状态？ |
| Plan Mode 中 Agent 的工具权限如何变化？ | Unknown unknown | Plan Mode 下 Agent 应只读（分析+规划），不执行修改。需要确定权限切换机制 | 4 | 4 | 2 | 3 | 96 | **Decision** | 需确定：Plan Mode 强制只读？还是可配置？ |
| 计划展示的格式和详细程度？ | Known unknown | Claude Code 展示结构化的 Step 列表，Codex 展示 Todo checklist。我们的 Plan 有四层结构，展示哪几层？ | 3 | 4 | 2 | 2 | 48 | **Decision** | 需确定：展示 Goal→Task→Step 三层？还是仅 Task 层？ |

---

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|-------------|-----------------|-------------------|
| Agent 执行前应该先展示计划 | Claude Code 的 Plan Mode、Codex 的 update_plan 都先展示计划再执行 | 对比竞品行为 |
| 用户应该能修改计划后再执行 | Claude Code 允许用户修改计划文本 | 对比 Claude Code 行为 |
| 执行过程中应显示进度 | Claude Code 的 Todo 列表、Codex 的 checklist | 对比竞品 UX |
| 高风险操作应询问用户 | Claude Code 的 AskUserQuestion、Codex 的 approval | 对比竞品安全机制 |
| 执行完成后应有总结 | Claude Code 的 summary、Codex 的 review | 对比竞品行为 |

---

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|-----------|-------------------|-------------------|
| Plan Mode 与现有 `/plan` 命令的兼容性 | 当前 `/plan` 是 Agent 路由的只读命令，改成 Plan Mode 后可能影响现有流程 | 代码审查 |
| 计划审批中的版本管理 | 用户修改计划后，需要增量版本还是重新生成？ | 原型验证 |
| Todo 与 Plan Task 的状态同步 | 如果 Plan 执行中 Task 被跳过/重试，Todo 如何反映？ | 原型验证 |
| 多用户协作场景下的审批 | 企业场景可能需要多人审批，当前单用户模式 | 设计评审 |
| Plan Mode 中 LLM Tool 调用的限制 | Plan Mode 下 LLM 应不调用有副作用的工具，如何确保？ | 安全审查 |

---

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|----------|---------|------------|-------------------|-------------------|
| Plan Mode 触发方式 | 1) `/plan` 命令进入 2) 自动检测复杂任务 3) 两者都有 | 1) 用户主动控制 2) 用户体验好但可能误触 3) 最灵活但实现复杂 | UX | 实现前 |
| 计划审批方式 | 1) 简单批准/拒绝 2) 支持修改计划文本 3) 支持逐条审批 Task | 1) 简单直接 2) 灵活但复杂 3) 最全面但 UX 重 | UX | 实现前 |
| Question 实现方式 | 1) 扩展现有 `ask.*` 工具 2) 新增 `core-agent-question` crate | 1) 改动小 2) 架构清晰但工作量大 | Architecture | 实现前 |
| 计划展示层级 | 1) 仅展示 Task 层 2) 展示 Task+Step 两层 3) 展示全部四层 | 1) 简洁 2) 信息适中 3) 信息完整但可能过于详细 | UX | 实现前 |

---

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|----------|--------|----------------|:----:|-------|
| Desktop 中 Plan 面板的交互原型？ | Vue 原型开发 | 用户能看懂计划结构并批准/拒绝 | Medium | Dev |
| CLI 中 Plan 模式的文本交互？ | TUI 原型开发 | 用户能通过文本确认/修改计划 | Medium | Dev |
| Question 与 Plan 的集成流程？ | 代码原型 | Plan 执行中遇到不确定时能暂停并询问用户 | Medium | Dev |

---

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|------------|----------------|---------------|
| Plan Mode 先从 `/plan` 命令进入 | 与现有命令兼容，不破坏现有流程 | 改为自动检测 |
| 审批先做简单的批准/拒绝 | 后端已有 PlanReview 模型，扩展成本低 | 增加修改功能 |
| Todo 作为 Plan 执行的观察者实现 | 不侵入 Execution 核心逻辑 | 改为独立模块 |
| 先实现 Desktop 端的 Plan 面板 | Desktop 用户占比高，UI 交互更直观 | 补充 CLI 端 |

---

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---------|-------------|----------------------|
| Multi-Agent 协作规划 | 超出当前 Plan Mode 范围 | P2 阶段考虑 |
| 自动重规划（Replan） | 超出当前 MVP 范围 | P5.4 阶段考虑 |
| 人工审批工作流（多人） | 超出当前单用户范围 | 企业版考虑 |
| LLM PlanBuilder | 当前 Rule Builder 可用 | 后续迭代 |

---

## Recommended Implementation Boundary

### Implement now

1. **Plan Mode 概念引入** — 新增 Plan Mode 模式切换，Agent 进入规划模式后只读分析+生成计划
2. **计划审批流程** — 计划生成后展示给用户，用户批准/拒绝后再执行
3. **Todo 进度展示** — 计划执行过程中显示已完成/进行中/待办的任务列表
4. **Question 集成** — Agent 在规划或执行过程中遇到不确定时主动询问用户

### Do not implement now

- 独立 `core-agent-question` crate（先在现有工具基础上扩展）
- 独立 `core-agent-todo` crate（先作为 Plan 的观察者实现）
- 独立 `core-agent-reflection` crate（先扩展认知命令）
- 自动重规划（Replan）
- Multi-Agent 协作规划

### Interfaces or data contracts to freeze

- `core-agent-plan` 的领域模型（Goal/Plan/Task/Step/Action/PlanningGraph）
- `core-agent-plan` 的 PlanStatus 生命周期
- `core-agent-execution` 的 Execution 接口
- `plan.create`/`plan.update`/`plan.review` tools 接口

### Areas that must remain reversible

- Plan Mode 的触发方式（命令 vs 自动）
- 计划审批的 UI 交互（Desktop 面板 vs CLI 文本）
- Todo 的展示格式（面板 vs 列表 vs 纯文本）

---

## Verification Plan

### Automated

- 单元测试: Plan Mode 状态切换、审批流程、Question 集成
- 集成测试: Plan → 审批 → Execution 完整链路
- E2E 测试: Desktop 计划面板交互、CLI 计划确认

### Manual

- Happy path: 输入目标 → 生成计划 → 展示 → 批准 → 执行 → 完成
- 修改路径: 输入目标 → 生成计划 → 展示 → 修改计划 → 批准 → 执行
- 拒绝路径: 输入目标 → 生成计划 → 展示 → 拒绝 → 重新规划
- 询问路径: 执行中遇到不确定 → 询问用户 → 用户回答 → 继续执行

### Observability

- 日志: Plan Mode 进入/退出、审批决策、Question 记录
- 指标: 计划审批通过率、计划修改率、用户询问频率

---

## Handoff

Convert resolved findings into:

- [ ] Plan Mode 概念与模式切换机制
- [ ] 计划审批流程（展示→批准/拒绝→执行）
- [ ] Todo 进度展示（Desktop 面板 + CLI 文本）
- [ ] Question 集成（Agent 主动询问）
- [ ] 对标分析文档（`docs/current/` 下）