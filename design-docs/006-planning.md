这里开始，我们进入整个 **Agent Runtime 的核心大脑**。

前面我们已经完成了：

```text
P0 Session Runtime      （生命周期）
P1 Context Runtime      （上下文）
P2 Model Runtime        （模型）
P3 Tool Runtime         （工具）
P4 Workspace Runtime    （工作空间）
```

很多 Agent Framework 到这里都会开始写：

```text
LLM
↓

Tool

↓

LLM

↓

Tool
```

然后称之为 Agent。

**我认为这是目前绝大多数开源项目最大的架构问题。**

真正缺少的是：

> **Planning Runtime（规划运行时）**

---

# 为什么 Planning Runtime 如此重要？

先理解一个事实：

**LLM 不会规划。**

LLM 每次只能回答：

> 下一步是什么。

真正的 Planner 要做的是：

```text
目标

↓

拆解任务

↓

生成计划

↓

执行顺序

↓

检查结果

↓

继续规划
```

也就是说：

Planner 才是真正的大脑。

Model：

只是 CPU。

Tool：

只是手。

Workspace：

只是世界。

Planner：

才是真正的 Agent。

---

# Phase 5：Planning Runtime ⭐⭐⭐⭐⭐

> 一句话：

**负责把 Goal 转换成可执行 Plan。**

以后：

Workflow

Coding

RCA

Automation

全部：

依赖。

---

# 第一性原理

很多项目：

Planner：

就是：

```text
List<Step>
```

这是错误的。

真正应该：

```text
Goal

↓

Plan

↓

Task

↓

Step

↓

Action
```

这是五层。

不要：

直接：

Step。

以后：

一定重构。

---

# Runtime职责

Planner：

只负责：

```text
理解目标

↓

生成计划

↓

维护计划

↓

更新计划

↓

结束计划
```

不要：

真正：

执行。

执行：

P6。

---

# Runtime架构

建议：

```text
Planning Runtime

│

├── PlanningManager

├── GoalManager

├── PlanBuilder

├── TaskManager

├── StepManager

├── PlanningStrategy

├── PlanReviewer

├── PlanSnapshot

└── PlanningLifecycle
```

以后：

不会：

推翻。

---

# 为什么不要只有 Planner？

因为：

Planner：

以后：

越来越大。

必须：

拆。

---

# 一、PlanningManager

唯一：

入口。

例如：

```rust
create_plan()

update_plan()

cancel_plan()

resume_plan()
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、GoalManager

不要：

Plan：

自己：

管理：

Goal。

Goal：

应该：

独立。

例如：

```text
修复 Bug

优化性能

分析 RCA

重构项目

写 README
```

全部：

Goal。

以后：

多 Goal。

---

Goal：

建议：

```text
Goal

├── Id

├── Title

├── Description

├── Priority

├── Status

├── Metadata

└── Constraints
```

不要：

String。

---

# 三、PlanBuilder

真正：

生成：

Plan。

例如：

```text
Goal

↓

Context

↓

Model

↓

Plan
```

Builder：

以后：

可以：

很多。

例如：

```text
Rule Planner

LLM Planner

Workflow Planner
```

统一：

Builder。

---

# 四、TaskManager

很多：

项目：

没有：

Task。

直接：

Step。

这是错误。

例如：

Goal：

```text
重构项目
```

Task：

```text
扫描代码

修改代码

运行测试

生成文档
```

Task：

是一层。

---

# 五、StepManager

Task：

下面：

才是：

Step。

例如：

```text
修改 User.java

修改 pom.xml

修改 README
```

不要：

直接：

Task。

---

# 六、PlanningStrategy

企业：

必须。

例如：

不同：

任务：

不同：

策略。

```text
Coding

↓

Tree Search

---------

RCA

↓

Hypothesis

---------

Workflow

↓

Rule

---------

Chat

↓

Simple
```

以后：

Planner：

自动：

切换。

---

# 七、PlanReviewer

第一版：

就有。

例如：

Plan：

生成：

之后：

Review。

以后：

Reflection。

Self Correct。

Human Review。

全部：

依赖。

---

# 八、PlanSnapshot

第一版：

预留。

例如：

```text
Plan

↓

Snapshot

↓

Resume
```

以后：

恢复：

非常方便。

---

# 九、PlanningLifecycle

生命周期：

建议：

```text
Created

↓

Planning

↓

Ready

↓

Executing

↓

Reviewing

↓

Completed
```

以后：

Replay。

Trace。

全部：

依赖。

---

# Plan对象

建议：

```text
Plan

├── Goal

├── Tasks

├── Strategy

├── Metadata

├── Status

└── Version
```

不要：

只有：

Step。

---

# Task对象

统一：

```text
Task

├── Id

├── Name

├── Steps

├── Status

├── Priority

└── Dependencies
```

以后：

并行：

直接：

支持。

---

# Step对象

建议：

```text
Step

├── Id

├── Action

