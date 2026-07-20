# Plan（计划+询问模式）模块现状与对标分析

> 对标 Claude Code Plan Mode、ChatGPT (Codex) update_plan、OpenCode 的规划能力，梳理当前 Plan 模块的现状、差距与补充路线。

---

## 一、当前 Plan 模块全景

### 1.1 核心模块：`core-agent-plan`（P5 Planning Runtime）

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

### 1.2 执行模块：`core-agent-execution`（P6 Execution Runtime）

| 组件 | 位置 | 状态 |
|------|------|:----:|
| Plan 消费链路 | `core-agent-execution/src/manager.rs` | ✅ 完整 |
| 步骤分发/重试/回滚/检查点 | `core-agent-execution/src/manager.rs` | ✅ 完整 |
| 状态机（Dispatch/Running/Completed/Failed） | `core-agent-execution/src/domain.rs` | ✅ 完整 |

### 1.3 内置工具

| 工具 | 位置 | 状态 |
|------|------|:----:|
| `plan.create` | `core-agent-tool/src/builtin/plan/create.rs` | ✅ 基础实现 |
| `plan.update` | `core-agent-tool/src/builtin/plan/update.rs` | ✅ 基础实现 |
| `plan.review` | `core-agent-tool/src/builtin/plan/review.rs` | ✅ 基础实现 |
| `ask.user` | `core-agent-tool/src/builtin/ask/user.rs` | ✅ 基础实现 |
| `ask.select` | `core-agent-tool/src/builtin/ask/select.rs` | ✅ 基础实现 |

### 1.4 认知命令

| 命令 | 位置 | 状态 |
|------|------|:----:|
| `/reason` | `src/cognitive.rs` | ✅ 完整 |
| `/think` | `src/cognitive.rs` | ✅ 完整 |
| `/hypothesis` | `src/cognitive.rs` | ✅ 完整 |
| `/critic` | `src/cognitive.rs` | ✅ 完整 |
| `/reflect` | `src/cognitive.rs` | ✅ 完整 |
| `/decision` | `src/cognitive.rs` | ✅ 完整（自动生成 ADR） |

### 1.5 Slash 命令

| 命令 | 路由 | 说明 | 状态 |
|------|------|------|:----:|
| `/plan` | Agent | 创建实施计划（只读） | ✅ 基础 |
| `/review` | Agent | 审查当前变更（只读） | ✅ 基础 |
| `/explain` | Agent | 解释代码（只读） | ✅ 基础 |
| `/test` | Agent | 运行/规划测试 | ⚠️ 入口 |
| `/fix` | Agent | 修复当前问题 | ⚠️ 入口 |
| `/refactor` | Agent | 重构目标 | ⚠️ 入口 |

### 1.6 架构集成

```
EnterpriseAgent (src/enterprise.rs)
    │
    ├── PlaningManager (core-agent-plan)  — 计划生成/管理/审查
    │
    ├── ExecutionManager (core-agent-execution)  — 计划执行
    │
    └── CognitiveCommand (src/cognitive.rs)  — 认知推理
```

---

## 二、竞品对标分析

### 2.1 Claude Code Plan Mode

Claude Code 的 Plan Mode 是核心体验之一：

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 用户输入目标 → 生成计划 | 用户输入需求，Agent 分析后生成计划 | ✅ 后端完整 |
| 计划展示 | 以结构化文本展示 Task/Step 列表 | ❌ 无展示层 |
| 用户审批 | 用户可批准/拒绝/修改计划 | ❌ 自动审批通过 |
| Todo 进度 | 执行中显示已完成/进行中/待办 | ❌ 无 Todo |
| 进入 Plan Mode | `/plan` 命令或自动检测 | ❌ 无模式切换 |
| Plan Mode 只读 | 模式中 Agent 不修改文件 | ❌ 无权限约束 |
| 退出 Plan Mode | 用户确认后进入执行模式 | ❌ 无模式切换 |
| 计划修改 | 用户可修改计划内容 | ❌ 无修改接口 |

### 2.2 ChatGPT (Codex) update_plan

