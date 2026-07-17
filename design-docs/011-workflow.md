我认为 **Workflow Runtime** 是整个 Agent 平台开始从「智能助手」演进为「自动化平台」的分水岭。

很多开源项目把 Workflow 理解为：

```text
Node
↓

Edge
↓

Done
```

或者：

```text
if

for

loop
```

**这只是流程编辑器（Flow Editor），不是 Workflow Runtime。**

真正的 Workflow 应该回答的是：

> **多个 Agent、多个 Tool、多个事件，如何按照业务规则持续协作完成目标？**

所以：

Workflow 不应该依赖 UI。

UI 只是它的一种表现形式。

---

# Phase 10：Workflow Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责业务流程的定义、编排、执行和治理，而不是绘制流程图。**

Workflow：

不知道：

* OpenAI
* Claude
* Tool

它只知道：

```text
Workflow

↓

Stage

↓

Activity

↓

Action

↓

Result
```

---

# 为什么放 P10？

因为：

现在：

整个 Agent Kernel 已经完成：

```text
Session

↓

Context

↓

Model

↓

Tool

↓

Workspace

↓

Planning

↓

Execution

↓

Agent

↓

Memory

↓

Event
```

现在：

终于：

可以：

让多个 Runtime：

一起：

工作。

---

# 第一性原理

Workflow：

不是：

Node。

真正：

应该：

```text
Business Goal

↓

Workflow

↓

Stage

↓

Activity

↓

Action
```

例如：

RCA：

```text
故障发生

↓

收集日志

↓

分析指标

↓

生成假设

↓

验证

↓

输出报告
```

这是：

Workflow。

不是：

节点。

---

# Runtime职责

Workflow：

只负责：

```text
定义流程

↓

调度流程

↓

维护状态

↓

等待事件

↓

继续流程
```

不要：

真正：

执行：

Tool。

Execution：

负责。

---

# Runtime架构

建议：

```text
Workflow Runtime

│

├── WorkflowManager

├── WorkflowRegistry

├── WorkflowEngine

├── WorkflowDefinition

├── WorkflowScheduler

├── WorkflowState

├── WorkflowPolicy

├── WorkflowSnapshot

├── WorkflowLifecycle

└── WorkflowObserver
```

---

# 一、WorkflowManager

唯一：

入口。

例如：

```rust
start()

pause()

resume()

cancel()
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、WorkflowRegistry

维护：

全部：

Workflow。

例如：

```text
RCA

CI/CD

Review

Approval

Release
```

统一：

注册。

以后：

Marketplace。

---

# 三、WorkflowDefinition

不要：

JSON。

建议：

对象。

例如：

```text
Workflow

├── Stage

├── Activity

├── Variables

├── Policy

└── Version
```

以后：

DSL。

YAML。

UI。

全部：

共用。

---

# 四、WorkflowEngine

真正：

运行。

例如：

```text
Stage

↓

Activity

↓

Execution Runtime

↓

Event

↓

Next
```

Engine：

不要：

自己：

执行：

Tool。

---

# 五、WorkflowScheduler

不要：

Engine：

自己：

调度。

例如：

以后：

支持：

```text
Sequential

Parallel

Conditional

Event Driven

Cron
```

统一：

Scheduler。

---

# 六、WorkflowState

建议：

独立。

例如：

```text
Running

Waiting Event

Paused

Completed

Failed
```

以后：

恢复：

非常：

容易。

---

# 七、WorkflowPolicy

企业：

必须。

例如：

```text
Timeout

Approval

Retry

Compensation

Concurrency
```

全部：

Policy。

---

# 八、WorkflowSnapshot

第一版：

预留。

例如：

```text
Workflow

↓

Snapshot

↓

Restore
```

以后：

升级。

恢复。

---

# 九、WorkflowLifecycle

生命周期：

建议：

```text
Created

↓

Scheduled

↓

Running

↓

Waiting

↓

Completed

↓

Archived
```

---

# Workflow对象

建议：

```text
Workflow

├── Identity

├── Definition

├── Variables

├── State

├── Version

├── Metadata

└── Policy
```

不要：

只有：

Nodes。

---

# Stage（重点）

我建议：

增加：

Stage。

不要：

Node。

例如：

```text
Workflow

↓

Collect

↓

Analyze

↓

Verify

↓

Report
```

每一个：

Stage：

里面：

再：

Activity。

以后：

企业：

非常：

舒服。

---

# Activity

例如：

```text
Collect

├── Read Logs

├── Read Metrics

├── Read Trace
```

真正：

Execution：

执行。

---

# API设计

Manager：

```rust
start()

