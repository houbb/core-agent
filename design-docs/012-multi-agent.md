到这里，我不会继续做 **Plugin Runtime**。

我建议调整一下顺序。

因为 Workflow 已经完成之后，真正缺的不是插件，而是：

> **Multi-Agent Runtime（多 Agent 协同运行时）**

原因很简单。

目前已经有：

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
Event
Workflow
```

现在已经可以运行：

* 一个 Agent
* 一个 Workflow

但是现实企业里面不会只有一个 Agent。

例如：

```text
RCA Agent

↓

Log Agent

↓

Metric Agent

↓

Trace Agent

↓

Knowledge Agent

↓

Report Agent
```

或者 Coding：

```text
Planner Agent

↓

Coder Agent

↓

Reviewer Agent

↓

Test Agent

↓

Document Agent
```

所以真正的下一层应该是：

# Phase 11：Multi-Agent Runtime ⭐⭐⭐⭐⭐

---

# 一句话

> **负责多个 Agent 的组织、协作、通信和治理。**

注意：

不是：

Agent Chat。

不是：

AutoGen。

不是：

CrewAI。

真正应该：

是：

Agent Operating System。

---

# 为什么很多 Agent Framework 后面越来越乱？

因为：

他们：

没有：

把：

Agent：

提升为：

Runtime。

于是：

Multi-Agent：

只能：

这样：

```text
Agent A

↓

Agent B

↓

Agent C
```

以后：

几百个 Agent：

直接：

炸。

---

真正：

应该：

```text
Organization

↓

Team

↓

Role

↓

Agent

↓

Execution
```

这是：

企业：

真正：

组织。

---

# Runtime职责

Multi-Agent：

只负责：

```text
创建 Agent

↓

组织 Agent

↓

调度 Agent

↓

Agent 通信

↓

协作完成 Goal
```

不要：

Planning。

不要：

Execution。

---

# Runtime架构

建议：

```text
Multi-Agent Runtime

│

├── AgentManager

├── TeamManager

├── OrganizationManager

├── RoleManager

├── CollaborationManager

├── AgentRouter

├── AgentPolicy

├── AgentDirectory

├── AgentLifecycle

└── AgentObserver
```

以后：

不会：

推翻。

---

# 第一性原理

不要：

Agent。

直接：

Team。

真正：

应该：

```text
Organization

↓

Team

↓

Role

↓

Agent
```

例如：

```text
Engineering

│

├── Coding Team

├── QA Team

├── DevOps Team
```

Coding Team：

```text
Planner

Coder

Reviewer

Tester
```

以后：

Marketplace。

直接：

安装。

---

# 一、OrganizationManager

以后：

支持：

```text
Company

Department

Project

Workspace
```

统一。

---

# 二、TeamManager

真正：

Team。

例如：

```text
RCA Team

Coding Team

Review Team

Support Team
```

以后：

共享。

---

# 三、RoleManager（重点）

不要：

Agent：

自己：

决定：

能力。

Role：

负责。

例如：

```text
Planner

Coder

Reviewer

Architect

Tester
```

Role：

以后：

可以：

换：

Agent。

---

例如：

```text
Role

↓

Planner

↓

Claude

---------

Role

↓

Coder

↓

Qwen
```

Agent：

可以：

替换。

---

# 四、CollaborationManager

真正：

协作。

例如：

```text
Planner

↓

Task

↓

Coder

↓

Reviewer

↓

Tester
```

以后：

全部：

这里。

---

# 五、AgentRouter

不要：

Agent：

自己：

找。

例如：

```text
Task

↓

Router

↓

Best Agent
```

以后：

Marketplace。

自动：

支持。

---

# 六、AgentDirectory

维护：

所有：

Agent。

例如：

```text
Coding

Capability

Online

Workspace
```

统一：

查询。

---

# 七、AgentPolicy

企业：

必须。

例如：

```text
Can Use Internet

×

---------

Workspace

Readonly

---------

Can Call Agent

✓
```

以后：

企业。

---

# 八、AgentLifecycle

建议：

```text
Created

↓

Idle

↓

Assigned

↓

Working

↓

Waiting

↓

Completed
```

统一。

---

# Agent对象

建议：

```text
Agent

├── Profile

├── Role

├── Capability

├── Workspace

├── Team

├── State

└── Metadata
```

---

# Team对象

建议：

```text
Team

├── Name

├── Goal

├── Members