Codex 的 `update_plan` 是一种轻量级任务管理：

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 任务列表展示 | 以 checklist 形式展示任务 | ❌ 无展示层 |
| 执行进度 | 已完成标记 ✅，进行中 ⏳，待办 ⬜ | ❌ 无进度展示 |
| 复杂任务规划 | 自动拆解大型任务为子任务 | ✅ 后端完整 |
| 审批机制 | 执行前询问用户确认 | ❌ 无审批流程 |
| 任务修改 | 用户可调整任务顺序或内容 | ❌ 无修改接口 |

### 2.3 OpenCode

OpenCode 规划能力较弱，主要是 Tool Runtime：

| 能力 | 描述 | 本项目是否已有 |
|------|------|:---:|
| 基本工具链 | File/Shell/Search/Git | ✅ 完整 |
| 简单规划 | 依靠 LLM 自身规划能力 | ✅ 后端更强 |
| 无 Plan Mode | 无专用规划模式 | 持平 |

---

## 三、差距分析

### 3.1 核心差距：缺少 Plan Mode 交互体验 🔴 P0

**Claude Code 的 Plan Mode 体验：**

```
用户: 帮我重构支付模块
                ↓
         ┌─ Plan Mode ─────────────────┐
         │  1. 分析现有支付接口          │
         │  2. 设计新支付流程            │
         │  3. 修改 PaymentService       │
         │  4. 修改 PaymentController    │
         │  5. 更新测试                  │
         │                               │
         │  [批准] [修改] [拒绝]         │
         └───────────────────────────────┘
                ↓ 用户批准
         ┌─ Execution ──────────────────┐
         │  ✅ 1. 分析现有支付接口       │
         │  ⏳ 2. 设计新支付流程         │
         │  ⬜ 3. 修改 PaymentService    │
         │  ⬜ ...                       │
         └───────────────────────────────┘
```

**我们的现状：**

```
用户: /plan 帮我重构支付模块
                ↓
         ┌─ Agent ──────────────────────┐
         │  Plan created for: 重构支付... │
         │  Goal: 重构支付模块            │
         │  Tasks: 5 个任务              │
         │  Status: Ready (已自动审批通过)  │
         └───────────────────────────────┘
                ↓ 直接执行
         ┌─ Execution ──────────────────┐
         │  执行中... (用户看不到进度)    │
         └───────────────────────────────┘
```

### 3.2 差距明细

| 差距 | 对标源 | 影响 | 优先级 | 当前状态 | 预估工作量 |
|------|:------:|------|:------:|:--------:|:----------:|
| **Plan Mode 模式切换** | Claude Code | 无法让用户在执行前审查和确认计划 | 🔴 P0 | ❌ 缺失 | 3-5 天 |
| **计划展示层** | CC/Codex | 用户看不到计划内容，无法感知 Agent 的规划 | 🔴 P0 | ❌ 缺失 | 3-5 天 |
| **用户审批流程** | CC/Codex | 计划自动通过，用户无控制权 | 🔴 P0 | ❌ 缺失 | 2-3 天 |
| **Todo 进度展示** | CC/Codex | 执行中用户看不到进度和状态 | 🟡 P1 | ❌ 缺失 | 3-5 天 |
| **Question 集成** | CC | Agent 遇到不确定时无法主动询问 | 🟡 P1 | ⚠️ 基础工具 | 2-3 天 |
| **Reflection 集成** | CC | 执行完成后无自我审查和总结 | 🟡 P1 | ⚠️ 认知命令 | 2-3 天 |
| **Plan 可修改** | CC | 用户不能修改计划内容 | 🟡 P1 | ❌ 缺失 | 2-3 天 |
| **Plan 可视化面板** | CC/Codex | Desktop 缺少计划面板 | 🟢 P2 | ❌ 缺失 | 5-8 天 |