├── Tool

├── Status

├── Retry

└── Metadata
```

以后：

Execution：

直接：

消费。

---

# Planning Graph（重点）

这里：

我建议：

**不要使用 List。**

而是：

Graph。

例如：

```text
Goal

 │

 ├──────────────┐

 │              │

Task A       Task B

 │              │

 ├──────┐       │

 │      │       │

Step1 Step2   Step3

 │

Action
```

以后：

并行。

依赖。

Rollback。

全部：

天然。

---

# API设计

Manager：

```rust
create()

update()

cancel()

resume()
```

Builder：

```rust
build()
```

Goal：

```rust
create_goal()

update_goal()
```

Reviewer：

```rust
review()
```

Snapshot：

```rust
save()

restore()
```

---

# 生命周期

建议：

```text
Goal

↓

Planning

↓

Plan

↓

Review

↓

Ready

↓

Execution
```

Execution：

P6。

---

# SQLite

建议：

第一版：

```text
goal

plan

task

step

plan_snapshot
```

五张。

够。

---

# UX设计

左边：

不要：

Conversation。

而是：

增加：

```text
Plan

────────────

Goal

↓

Tasks

↓

Steps
```

点击：

Goal：

```text
Goal

────────────

重构项目

Status

Planning

Priority

High
```

下面：

Task：

```text
Task

────────────

扫描代码

修改代码

测试

文档
```

点击：

Task：

展开：

```text
Step

────────────

Read File

Modify

Run Test
```

Agent：

真正：

透明。

---

增加：

Planning Timeline：

```text
Planning

────────────

Goal

✓

Task1

✓

Task2

Running

Task3

Pending
```

以后：

Workflow：

共用。

---

再增加：

Planning Graph：

例如：

```text
Goal

 │

 ├────Task1

 │

 ├────Task2

 │

 └────Task3
```

用户：

一眼：

知道：

Agent：

准备：

干什么。

---

# MVP 不做什么

不要：

* ❌ Tree Search
* ❌ Reflection
* ❌ Multi-Agent Planning
* ❌ Auto Replan
* ❌ Parallel Planning
* ❌ Workflow Engine
* ❌ DAG Scheduler
* ❌ RL Planner
* ❌ Graph Search

以后：

做。

---

# 扩展点（第一版就预留）

```text
Planning Runtime
│
├── GoalProvider           // Goal 来源
├── PlanningStrategy       // 不同规划算法
├── PlanBuilder            // 计划生成器
├── PlanReviewer           // 审查器
├── TaskScheduler          // 后续并行调度
├── PlanSnapshotStore      // 快照
├── PlanningObserver       // Metrics、Trace
├── PlanningPolicy         // 企业策略
└── PlanningInterceptor    // Hook
```

---

# 企业版演进路线

| Phase    | 能力                         | 为什么          |
| -------- | -------------------------- | ------------ |
| **P5.0** | Goal → Plan                | MVP          |
| **P5.1** | Task Layer                 | 多层任务拆解       |
| **P5.2** | Dependency Graph           | Task 依赖关系    |
| **P5.3** | Review                     | 计划审查         |
| **P5.4** | Replan                     | 动态调整计划       |
| **P5.5** | Parallel Plan              | 并行规划         |
| **P5.6** | Planning Policy            | 企业规划策略       |
| **P5.7** | Human Approval             | 人工审批         |
| **P5.8** | Multi-Agent Planning       | 多 Agent 协同规划 |
| **P5.9** | Autonomous Planning Engine | 自主规划引擎       |

---

# 我建议增加一个比 OpenCode、Claude Code、Grok Build 都更底层的抽象

## 引入 Intent Runtime（意图层）

目前大多数 Agent 都是：

```text
User

↓

Goal

↓

Plan
```

但企业场景中，一个用户请求往往包含多个不同意图。

例如：

> "帮我分析这个 Java 项目的性能问题，修复明显 Bug，然后生成一份优化报告。"

实际上应该先拆成：

```text
User Request

↓

Intent
├── Analysis
├── Fix
└── Report

↓

Goals
├── 分析性能
├── 修复问题
└── 生成报告

↓

Plans
```

因此我建议在 `Goal` 之前增加一个 **Intent** 抽象：

```text
Intent
│
├── Goals
│     ├── Plan
│     │     ├── Task
│     │     │      └── Step
```

这样做的价值是：

* 一次请求可以产生多个 Goal，而不是一个巨大 Goal。
* Planner 可以针对不同 Intent 使用不同策略（例如 RCA 用假设推理，Coding 用代码规划，Report 用文档生成）。
* Multi-Agent 可以按 Intent 分工，而不是拆 Step。
* Workflow Runtime 可以直接消费 Intent，形成长期稳定的企业自动化架构。

**这是我认为整个 Agent Runtime 未来最值得提前预留的一层抽象，也是后续 Human-Agent、Workflow、Enterprise Automation 能够自然融合的关键。**
