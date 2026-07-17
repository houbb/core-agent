到了这里，我会再次调整整个路线。

很多 Agent Framework 下一步会开始做：

```text
Memory
```

或者：

```text
Workflow
```

**我认为顺序都不太合理。**

因为现在已经有：

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
```

但是：

整个系统还缺少一个东西：

> **Agent Runtime（Agent 生命周期）**

为什么？

因为目前：

Execution：

只能执行：

一个 Plan。

但是：

真正的 Agent：

会：

```text
接收任务

↓

规划

↓

执行

↓

等待

↓

继续

↓

再次规划

↓

结束
```

这是：

Agent 自己的生命周期。

所以：

P7：

应该：

Agent Runtime。

而不是：

Memory。

---

# Phase 7：Agent Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责整个 Agent 的生命周期，而不是某一次执行。**

Execution：

负责：

一次：

Plan。

Agent：

负责：

整个：

生命周期。

---

# 第一性原理

很多：

Framework：

```text
Agent

=

LLM
```

这是错误。

真正：

应该：

```text
Agent

=

Identity

+

Capability

+

Runtime

+

Lifecycle
```

LLM：

只是：

Agent：

的大脑。

不是：

Agent。

---

# Runtime职责

Agent Runtime：

负责：

```text
启动

↓

接受 Goal

↓

调用 Planner

↓

调用 Execution

↓

观察结果

↓

结束
```

不要：

真正：

Tool。

不要：

真正：

Context。

---

# Runtime架构

建议：

```text
Agent Runtime

│

├── AgentManager

├── AgentRegistry

├── AgentProfile

├── AgentLifecycle

├── AgentCoordinator

├── AgentCapability

├── AgentPolicy

├── AgentObserver

└── AgentSnapshot
```

以后：

整个：

平台：

不会：

推翻。

---

# 为什么不要只有 Agent？

因为：

以后：

Agent：

越来越复杂。

必须：

拆。

---

# 一、AgentManager

唯一：

入口。

例如：

```rust
create()

start()

stop()

destroy()
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、AgentRegistry

负责：

维护：

全部：

Agent。

例如：

```text
Coding Agent

RCA Agent

DevOps Agent

SQL Agent

Document Agent
```

统一：

注册。

以后：

Marketplace。

直接：

共用。

---

# 三、AgentProfile（重点）

我认为：

Agent：

第一版：

就要：

Profile。

例如：

```text
Coding Agent

↓

Model

↓

Workspace

↓

Capability

↓

Policy
```

Profile：

以后：

Agent：

真正：

可以：

配置。

---

例如：

```yaml
agent:

name: Coding Agent

model: coding-fast

planner: coding

workspace: local

toolset:

- filesystem

- git

- terminal
```

以后：

Agent：

完全：

声明式。

---

# 四、AgentLifecycle

生命周期：

建议：

```text
Created

↓

Ready

↓

Running

↓

Waiting

↓

Paused

↓

Completed

↓

Destroyed
```

以后：

桌面端。

恢复。

直接：

支持。

---

# 五、AgentCoordinator

以后：

多：

Runtime：

统一：

协调。

例如：

```text
Goal

↓

Planning

↓

Execution

↓

Review

↓

Next Goal
```

Agent：

自己：

不用：

写：

if。

---

# 六、AgentCapability

不要：

Agent：

只有：

名字。

例如：

```text
Can Code

Can Search

Can Git

Can SQL

Can Browser

Can RCA
```

Planner：

以后：

直接：

按：

Capability。

---

# 七、AgentPolicy

企业：

必须。

例如：

```text
Can Delete

No

---------

Internet

Yes

---------

Workspace

Readonly
```

以后：

Agent：

上线。

---

# 八、AgentObserver

第一版：

预留。

例如：

```text
Agent Started

↓

Goal Created

↓

Execution

↓

Finished
```

Audit。

Trace。

Replay。

全部：

依赖。

---

# 九、AgentSnapshot

第一版：

就有。

例如：

```text
Agent

↓

Snapshot

↓

Restore
```

以后：

恢复。

Checkpoint。

全部：

依赖。

---

# Agent对象

建议：

```text
Agent

├── Identity

├── Profile

├── Capability

├── Runtime

├── Workspace

├── Policy

├── State

└── Metadata
```

不要：

只有：

Prompt。

---

# AgentProfile

建议：

```text
Profile

├── Model

├── Planner

├── Toolset

├── Workspace

├── Memory

├── Policy

└── Config
```