├── Policy

├── Workspace

└── Metadata
```

以后：

Marketplace。

直接：

下载。

---

# API

Manager：

```rust
create_team()

join()

leave()
```

Router：

```rust
route()
```

Collaboration：

```rust
assign()

handover()
```

Directory：

```rust
lookup()
```

---

# 生命周期

建议：

```text
Create Team

↓

Assign Goal

↓

Planner

↓

Execution

↓

Review

↓

Finish
```

---

# SQLite

建议：

第一版：

```text
organization

team

agent_member

role

collaboration
```

五张。

---

# UX设计

左边：

增加：

```text
Teams

────────────

Coding

RCA

DevOps
```

点击：

Coding：

例如：

```text
Planner

↓

Coder

↓

Reviewer

↓

Tester
```

下面：

```text
Members

────────────

Claude

Qwen

Gemini
```

非常：

清晰。

---

增加：

Team Timeline：

```text
Planner

✓

Coder

Running

Reviewer

Pending
```

以后：

特别：

舒服。

---

增加：

Communication：

例如：

```text
Planner

↓

Task

↓

Coder

↓

Review
```

用户：

终于：

知道：

Agent：

在：

交流：

什么。

---

# MVP 不做什么

不要：

* ❌ Swarm
* ❌ Hive
* ❌ Consensus
* ❌ Voting
* ❌ Negotiation
* ❌ Distributed Agent
* ❌ Marketplace Discovery
* ❌ Agent Economy
* ❌ Autonomous Society

以后。

---

# 扩展点（第一版就预留）

```text
Multi-Agent Runtime
│
├── TeamManager
├── OrganizationManager
├── CollaborationManager
├── AgentRouter
├── AgentDirectory
├── AgentPolicy
├── AgentObserver
├── AgentProtocol
└── AgentInterceptor
```

---

# 企业版演进路线

| Phase     | 能力                            | 为什么            |
| --------- | ----------------------------- | -------------- |
| **P11.0** | Team Runtime                  | MVP，多 Agent 编组 |
| **P11.1** | Role Runtime                  | 角色与职责          |
| **P11.2** | Collaboration                 | 协作与任务移交        |
| **P11.3** | Capability Routing            | 根据能力选择 Agent   |
| **P11.4** | Shared Workspace              | 共享工作区          |
| **P11.5** | Shared Memory                 | 团队知识共享         |
| **P11.6** | Dynamic Team                  | 动态组队           |
| **P11.7** | Cross-Team Collaboration      | 跨团队协作          |
| **P11.8** | Organization Runtime          | 企业组织运行时        |
| **P11.9** | Autonomous Agent Organization | 自治 Agent 组织    |

---

# 我建议加入一个目前大多数框架都没有认真抽象的层

## Agent Protocol Runtime（Agent 通信协议）

目前很多 Multi-Agent 都是：

```text
Agent A

↓

Prompt

↓

Agent B
```

或者：

```text
JSON
```

这会导致：

* 无法审计
* 无法追踪
* 无法重放
* 无法治理

我建议所有 Agent 通信都基于统一协议，例如：

```text
Agent Message
│
├── Header
│     ├── Source Agent
│     ├── Target Agent
│     ├── Correlation ID
│     ├── Workflow ID
│     └── Priority
│
├── Intent
│
├── Payload
│
├── Context Reference
│
└── Signature（企业版）
```

然后整个通信流程变成：

```text
Planner
      │
      ▼
Agent Message
      │
      ▼
Agent Router
      │
      ▼
Coder
      │
      ▼
Execution Runtime
```

这样未来：

* 多 Agent
* 跨机器部署
* Remote Agent
* MCP Agent
* 企业审计
* 消息重放
* SLA 统计

都建立在统一协议之上，而不是 Prompt 拼接。

---

## 我还建议把最后两层调整为：

```text
P12  Extension Runtime（插件 / MCP / Provider）
P13  Enterprise Runtime（组织、安全、权限、审计、配额、多租户）
```

这样整个平台会形成一个非常清晰的四层结构：

```text
Kernel Runtime
(P0~P9)

↓

Automation Runtime
(P10 Workflow)

↓

Collaboration Runtime
(P11 Multi-Agent)

↓

Platform Runtime
(P12 Extension + P13 Enterprise)
```

这个分层会比目前大多数 Agent Framework 更稳定，也更适合作为长期维护的开源平台架构。
