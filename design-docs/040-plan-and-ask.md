# 目标

对标一下 opencode/claude-code/chatGPT(codex) 的核心能力

看一下我们目前在 plan(计划+询问模式) 模块的现状+差距

# 相关梳理文档

D:\_core_ai\core-agent\docs\current\current-plan-vs-cc-codex-unknowns-report.md

D:\_core_ai\core-agent\docs\current\current-plan-vs-cc-codex.md

---

# 一、Plan + Ask 模块全景

## 1.1 核心模块：`core-agent-plan`（P5 Planning Runtime）

| 组件 | 位置 | 状态 |
|------|------|:----:|
| 领域模型（Goal/Plan/Task/Step/Action） | `core-agent-plan/src/domain.rs` | ✅ 完整 |
| PlanningGraph（DAG 依赖图） | `core-agent-plan/src/domain.rs` | ✅ 完整 |
| PlanReview（审查机制） | `core-agent-plan/src/domain.rs` | ✅ 完整 |
| PlanSnapshot（快照/恢复） | `core-agent-plan/src/domain.rs` | ✅ 完整 |
| PlanningManager（主入口） | `core-agent-plan/src/manager.rs` | ✅ 完整 |
| RulePlanBuilder（默认规则生成器） | `core-agent-plan/src/defaults.rs` | ✅ 完整 |
| StructuralPlanReviewer（自动审查） | `core-agent-plan/src/defaults.rs` | ✅ 完整 |
| SQLite 持久化（5 表） | `core-agent-plan/src/persistence/` | ✅ 完整 |
| 乐观版本控制 | `core-agent-plan/src/manager.rs` | ✅ 完整 |

## 1.2 执行模块：`core-agent-execution`（P6 Execution Runtime）

| 组件 | 位置 | 状态 |
|------|------|:----:|
| Plan 消费链路 | `core-agent-execution/src/manager.rs` | ✅ 完整 |
| 步骤分发/重试/回滚/检查点 | `core-agent-execution/src/manager.rs` | ✅ 完整 |
| 状态机（Dispatch/Running/Completed/Failed） | `core-agent-execution/src/domain.rs` | ✅ 完整 |

## 1.3 内置工具（41 个）

| 工具 | 位置 | 状态 | 说明 |
|------|------|:----:|------|
| `plan.create` | `core-agent-tool/src/builtin/plan/create.rs` | ✅ 完整 | 连接 PlanningManager |
| `plan.update` | `core-agent-tool/src/builtin/plan/update.rs` | ✅ 完整 | 连接 PlanningManager |
| `plan.review` | `core-agent-tool/src/builtin/plan/review.rs` | ✅ 完整 | 连接 PlanningManager |
| `ask.user` | `core-agent-tool/src/builtin/ask/user.rs` | ✅ 完整 | 标记 user_input_required |
| `ask.select` | `core-agent-tool/src/builtin/ask/select.rs` | ✅ 完整 | 支持多选项 |
| `ask.confirm` | `core-agent-tool/src/builtin/ask/confirm.rs` | ✅ 完整 | 确认对话框 |
| `todo.add` | `core-agent-tool/src/builtin/todo/add.rs` | ⚠️ 基础 stub | 未连接 PlanningManager |
| `todo.list` | `core-agent-tool/src/builtin/todo/list.rs` | ⚠️ 基础 stub | 未连接 PlanningManager |
| `todo.update` | `core-agent-tool/src/builtin/todo/update.rs` | ⚠️ 基础 stub | 未连接 PlanningManager |

## 1.4 认知命令

| 命令 | 位置 | 状态 |
|------|------|:----:|
| `/reason` | `src/cognitive.rs` | ✅ 完整 |
| `/think` | `src/cognitive.rs` | ✅ 完整 |
| `/hypothesis` | `src/cognitive.rs` | ✅ 完整 |
| `/critic` | `src/cognitive.rs` | ✅ 完整 |
| `/reflect` | `src/cognitive.rs` | ✅ 完整 |
| `/decision` | `src/cognitive.rs` | ✅ 完整（自动生成 ADR） |

## 1.5 Plan Slash 命令

| 命令 | 路由 | 说明 | 状态 |
|------|------|------|:----:|
| `/plan-show <id>` | Runtime | 展示计划详情（Goal+Task+Step） | ✅ 完整 |
| `/plan-list` | Runtime | 列出所有计划 | ✅ 完整 |
| `/plan-approve <id>` | Runtime | 批准计划并启动执行（带面板输出） | ✅ 完整 |
| `/plan-reject <id>` | Runtime | 拒绝计划（添加 Rejected Review） | ✅ 新增 |
| `/plan-replan <id>` | Runtime | 从被拒绝计划的 Goal 重建计划 | ✅ 新增 |

## 1.6 架构集成

```
EnterpriseAgent (src/enterprise.rs)
    │
    ├── PlanningManager (core-agent-plan)  — 计划生成/管理/审查
    │
    ├── ExecutionManager (core-agent-execution)  — 计划执行
    │
    ├── CognitiveCommand (src/cognitive.rs)  — 认知推理
    │
    ├── plan-runtime tools (plan.*)  — 连接 PlanningManager 的工具
    │
    └── builtin tools (ask.* / todo.*)  — 询问 + 待办
```