### 3.3 已就绪的后端能力（无需改动）

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
ask.user / ask.select 工具   ✅ 基础实现
认知命令 (6个)               ✅ 完整
```

---

## 四、实施路线

### 第一阶段：Plan Mode 基础体验（P0 🔴）

**目标：** 让用户能在执行前看到计划并审批

1. **Plan Mode 模式切换**
   - 新增 `PlanMode` 概念：Agent 进入规划模式后只读
   - `/plan` 命令进入 Plan Mode，展示计划 → 等待用户确认
   - 用户确认后退出 Plan Mode 进入执行模式

2. **计划展示层**
   - Desktop：消息中展示结构化计划（Goal → Task → Step）
   - CLI：文本展示计划列表
   - 展示 Review 结果（Approved / Changes Required / Rejected）

3. **用户审批流程**
   - 计划生成后暂停，等待用户批准
   - 用户批准 → 进入 Execution
   - 用户拒绝 → 重新规划
   - 利用后端已有的 `PlanReview` + `ReviewDecision` 模型

### 第二阶段：Todo + Question 集成（P1 🟡）

**目标：** 执行中有进度可追踪，不确定时能询问用户

1. **Todo 进度展示**
   - Plan 执行时，Task 状态变化→实时更新 Todo
   - Desktop：消息面板中显示 Todo 列表
   - CLI：文本进度条

2. **Question 集成**
   - Agent 规划或执行中调用 `ask.user`/`ask.select` 询问用户
   - 用户回答后继续执行
   - 利用现有 `ask.user`/`ask.select` 工具

3. **Reflection 集成**
   - 执行完成后自动调用认知命令 `/reflect`
   - 生成执行总结展示给用户

### 第三阶段：Plan 可视化面板（P2 🟢）

**目标：** Desktop 端有完整的计划管理面板

1. **Desktop Plan 面板**
   - 侧边栏或独立面板显示当前计划
   - 任务列表 + 状态 + 进度条
   - 点击任务可查看详情

2. **计划修改**
   - 用户可修改计划内容（增删 Task）
   - 修改后重新生成 Plan

3. **历史计划**
   - 查看历史执行计划
   - 对比不同计划的执行结果

---

## 五、架构变更

### 5.1 新增 Plan Mode 流程

```
当前流程：
  用户输入 → Agent → 自动生成 Plan → 自动审查通过 → 执行

新增 Plan Mode 流程：
  用户输入 → Agent 进入 Plan Mode → 生成 Plan → 展示计划
       ↓
  用户审批 → [批准] → 退出 Plan Mode → 执行
           → [修改] → 修改计划 → 重新展示
           → [拒绝] → 重新规划 → 重新展示
```

### 5.2 新增组件

```
EnterpriseAgent (src/enterprise.rs)
    │
    ├── PlaningManager (core-agent-plan)  — 不变
    │
    ├── ExecutionManager (core-agent-execution)  — 不变
    │
    ├── PlanMode (新增概念)  — 模式切换 + 只读约束
    │   ├── enter_plan_mode()
    │   ├── show_plan()
    │   ├── wait_for_approval()
    │   └── exit_plan_mode()
    │
    ├── TodoObserver (新增)  — 计划执行进度观察者
    │   └── on_task_status_change()
    │
    └── QuestionIntegration (增强)  — 现有 ask.* 工具集成
        └── ask_user() / ask_select()
```

### 5.3 需要修改的文件

| 文件 | 修改内容 | 工作量 |
|------|----------|:------:|
| `src/enterprise.rs` | 新增 Plan Mode 模式切换逻辑 | 3 天 |
| `src/interaction.rs` | `/plan` 命令增强（Plan Mode 入口） | 1 天 |
| `agent-desktop/src/App.vue` | 计划展示 UI + 审批按钮 | 2 天 |
| `agent-desktop/src/controller.ts` | Plan Mode API 集成 | 1 天 |
| `agent-cli/src/tui.rs` | CLI 计划展示 + 文本确认 | 2 天 |
| `core-agent-tool/src/builtin/plan/` | 增强 plan tools 与 Plan Mode 联动 | 1 天 |

---

## 六、与现有架构的关系

```
                      ┌──────────────────────┐
                      │   User Input          │
                      │  /plan <goal>         │
                      └──────────┬───────────┘
                                 │
                      ┌──────────▼───────────┐
                      │   Plan Mode Entry     │  ← 新增
                      │  (src/enterprise.rs)   │
                      └──────────┬───────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
         ┌────▼────┐      ┌─────▼─────┐      ┌─────▼─────┐
         │ Plan     │      │  Show     │      │  Wait     │
         │ Generate │      │  Plan     │      │  Approval │
         │ (已有)    │      │  (新增)    │      │  (新增)    │
         └────┬─────┘      └───────────┘      └─────┬─────┘
              │                                      │
              └──────────────────┬───────────────────┘
                                 │ 批准
                      ┌──────────▼───────────┐
                      │  Execution            │
                      │  (core-agent-execution)│
                      └──────────┬───────────┘
                                 │
              ┌──────────────────┼──────────────────┐
              │                  │                  │
         ┌────▼────┐      ┌─────▼─────┐      ┌─────▼─────┐
         │ Todo     │      │  Question  │      │ Reflection│
         │ Observer │      │  (ask.*)   │      │ (cognitive)│
         │ (新增)    │      │  (已有)     │      │  (已有)    │
         └──────────┘      └───────────┘      └───────────┘
