这里开始，我会**调整一下路线**。

因为到了这里，如果继续按照很多 Agent 项目的做法：

```text
Planning
↓

LLM

↓

Tool

↓

LLM

↓

Tool
```

整个架构很快就会变成一个巨大的 `AgentLoop`。

**我建议不要设计 Agent Loop。**

应该设计：

> **Execution Runtime（执行运行时）**

这是整个 Runtime 的心脏。

以后：

* Planner
* Workflow
* Multi-Agent
* Human Approval
* Scheduler

全部共享这一套 Execution Engine。

这也是我认为比 OpenCode、Claude Code、Grok Build 更适合作为平台的一种抽象。

---

# Phase 6：Execution Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责执行 Plan，而不是生成 Plan。**

Planning：

负责：

```text
Goal

↓

Plan
```

Execution：

负责：

```text
Plan

↓

Execute

↓

Observe

↓

Continue

↓

Complete
```

所以：

Planning：

永远：

不要：

执行。

---

# 为什么必须单独 Runtime？

因为：

以后：

不仅：

Planner：

执行。

还有：

```text
Workflow

Schedule

Approval

Multi-Agent

Human

Event
```

全部：

需要：

Execution。

所以：

Execution：

必须：

独立。

---

# 第一性原理

Execution：

其实：

只有：

五件事情。

```text
拿到 Plan

↓

调度 Step

↓

执行 Action

↓

处理结果

↓

更新状态
```

结束。

---

# Runtime职责

Execution：

只负责：

```text
Schedule

↓

Dispatch

↓

Execute

↓

Observe

↓

Finish
```

不要：

生成：

Plan。

---

# Runtime架构

建议：

```text
Execution Runtime

│

├── ExecutionManager

├── ExecutionEngine

├── Dispatcher

├── ActionExecutor

├── StateMachine

├── CheckpointManager

├── RetryManager

├── RollbackManager

├── ExecutionObserver

└── ExecutionLifecycle
```

---

# 一、ExecutionManager

唯一：

入口。

例如：

```rust
execute(plan)

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

# 二、ExecutionEngine

真正：

执行：

Plan。

例如：

```text
Task

↓

Step

↓

Action

↓

Tool

↓

Result
```

以后：

Agent：

只有：

Engine。

---

# 三、Dispatcher

不要：

Engine：

自己：

调度。

Dispatcher：

独立。

以后：

支持：

```text
Sequential

Parallel

Priority

Round Robin

Dependency
```

不用：

改：

Engine。

---

# 四、ActionExecutor

不要：

Execution：

直接：

调用：

Tool。

统一：

Executor。

例如：

```text
Action

↓

Tool

↓

Workspace

↓

Result
```

以后：

Action：

不仅：

Tool。

还可能：

```text
Sleep

Approval

Event

Agent Call
```

所以：

Action：

必须：

抽象。

---

# 五、StateMachine（重点）

我认为：

Agent：

一定：

需要：

状态机。

例如：

```text
Pending

↓

Ready

↓

Running

↓

Waiting

↓

Retrying

↓

Paused

↓

Completed

↓

Failed

↓

Cancelled
```

以后：

任何：

Runtime：

共享。

---

# 六、CheckpointManager

第一版：

就要。

例如：

```text
Running

↓

Checkpoint

↓

Resume
```

以后：

桌面端：

关闭。

恢复。

全部：

依赖。

---

# 七、RetryManager

不要：

Tool：

自己：

Retry。

统一。

例如：

```text
Retry

↓

Linear

↓

Exponential

↓

Policy
```

以后：

全部：

共享。

---

# 八、RollbackManager

企业：

必须。

例如：

```text
Write File

↓

失败

↓

Rollback
```

以后：

Workflow：

直接：

支持。

---

# 九、ExecutionObserver

第一版：

预留。

例如：

```text
Step Started

↓

Tool Finished

↓

Checkpoint

↓

Retry

↓

Complete
```

Audit。

Trace。

Replay。

全部：

依赖。

---

# Action对象（重点）

这里：

我建议：

不要：

Step：

直接：

调用：

Tool。

应该：

增加：

Action。

```text
Plan

↓

Task

↓

Step

↓

Action

↓

Tool
```

为什么？

因为：

以后：

Action：

可能：

```text
Tool

Sleep

Delay

Approval

Event

Condition

Loop

Agent

