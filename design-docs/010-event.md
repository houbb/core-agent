这里开始，我会**调整一下整个路线**。

按照很多 Agent Framework，会开始做：

```text
Workflow
```

但是如果仔细分析：

目前我们已经有：

```text
Session
Context
Model
Tool
Workspace
Planning
Execution
Agent
Memory
```

**还缺少一个最重要的东西：**

> **Event Runtime（事件运行时）**

为什么？

因为前面所有 Runtime：

目前都是：

```text
A

↓

B

↓

C
```

直接调用。

以后：

系统越来越大：

```text
Agent

Workflow

Plugin

Desktop

Notification

Audit

Analytics

Billing

Marketplace
```

全部：

互相调用。

最后：

整个系统：

就是：

蜘蛛网。

所以：

真正应该出现的是：

> **Event Bus**

也就是：

整个系统开始：

**事件驱动（Event Driven）。**

这是整个平台从 MVP 走向企业级最关键的一步。

---

# Phase 9：Event Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责 Runtime 之间的解耦通信，而不是业务逻辑。**

例如：

不要：

```text
Tool

↓

Memory
```

应该：

```text
Tool

↓

Event

↓

Memory
```

Memory：

不知道：

Tool。

Tool：

不知道：

Memory。

---

# 第一性原理

很多项目：

Event：

就是：

```text
publish()

subscribe()
```

太浅。

真正：

应该：

```text
Event

↓

Routing

↓

Policy

↓

Delivery

↓

Replay
```

以后：

Audit。

Workflow。

Notification。

全部：

依赖。

---

# 为什么放这里？

因为：

现在：

Runtime：

越来越多。

如果：

继续：

直接：

调用。

以后：

一定：

重构。

---

# Runtime职责

Event：

只负责：

```text
Publish

↓

Route

↓

Dispatch

↓

Deliver

↓

Replay
```

不要：

业务。

---

# Runtime架构

建议：

```text
Event Runtime

│

├── EventManager

├── EventBus

├── EventRegistry

├── EventRouter

├── EventDispatcher

├── EventPolicy

├── EventReplay

├── EventLifecycle

└── EventObserver
```

---

# 一、EventManager

唯一：

入口。

例如：

```rust
publish()

subscribe()

unsubscribe()
```

其它：

Runtime：

全部：

调用。

---

# 二、EventBus

真正：

Bus。

例如：

```text
Execution Finished

↓

Bus

↓

Memory

↓

Notification

↓

Analytics
```

以后：

全部：

共享。

---

# 三、EventRegistry

不要：

String。

例如：

```text
ExecutionCompleted

ToolStarted

ToolFinished

MemoryCreated

PlanCreated

WorkspaceLoaded
```

统一：

Registry。

以后：

IDE：

自动提示。

---

建议：

事件：

都有：

Schema。

例如：

```rust
Event<T>
```

不是：

Map。

---

# 四、EventRouter

企业：

必须。

例如：

```text
Tool Event

↓

Memory

---------

Execution Event

↓

Audit

---------

Workspace Event

↓

Index
```

Router：

决定：

去哪。

---

# 五、EventDispatcher

真正：

发送。

例如：

以后：

支持：

```text
Sync

Async

Delayed

Priority
```

Dispatcher：

统一。

---

# 六、EventPolicy

企业：

必须。

例如：

```text
Sensitive

×

External

---------

Internal

✓

---------

Replay

✓
```

以后：

安全。

---

# 七、EventReplay

第一版：

预留。

例如：

```text
Execution

↓

Replay

↓

Debug
```

以后：

RCA。

直接：

支持。

---

# 八、EventLifecycle

生命周期：

建议：

```text
Created

↓

Published

↓

Dispatched

↓

Delivered

↓

Handled

↓

Archived
```

以后：

Trace。

---

# 九、EventObserver

第一版：

预留。

例如：

```text
Published

↓

Delivered

↓

Retry

↓

Dead Letter
```

以后：

Metrics。

Audit。

---

# Event对象

建议：

```text
Event

├── Id

├── Type

├── Source

├── Target

├── Payload

├── Metadata

├── Timestamp

└── Version
```

不要：

Map。

---

# EventSource

建议：

```text
Agent

Execution

Tool

Workspace

Planner

Memory

Plugin

Workflow
```