---

# 二、竞品对标分析

## 2.1 Claude Code Plan Mode

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 用户输入目标 → 生成计划 | 用户输入需求，Agent 分析后生成计划 | ✅ 后端完整 |
| 计划展示 | 以结构化文本展示 Task/Step 列表 | ✅ 文本展示 |
| 用户审批 | 用户可批准/拒绝/修改计划 | ✅ 批准/拒绝/重建 |
| Todo 进度 | 执行中显示已完成/进行中/待办 | ⚠️ 工具 stub |
| 进入 Plan Mode | `/plan` 命令或自动检测 | ❌ 无模式切换 |
| Plan Mode 只读 | 模式中 Agent 不修改文件 | ❌ 无权限约束 |
| 退出 Plan Mode | 用户确认后进入执行模式 | ❌ 无模式切换 |
| 计划修改 | 用户可修改计划内容 | ❌ 无修改接口 |

## 2.2 ChatGPT (Codex) update_plan

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 任务列表展示 | 以 checklist 形式展示任务 | ✅ 文本 checklist |
| 执行进度 | 已完成标记 ✅，进行中 ⏳，待办 ⬜ | ✅ 文本展示 |
| 复杂任务规划 | 自动拆解大型任务为子任务 | ✅ 后端完整 |
| 审批机制 | 执行前询问用户确认 | ✅ 批准/拒绝/重建 |
| 任务修改 | 用户可调整任务顺序或内容 | ❌ 无修改接口 |

## 2.3 OpenCode

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 基本工具链 | File/Shell/Search/Git | ✅ 完整 |
| 简单规划 | 依靠 LLM 自身规划能力 | ✅ 后端更强 |
| 无 Plan Mode | 无专用规划模式 | 持平 |

---

# 三、实际差距分析（v2026-07-21 更新）

## 3.1 实际现状

```
用户: /plan 帮我重构支付模块
                ↓
         ┌─ Agent ──────────────────────┐
         │  Plan created for: 重构支付... │
         │  Goal: 重构支付模块            │
         │  Tasks: 5 个任务              │
         │  Status: Reviewing            │
         └───────────────────────────────┘
                ↓ 用户审查
         ┌─ User Decision ───────────────┐
         │  /plan-approve → 批准+执行     │
         │  /plan-reject  → 拒绝         │
         │  /plan-replan  → 重建计划     │
         └───────────────────────────────┘
```

## 3.2 差距明细（更新后）

| 差距 | 对标源 | 影响 | 优先级 | 当前状态 | 剩余工作量 |
|------|:------:|------|:------:|:--------:|:----------:|
| **Plan Mode 模式切换** | Claude Code | 无法让 Agent 进入"只规划不执行"模式 | 🔴 P0 | ❌ 缺失 | 3-5 天 |
| **Todo 连接 PlanningManager** | CC/Codex | todo 工具未从 Plan 读取真实进度 | 🟡 P1 | ⚠️ 工具 stub | 1-2 天 |
| **Question 与 Plan 集成** | CC | ask.* 工具未与 Plan 执行联动 | 🟡 P1 | ⚠️ 基础工具 | 2-3 天 |
| **Reflection 与 Plan 集成** | CC | 执行完成后无自动 Reflection | 🟡 P1 | ⚠️ 认知命令 | 2-3 天 |
| **Plan 可修改** | CC | 用户不能修改计划内容 | 🟡 P1 | ❌ 缺失 | 2-3 天 |
| **Plan 可视化面板** | CC/Codex | Desktop 缺少计划面板 | 🟢 P2 | ❌ 缺失 | 5-8 天 |

### 已补齐的差距（从 ❌→✅）

| 原本差距 | 实现方式 | 完成时间 |
|----------|---------|:--------:|
| 计划展示层 | `/plan-show` + `/plan-approve` 面板输出 | ✅ 已实现 |
| 用户审批流程 | `/plan-approve` + `/plan-reject` + `/plan-replan` | ✅ 已实现 |

## 3.3 已就绪的后端能力（无需改动）

```
后端能力                   状态
─────────────────────────────────────────────────
Goal→Plan→Task→Step→Action  ✅ 完整领域模型
DAG 依赖图                  ✅ 完整 PlanningGraph
PlanReview 审查模型          ✅ 完整 ReviewDecision
PlanSnapshot 快照/恢复        ✅ 完整
乐观版本控制                 ✅ 完整
SQLite 持久化                ✅ 完整
Execution 链路               ✅ 完整
ask.user / ask.select / ask.confirm  ✅ 完整
认知命令 (6个)               ✅ 完整
plan.show / plan.list       ✅ 完整
plan.approve / reject / replan  ✅ 完整
```

---

# 四、实施路线（更新）

## 第一阶段：✅ 已完成 — Plan 展示 + 审批流程

**目标：** 让用户能在执行前看到计划并审批