Workflow
```

Tool：

只是：

一种：

Action。

这是整个架构最大的区别。

---

# Execution对象

建议：

```text
Execution

├── Plan

├── CurrentTask

├── CurrentStep

├── CurrentAction

├── State

├── Checkpoint

└── Metadata
```

---

# API设计

Manager：

```rust
execute()

pause()

resume()

cancel()
```

Dispatcher：

```rust
dispatch()
```

Engine：

```rust
run()
```

Checkpoint：

```rust
save()

restore()
```

Retry：

```rust
retry()
```

Rollback：

```rust
rollback()
```

---

# 生命周期

建议：

```text
Plan

↓

Dispatch

↓

Execute

↓

Observe

↓

Checkpoint

↓

Next

↓

Completed
```

不要：

Execution：

自己：

Planning。

---

# SQLite

建议：

第一版：

```text
execution

checkpoint

execution_state

retry

rollback
```

五张。

---

# UX设计

建议：

右边：

增加：

Execution。

例如：

```text
Execution

────────────

Running

↓

Task2

↓

Step5

↓

Action3
```

用户：

知道：

Agent：

现在：

干什么。

---

增加：

Execution Timeline：

```text
Read File

✓

↓

Modify

✓

↓

Run Test

Running

↓

Generate Report

Pending
```

以后：

Replay。

直接：

支持。

---

增加：

Checkpoint：

例如：

```text
Checkpoint

────────────

Step 12

10:20

Resume
```

桌面端：

关闭。

回来。

继续。

---

增加：

Execution Graph：

```text
Task1

 │

 ├────Step1

 │

 ├────Step2

 │

 └────Step3
```

以后：

Debug。

非常：

舒服。

---

# MVP 不做什么

不要：

* ❌ Parallel Execution
* ❌ Distributed Scheduler
* ❌ Workflow Engine
* ❌ Event Bus
* ❌ Human Approval
* ❌ DAG Scheduler
* ❌ Auto Scaling
* ❌ Queue
* ❌ Multi-Agent Dispatch

以后：

做。

---

# 扩展点（第一版就预留）

```text
Execution Runtime
│
├── Dispatcher              // 调度策略
├── ActionExecutor          // Action 执行器
├── RetryPolicy             // 重试策略
├── RollbackPolicy          // 回滚策略
├── CheckpointStore         // 检查点
├── ExecutionObserver       // Trace、Metrics
├── ExecutionPolicy         // 企业策略
├── StateMachine            // 状态机
└── ExecutionInterceptor    // Hook
```

---

# 企业版演进路线

| Phase    | 能力                          | 为什么         |
| -------- | --------------------------- | ----------- |
| **P6.0** | Sequential Execution        | MVP         |
| **P6.1** | Checkpoint                  | 可恢复执行       |
| **P6.2** | Retry Policy                | 自动恢复        |
| **P6.3** | Rollback                    | 安全执行        |
| **P6.4** | Parallel Dispatch           | 并行执行        |
| **P6.5** | Conditional Action          | 条件执行        |
| **P6.6** | Human Approval              | 人工确认        |
| **P6.7** | Distributed Execution       | 多节点执行       |
| **P6.8** | Workflow Integration        | Workflow 共用 |
| **P6.9** | Autonomous Execution Engine | 企业级执行引擎     |

---

# 我建议增加一个比 OpenCode、Claude Code、Grok Build 都更通用的抽象

## 引入 Command Runtime（命令运行时）

目前很多 Agent 的执行流都是：

```text
Step
↓

Tool
```

但实际上，Execution 真正执行的不应该是 Tool，而应该是 **Command**。

例如：

```text
Action
│
├── Command
│      ├── ToolCommand
│      ├── AgentCommand
│      ├── WorkflowCommand
│      ├── ApprovalCommand
│      ├── DelayCommand
│      └── EventCommand
│
└── Result
```

这样：

* Tool Runtime 只是 Command 的一种实现。
* Multi-Agent 变成 `AgentCommand`。
* Workflow Runtime 变成 `WorkflowCommand`。
* 人工审批变成 `ApprovalCommand`。
* 定时任务变成 `DelayCommand`。

Execution Engine 永远只执行 `Command`，而不关心底层是什么。这种抽象可以让整个执行层保持稳定，未来无论增加多少新能力，都不需要修改 Execution Engine 本身。这也是我认为整个 `core-agent` 能够长期演进、避免架构膨胀的关键设计。