pause()

resume()

cancel()
```

Engine：

```rust
run()
```

Scheduler：

```rust
schedule()
```

Registry：

```rust
register()

list()
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
Workflow

↓

Schedule

↓

Running

↓

Waiting Event

↓

Resume

↓

Completed
```

---

# SQLite

建议：

第一版：

```text
workflow

workflow_definition

workflow_instance

workflow_snapshot

workflow_state
```

五张。

---

# UX设计

左边：

增加：

```text
Workflow

────────────

RCA

Release

Approval

CI/CD
```

点击：

RCA：

例如：

```text
Workflow

────────────

Collect

↓

Analyze

↓

Verify

↓

Report
```

用户：

直接：

知道。

---

增加：

Stage：

例如：

```text
Collect

✓

Analyze

Running

Verify

Pending

Report

Pending
```

以后：

非常：

清晰。

---

增加：

Workflow Timeline：

```text
Started

↓

Collect

↓

Analyze

↓

Waiting Approval

↓

Continue
```

企业：

特别：

喜欢。

---

增加：

Workflow Variables：

例如：

```text
ticket=INC12345

host=server01

severity=P1
```

以后：

自动化。

全部：

支持。

---

# MVP 不做什么

不要：

* ❌ BPMN
* ❌ DAG Engine
* ❌ Visual Editor
* ❌ Multi Workflow
* ❌ Distributed Scheduler
* ❌ Cluster
* ❌ Compensation Engine
* ❌ Saga
* ❌ BPM Platform

以后。

---

# 扩展点（第一版就预留）

```text
Workflow Runtime
│
├── WorkflowEngine
├── WorkflowScheduler
├── WorkflowRegistry
├── WorkflowPolicy
├── WorkflowSnapshot
├── WorkflowVariableStore
├── WorkflowObserver
├── WorkflowInterceptor
└── WorkflowDSL
```

---

# 企业版演进路线

| Phase     | 能力                             | 为什么     |
| --------- | ------------------------------ | ------- |
| **P10.0** | Sequential Workflow            | MVP     |
| **P10.1** | Variables                      | 流程变量    |
| **P10.2** | Conditional Branch             | 条件分支    |
| **P10.3** | Parallel Stage                 | 并行执行    |
| **P10.4** | Event Trigger                  | 事件驱动    |
| **P10.5** | Human Approval                 | 人工审批    |
| **P10.6** | Workflow DSL                   | 声明式定义   |
| **P10.7** | Visual Designer                | 可视化编排   |
| **P10.8** | Distributed Workflow           | 分布式流程   |
| **P10.9** | Enterprise Automation Platform | 企业自动化平台 |

---

# 我建议增加一个比 OpenCode、n8n、Dify Workflow 更稳定的抽象

## 引入 Workflow Activity（活动层）

目前很多 Workflow 都是：

```text
Node

↓

Node

↓

Node
```

随着流程复杂，很快会出现几百个节点，维护困难。

我建议采用四层模型：

```text
Workflow
│
├── Stage（阶段）
│      ├── Activity（活动）
│      │      ├── Action（动作）
│      │      └── Action
│      │
│      └── Activity
│
└── Variables
```

例如，一个 RCA 流程：

```text
RCA Workflow
│
├── Stage：Collect
│      ├── Activity：Collect Logs
│      │      ├── Read Log Tool
│      │      └── Parse Log Tool
│      │
│      └── Activity：Collect Metrics
│             ├── Query Prometheus
│             └── Aggregate Metrics
│
├── Stage：Analyze
│      ├── Activity：Correlation Analysis
│      └── Activity：Hypothesis Generation
│
├── Stage：Verify
│      └── Activity：Execute Validation
│
└── Stage：Report
       └── Activity：Generate Report
```

这种设计相比传统 Node-Edge 模型有几个优势：

* **业务可读性更高**：业务人员讨论的是"采集、分析、验证、报告"，而不是几十个节点。
* **运行时更清晰**：Execution Runtime 执行的是 `Action`，Workflow Runtime 管理的是 `Stage/Activity`，职责分离。
* **复用能力更强**：一个 `Activity`（例如"发送通知"、"审批"、"收集日志"）可以在多个 Workflow 中复用。
* **更适合企业治理**：权限、审计、版本、SLA 可以配置在 Stage 或 Activity 层，而不是散落在每个节点上。

我认为，这种 **Workflow → Stage → Activity → Action** 的层次结构，比单纯的 Node-Edge 更适合作为长期演进的企业级 Agent Workflow Runtime。
