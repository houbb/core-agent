我觉得，这个项目甚至可以独立成一个开源项目，而不是作为 `core-ai` 的一部分。

名字可以先叫：

```
core-agent
```

定位不是「聊天」。

而是：

> 一个企业级 Agent Runtime（Agent Operating System）

它应该像 SpringBoot 一样，别人基于它开发各种 Agent。

---

# 第一性原理

不要照着 OpenCode 做。

而是先问：

> 一个 AI Agent，到底需要哪些"不可缺少"的运行时(Runtime)？

我认为可以抽象成下面几个层次：

```
Agent

↓

Planning

↓

Context

↓

Memory

↓

Tool

↓

Workspace

↓

Execution

↓

Permission

↓

Observation

↓

Plugin
```

几乎所有 Agent：

Claude Code

OpenCode

Cursor

Codex

Grok Build

……

最终都会落到这些 Runtime。

所以我的建议是：

不要按功能开发。

而是：

> 按 Runtime 开发。

这样以后任何能力都是插拔式。

---

# 整个 Roadmap

我建议分成 P0~P12。

---

# Phase 0：Session Runtime（MVP）⭐⭐⭐⭐⭐

为什么先做？

因为所有 Agent 第一件事情：

就是：

**有生命周期。**

Agent：

不是一次请求。

而是一段持续运行。

例如：

```
创建 Session

↓

保存 Conversation

↓

恢复 Session

↓

关闭
```

以后：

Checkpoint

Resume

Cloud Sync

全部依赖它。

---

需要实现：

```
Session

Conversation

Message

State

Metadata
```

接口：

```
SessionProvider

SessionStore

SessionSerializer
```

以后：

SQLite

MySQL

Redis

都只是实现。

---

为什么第一？

没有 Session，

Agent 根本不存在。

---

# Phase 1：Context Runtime ⭐⭐⭐⭐⭐

这是所有 Agent 最核心。

为什么？

LLM：

不会记忆。

Agent：

就是：

不断重建 Context。

所以：

真正重要的是：

```
User Prompt

+

History

+

Workspace

+

Memory

+

Environment

+

Instruction
```

最终：

生成：

```
Prompt
```

所以：

需要：

```
ContextBuilder

ContextProvider

PromptComposer

ContextReducer

ContextCompressor
```

以后：

Claude Context

Gemini Context

都只是不同实现。

---

# Phase 2：Model Runtime ⭐⭐⭐⭐⭐

真正和 LLM 通信。

需要：

```
Provider

↓

OpenAI

Claude

Gemini

DeepSeek

Qwen

Ollama
```

统一：

```
Chat()

Stream()

Embedding()

Vision()

ToolCall()
```

以后：

新增 Provider：

零改动。

---

# Phase 3：Tool Runtime ⭐⭐⭐⭐⭐

没有 Tool。

Agent：

就是 ChatBot。

所以：

Tool：

必须独立。

接口：

```
Tool

ToolExecutor

ToolRegistry

ToolResult

ToolSchema
```

以后：

Tool：

来源：

```
Builtin

MCP

Plugin

HTTP

Workflow
```

全部统一。

---

# Phase 4：Workspace Runtime ⭐⭐⭐⭐⭐

这是 OpenCode 非常优秀的地方。

Workspace：

不是目录。

而是：

Agent 工作空间。

里面：

```
Files

Git

Terminal

Search

Index

Snapshot
```

全部属于 Workspace。

以后：

IDE

CLI

Web

都是同一个 Workspace。

---

# Phase 5：Planning Runtime ⭐⭐⭐⭐⭐

为什么？

Agent：

真正区别：

不是 Tool。

而是：

Planner。

需要：

```
Task

↓

Plan

↓

Step

↓

Action

↓

Review
```

接口：

```
Planner

PlanExecutor

Reviewer
```

以后：

Tree Search

Reflection

Multi-Agent

都是 Planner。

---