1. ✅ **计划展示层** — `/plan-show` 展示 Goal → Task → Step 结构
2. ✅ **用户审批流程** — `/plan-approve` 批准并执行
3. ✅ **用户拒绝流程** — `/plan-reject` 拒绝 + `/plan-replan` 重建
4. ✅ **面板输出增强** — 带 Emoji 的结构化计划展示

## 第二阶段：Todo 连接 + Question 集成（P1 🟡）

**目标：** 执行中有进度可追踪，不确定时能询问用户

1. **Todo 连接 PlanningManager**
   - todo 工具从 Plan 读取真实 Task 列表作为 Todo 项
   - todo 状态与 Plan 的 Task/Step 状态同步
   - 执行中自动更新 Todo 进度

2. **Question 集成**
   - Agent 规划或执行中调用 `ask.user`/`ask.select` 询问用户
   - 用户回答后继续执行

3. **Reflection 集成**
   - 执行完成后自动调用认知命令 `/reflect`
   - 生成执行总结展示给用户

## 第三阶段：Plan Mode + 可视化面板（P1 🟡 → P2 🟢）

**目标：** 完整的 Plan Mode 交互 + Desktop 面板

1. **Plan Mode 模式切换**
   - 新增 `PlanMode` 概念：Agent 进入规划模式后只读
   - `/plan` 命令进入 Plan Mode，展示计划 → 等待用户确认
   - 用户确认后退出 Plan Mode 进入执行模式

2. **Desktop Plan 面板**
   - 侧边栏或独立面板显示当前计划
   - 任务列表 + 状态 + 进度条

3. **计划修改** — 用户可修改计划内容

---

# 五、架构现状

## 5.1 当前流程

```
用户输入 → Agent → 生成 Plan → 自动审查
    ↓
┌─ User Decision ───────────────┐
│  /plan-approve → 批准+执行     │
│  /plan-reject  → 拒绝         │
│  /plan-replan  → 重建计划     │
└───────────────────────────────┘
    ↓
执行 → 完成
```

## 5.2 当前组件

```
EnterpriseAgent (src/enterprise.rs)
    │
    ├── PlanningManager (core-agent-plan)  — 计划生成/管理/审查
    │
    ├── ExecutionManager (core-agent-execution)  — 计划执行
    │
    ├── CognitiveCommand (src/cognitive.rs)  — 认知推理
    │
    ├── plan-runtime tools: plan.create/update/review  — 连接 PlanningManager
    │
    ├── builtin tools: ask.user/confirm/select  — 询问用户
    │
    ├── builtin tools: todo.add/list/update  — 待办（stub）
    │
    └── Plan Slash Commands: plan-show/list/approve/reject/replan  — 用户交互
```

## 5.3 关键文件

| 功能 | 位置 | 说明 |
|------|------|------|
| Plan 领域模型 | `core-agent-plan/src/domain.rs` | 1044 行，所有核心类型 |
| Plan 管理器 | `core-agent-plan/src/manager.rs` | 1054 行，主逻辑 |
| 默认实现 | `core-agent-plan/src/defaults.rs` | RuleBuilder + 审查器 + 策略 |
| Plan 持久化 | `core-agent-plan/src/persistence/` | SQLite 5 表 |
| Execution 管理器 | `core-agent-execution/src/manager.rs` | 计划执行 |
| plan tools | `core-agent-tool/src/builtin/plan/` | create/update/review |
| ask tools | `core-agent-tool/src/builtin/ask/` | user/confirm/select |
| todo tools | `core-agent-tool/src/builtin/todo/` | add/list/update |
| 认知命令 | `src/cognitive.rs` | 6 个认知命令 |
| 交互命令 | `src/interaction.rs` | 命令注册表 + @ 提及 |
| 企业运行时 | `src/enterprise.rs` | 持有 PlanningManager |
| 设计文档 | `design-docs/006-planning.md` | Planning Runtime 设计 |
| 设计文档 | `design-docs/042-core-ablity-p1-plan.md` | P1 智能增强设计 |
| 现状对标 | `docs/current/current-agent-v20260720.md` | Agent 现状分析 |
| 现状对标 | `docs/current/current-plan-vs-cc-codex.md` | Plan 对标分析 |

---

# 六、总结

## 一句话

**后端 Plan 能力完整，展示层+审批流程已补齐，Todo 工具 stub 需要连接 PlanningManager，Plan Mode 概念需要架构变更。**

## 已完成

1. ✅ **计划展示层** — `/plan-show` 展示 Goal → Task → Step
2. ✅ **用户审批流程** — 批准(`/plan-approve`)/拒绝(`/plan-reject`)/重建(`/plan-replan`)
3. ✅ **面板输出** — 带 Emoji 的结构化计划展示
4. ✅ **plan.* 工具** — 全部连接 PlanningManager

## 待完成

1. 🔴 **Todo 连接 PlanningManager** — 让 todo 工具从 Plan 读取真实进度
2. 🔴 **Plan Mode 模式切换** — Agent 进入规划模式后只读
3. 🟡 **Question 与 Plan 集成** — ask.* 工具与 Plan 执行联动
4. 🟡 **Reflection 集成** — 执行完成后自动 Reflection
5. 🟡 **Plan 可修改** — 用户可修改计划内容