以后：

Marketplace。

直接：

下载。

---

# API设计

Manager：

```rust
create()

start()

stop()

destroy()
```

Registry：

```rust
register()

remove()

list()
```

Coordinator：

```rust
run()

next()

finish()
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
Agent

↓

Ready

↓

Goal

↓

Planning

↓

Execution

↓

Observe

↓

Next Goal

↓

Completed
```

Execution：

只是：

Agent：

里面：

一个阶段。

---

# SQLite

建议：

第一版：

```text
agent

agent_profile

agent_snapshot

agent_state

agent_policy
```

五张。

---

# UX设计

左边：

增加：

```text
Agents

────────────

Coding

RCA

DevOps

SQL

Document
```

点击：

Coding：

```text
Profile

────────────

Model

Claude

Planner

Coding

Workspace

Local
```

下面：

```text
Capability

────────────

Git

✓

Terminal

✓

Browser

✓
```

再下面：

```text
Policy

────────────

Delete

Ask

Network

Allow
```

Agent：

真正：

可配置。

---

增加：

Agent Timeline：

```text
Agent Started

↓

Goal

↓

Planning

↓

Execution

↓

Waiting

↓

Completed
```

以后：

企业：

特别：

喜欢。

---

增加：

Agent Monitor：

例如：

```text
Agent

────────────

Status

Running

Goal

3

Tasks

12

Runtime

8m
```

以后：

Dashboard。

直接：

复用。

---

# MVP 不做什么

不要：

* ❌ Multi-Agent
* ❌ Swarm
* ❌ Team
* ❌ Long Memory
* ❌ Workflow
* ❌ Human Collaboration
* ❌ Agent Marketplace
* ❌ Agent Cloud
* ❌ Distributed Agent

以后。

---

# 扩展点（第一版就预留）

```text
Agent Runtime
│
├── AgentProfile         // Agent 配置
├── AgentCapability      // 能力
├── AgentPolicy          // 企业策略
├── AgentLifecycle       // 生命周期
├── AgentObserver        // Trace、Metrics
├── AgentSnapshotStore   // 快照
├── AgentCoordinator     // Runtime 协调
├── AgentFactory         // Agent 创建
└── AgentInterceptor     // Hook
```

---

# 企业版演进路线

| Phase    | 能力                        | 为什么          |
| -------- | ------------------------- | ------------ |
| **P7.0** | Single Agent              | MVP          |
| **P7.1** | Agent Profile             | 可配置 Agent    |
| **P7.2** | Capability Management     | 能力声明         |
| **P7.3** | Policy                    | 企业安全策略       |
| **P7.4** | Agent Snapshot            | 恢复执行         |
| **P7.5** | Agent Template            | Agent 模板     |
| **P7.6** | Agent Marketplace         | Agent 分发     |
| **P7.7** | Agent Team                | 多 Agent 编组   |
| **P7.8** | Distributed Agent         | 多节点运行        |
| **P7.9** | Enterprise Agent Platform | 企业级 Agent 平台 |

---

# 我建议把 Agent 提升为一个真正的 Runtime 实体

相比很多框架，我更建议把 **Agent** 定义为一个**可部署单元（Deployable Unit）**，而不是一个内存对象。

例如：

```text
Agent Package
│
├── agent.yaml
├── profile.yaml
├── prompts/
├── tools/
├── policies/
├── workflows/
└── assets/
```

这样一个 Agent 可以：

* 在 CLI 中运行
* 在 Web 中运行
* 在 Desktop 中运行
* 在 Server 中运行
* 发布到 Marketplace
* 导出/导入
* 做版本管理

换句话说，**Agent 不再只是代码里的一个类，而是像 Docker Image、VS Code Extension 一样的可发布、可复用、可安装的软件单元**。

这是我认为长期来看，比目前大多数 Agent Framework 更适合作为企业级平台和开源生态基础的设计方向。

另外，我会对后续阶段的顺序做一个小调整，建议改为：

```text
P8  Memory Runtime        （长期记忆）
P9  Workflow Runtime      （流程编排）
P10 Event Runtime         （事件总线）
P11 Multi-Agent Runtime   （多 Agent 协同）
P12 Plugin Runtime        （插件生态）
P13 Enterprise Runtime    （企业能力）
```

这样各 Runtime 的依赖关系会更清晰，也更符合从 MVP 到企业级平台的自然演进路径。