以后：

很好。

---

# API设计

Manager：

```rust
publish()

subscribe()
```

Bus：

```rust
dispatch()
```

Replay：

```rust
replay()
```

Router：

```rust
route()
```

Policy：

```rust
check()
```

---

# 生命周期

建议：

```text
Event

↓

Publish

↓

Route

↓

Dispatch

↓

Handle

↓

Archive
```

---

# SQLite

第一版：

建议：

```text
event

event_subscription

event_replay

event_policy

event_dead_letter
```

五张。

---

# UX设计

左边：

增加：

```text
Events

────────────

Execution

Workspace

Tool

Memory

Agent
```

点击：

Execution：

例如：

```text
ExecutionCompleted

Source

Execution

Time

10:21
```

下面：

```text
Subscribers

────────────

Memory

Audit

Analytics
```

全部：

透明。

---

增加：

Event Timeline：

```text
Tool Started

↓

Tool Finished

↓

Execution Completed

↓

Memory Created
```

以后：

Debug。

非常：

舒服。

---

增加：

Replay：

例如：

```text
Replay

────────────

Yesterday

Execution

Replay
```

企业：

非常：

喜欢。

---

# MVP 不做什么

不要：

* ❌ Kafka
* ❌ RabbitMQ
* ❌ Redis Stream
* ❌ Event Sourcing
* ❌ CQRS
* ❌ Distributed Bus
* ❌ Cluster Routing
* ❌ Cloud Event
* ❌ Message Queue

SQLite：

足够。

---

# 扩展点（第一版就预留）

```text
Event Runtime
│
├── EventBus
├── EventRouter
├── EventDispatcher
├── EventRegistry
├── EventPolicy
├── EventReplay
├── EventObserver
├── DeadLetterQueue
└── EventInterceptor
```

---

# 企业版演进路线

| Phase    | 能力                        | 为什么    |
| -------- | ------------------------- | ------ |
| **P9.0** | Local Event Bus           | MVP    |
| **P9.1** | Typed Event               | 强类型事件  |
| **P9.2** | Replay                    | 回放     |
| **P9.3** | Retry                     | 自动重试   |
| **P9.4** | Dead Letter               | 死信队列   |
| **P9.5** | Async Dispatch            | 异步派发   |
| **P9.6** | Distributed Event         | 分布式事件  |
| **P9.7** | Event Store               | 持久化事件  |
| **P9.8** | Event Streaming           | 流式事件   |
| **P9.9** | Enterprise Event Platform | 企业事件平台 |

---

# 我建议增加一个比 OpenCode、Claude Code 更值得提前设计的抽象

## 引入 Domain Event（领域事件）

不要：

```text
ToolFinished
```

这种：

技术事件。

而是：

区分：

```text
System Event
│
├── ToolFinished
├── SessionCreated
├── WorkspaceLoaded
└── MemoryStored

Domain Event
│
├── BugFixed
├── RCADetected
├── ReportGenerated
├── BuildSucceeded
└── DeploymentFailed
```

为什么？

因为：

Workflow：

Notification：

Analytics：

Audit：

真正：

关心的是：

```text
BugFixed
```

不是：

```text
ToolFinished
```

---

整个平台：

以后：

会形成：

```text
System Runtime

↓

System Event

↓

Domain Event

↓

Business Runtime
```

这样：

Agent Runtime：

与：

企业业务：

彻底：

解耦。

---

## 到这里，整个 Agent Kernel（内核）其实已经完成了

```text
P0  Session Runtime
P1  Context Runtime
P2  Model Runtime
P3  Tool Runtime
P4  Workspace Runtime
P5  Planning Runtime
P6  Execution Runtime
P7  Agent Runtime
P8  Memory Runtime
P9  Event Runtime
```

这 **P0～P9** 可以构成一个非常稳定的 Agent Kernel。

在此基础上，再向上构建：

```text
P10 Workflow Runtime
P11 Multi-Agent Runtime
P12 Plugin Runtime（MCP/Extension）
P13 Enterprise Runtime
```

这样，内核（Kernel）始终保持精简稳定，而高级能力作为上层 Runtime 逐步叠加，不会让底层架构随着功能增加而不断重构。这也是我建议作为长期开源项目的整体演进方向。