# Phase 6：Execution Runtime ⭐⭐⭐⭐⭐

真正执行：

```
Plan

↓

Tool

↓

Retry

↓

Rollback

↓

Checkpoint
```

为什么独立？

以后：

Workflow

Approval

Interrupt

Human Review

全部在这里。

---

# Phase 7：Memory Runtime ⭐⭐⭐⭐☆

不要一开始做 RAG。

先：

Memory。

```
Short Memory

↓

Long Memory

↓

Knowledge

↓

Semantic Search
```

以后：

Vector DB

Graph

Knowledge Base

全部实现这个接口。

---

# Phase 8：Permission Runtime ⭐⭐⭐⭐⭐

企业一定需要。

例如：

```
Read File

Write File

Run Shell

Delete File

Network

Git Push
```

必须：

统一权限。

否则：

Agent：

不能上线。

---

# Phase 9：Plugin Runtime ⭐⭐⭐⭐⭐

这是未来。

不要：

把：

```
Planner

Tool

Provider

Prompt

Memory
```

写死。

全部：

Plugin。

接口：

```
Plugin

Lifecycle

Extension

Hook
```

以后：

Marketplace：

直接上线。

---

# Phase 10：Observation Runtime ⭐⭐⭐⭐⭐

企业级：

一定需要。

Agent：

每一步：

必须：

记录。

例如：

```
Prompt

Latency

Token

Cost

Error

Tool

Reasoning

Trace
```

以后：

OpenTelemetry。

Audit。

Replay。

全部依赖这里。

---

# Phase 11：Multi-Agent Runtime ⭐⭐⭐⭐☆

以后：

不是：

一个 Agent。

而是：

```
CEO

↓

Planner

↓

Coder

↓

Tester

↓

Reviewer
```

Agent：

互相调用。

接口：

```
AgentRegistry

AgentCall

TaskDispatch

Mailbox
```

以后：

CrewAI。

AutoGen。

Claude Team。

都属于这里。

---

# Phase 12：Enterprise Agent Platform ⭐⭐⭐⭐⭐

最后：

真正的平台。

包括：

```
Agent Marketplace

↓

Workflow

↓

Human Approval

↓

RBAC

↓

Billing

↓

Analytics

↓

Deployment

↓

Cluster

↓

Cloud
```

真正成为：

企业级 Agent OS。

---

# 我会进一步调整 OpenCode 的架构

OpenCode 目前很多能力还是按产品组织。

如果从**平台（Framework）**角度重新设计，我会把它拆成下面这些独立模块，每一个都只有一个职责：

```
core-agent
│
├── core-agent-session      (Session Runtime)
├── core-agent-context      (Context Runtime)
├── core-agent-model        (LLM Runtime)
├── core-agent-tool         (Tool Runtime)
├── core-agent-workspace    (Workspace Runtime)
├── core-agent-plan         (Planning Runtime)
├── core-agent-execution    (Execution Runtime)
├── core-agent-memory       (Memory Runtime)
├── core-agent-permission   (Permission Runtime)
├── core-agent-plugin       (Plugin Runtime)
├── core-agent-observation  (Trace Runtime)
├── core-agent-multi        (Multi-Agent Runtime)
│
└── core-agent-server
```

这样设计最大的价值，不是为了今天能实现一个简化版 OpenCode，而是为了**未来十年的可演进性**。

你之前规划的 `core-user`、`core-config`、`core-storage`、`core-workflow`、`core-openapi`、`core-ai`、`core-plugin` 等平台，都可以自然成为 `core-agent` 的底层依赖，而 `core-agent` 则成为所有 AI 应用（Coding Agent、RCA Agent、知识库 Agent、办公 Agent 等）的统一运行时。

**如果按你的整体平台路线，我会把 `core-agent` 放在 `core-ai` 之后，作为整个 AI 生态真正的“操作系统”。** 它不是一个应用，而是整个 AI 平台最核心的 Runtime。