```

---

## 七、总结

### 一句话

**后端 Plan 能力（数据模型/生命周期/持久化/执行）已完整，但缺少 Plan Mode 交互体验，用户无法在执行前看到计划、审批计划、跟踪进度。**

### 核心差距

1. **Plan Mode 模式切换** — 无法让 Agent 进入"只规划不执行"的模式
2. **计划展示层** — 用户看不到计划内容
3. **用户审批流程** — 计划自动通过，用户无控制权
4. **Todo 进度展示** — 执行中看不到进度
5. **Question 集成** — 遇到不确定时无法主动询问

### 推荐实施路线

1. **P0（3-5 天）** — Plan Mode 模式切换 + 计划展示 + 审批流程
2. **P1（3-5 天）** — Todo 进度展示 + Question 集成 + Reflection 集成
3. **P2（5-8 天）** — Desktop 计划面板 + 计划修改 + 历史计划

### 三大竞品对比

| 维度 | Claude Code | Codex | OpenCode | 本项目（后端） | 本项目（交互） |
|------|:-----------:|:-----:|:--------:|:------------:|:------------:|
| 数据模型 | 简单 Step 列表 | 简单 Task 列表 | 无 | ✅ 五层结构 | ✅ |
| DAG 依赖 | 隐式 | 无 | 无 | ✅ PlanningGraph | ✅ |
| 审查机制 | 无 | 无 | 无 | ✅ PlanReview | ✅ |
| 快照/恢复 | 无 | 无 | 无 | ✅ PlanSnapshot | ✅ |
| 持久化 | 无 | 无 | 无 | ✅ SQLite | ✅ |
| Plan Mode | ✅ 完整 | ❌ | ❌ | ❌ | ❌ |
| 计划展示 | ✅ 结构化 | ✅ checklist | ❌ | ❌ | ❌ |
| 用户审批 | ✅ 可选 | ✅ 可选 | ❌ | ❌ | ❌ |
| Todo 进度 | ✅ 实时 | ✅ 实时 | ❌ | ❌ | ❌ |
| Question | ✅ AskUser | ✅ approval | ❌ | ⚠️ 工具 | ❌ |
| Reflection | ⚠️ 简单 | ❌ | ❌ | ⚠️ 命令 | ❌ |
| 可视化面板 | ✅ Desktop | ✅ Web | ❌ | ❌ | ❌ |

---

## 八、现有代码关键位置

| 功能 | 位置 | 说明 |
|------|------|------|
| Plan 领域模型 | `core-agent-plan/src/domain.rs` | 1044 行，所有核心类型 |
| Plan 管理器 | `core-agent-plan/src/manager.rs` | 1054 行，主逻辑 |
| 默认实现 | `core-agent-plan/src/defaults.rs` | RuleBuilder + 审查器 + 策略 |
| Plan 持久化 | `core-agent-plan/src/persistence/` | SQLite 5 表 |
| Execution 管理器 | `core-agent-execution/src/manager.rs` | 计划执行 |
| plan tools | `core-agent-tool/src/builtin/plan/` | create/update/review |
| ask tools | `core-agent-tool/src/builtin/ask/` | user/select |
| 认知命令 | `src/cognitive.rs` | 6 个认知命令 |
| 交互命令 | `src/interaction.rs` | 命令注册表 + @ 提及 |
| 企业运行时 | `src/enterprise.rs` | 持有 PlanningManager |
| 设计文档 | `design-docs/006-planning.md` | Planning Runtime 设计 |
| 设计文档 | `design-docs/042-core-ablity-p1-plan.md` | P1 智能增强设计 |
| 现状对标 | `docs/current/current-agent-v20260720.md` | Agent 现状分析 |
| 现状对标 | `docs/current/current-components-v20260720.md` | 组件现状分析 |
| Unknowns 报告 | `docs/current/current-plan-vs-cc-codex-unknowns-report.md` | 本任务 Unknowns 报告 |