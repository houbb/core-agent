我认为到了 **Phase 3**，这里应该做一次**产品定位升级**。

前面的路线是：

```text
Phase 0
CLI

↓

Phase 1
Professional CLI

↓

Phase 2
Desktop Workspace
```

这三个阶段，本质上还是：

> **Developer Tool（开发工具）**

而 **Phase 3 开始，就不再是工具。**

而是：

> **Developer Platform（开发平台）**

这也是和 OpenCode 最大的分水岭。

---

# Phase 3：Agent Studio ⭐⭐⭐⭐⭐

一句话定位：

> **一个用于创建、调试、观察、运营 Agent 的 IDE。**

注意：

不是：

Agent Chat。

不是：

Workflow。

不是：

Prompt。

而是：

Agent IDE。

类似：

```text
VS Code

+

Figma

+

Postman

+

LangSmith

+

Docker Desktop
```

全部：

融合。

---

# 为什么是 Studio？

因为：

前面：

用户：

只能：

```text
使用 Agent
```

现在：

开始：

```text
设计 Agent

↓

调试 Agent

↓

观察 Agent

↓

优化 Agent

↓

发布 Agent
```

Agent：

第一次：

变成：

资产。

---

# Phase3 总体架构

```text
               Agent Studio

------------------------------------------------

Agent Designer

Workflow Designer

Prompt Studio

Memory Studio

Trace Studio

Knowledge Studio

Capability Studio

Model Studio

------------------------------------------------

Studio API

------------------------------------------------

core-agent

------------------------------------------------

Kernel
```

注意：

Studio：

没有：

AI。

全部：

core-agent。

---

# Studio 首页

不要：

聊天。

首页：

应该：

像：

JetBrains。

例如：

```text
+---------------------------------------------------------+

Projects

Recent Sessions

Favorite Agents

Running Tasks

----------------------------------------------------------

Quick Action

New Agent

New Workflow

Import Project

Open Session

----------------------------------------------------------

Runtime

Healthy

Model

Claude

Memory

Ready

```

这就是：

Studio。

---

# Studio 左侧导航

建议：

固定：

```text
🏠 Home

🤖 Agents

🧠 Memory

🔧 Capabilities

📦 Workspace

📚 Knowledge

🔄 Workflow

📊 Trace

⚙ Settings
```

以后：

Marketplace：

也是：

这里。

---

# ① Agent Designer ⭐⭐⭐⭐⭐

这是：

整个：

Studio：

核心。

不是：

Prompt。

而是：

Agent。

---

例如：

```text
Agent

----------------

Name

Coding Agent

Role

Architect

Model

Claude

Memory

Project Memory

Tools

Git

Shell

Filesystem

Prompt

Architect Prompt

Workflow

Coding Flow
```

点击：

保存。

Agent：

创建。

---

Agent：

以后：

就是：

产品。

---

# ② Prompt Studio ⭐⭐⭐⭐⭐

不要：

Prompt：

写：

文本。

建议：

版本化。

例如：

```text
Prompt

----------------

System Prompt

Variables

Template

History

A/B Test

```

以后：

Rollback。

直接。

---

# ③ Workflow Studio ⭐⭐⭐⭐⭐

这里：

不是：

n8n。

因为：

Workflow：

已经：

Runtime。

这里只是：

Designer。

例如：

```text
Trigger

↓

Agent

↓

Tool

↓

Approval

↓

Finish
```

底层：

还是：

Workflow Runtime。

---

建议：

VueFlow。

---

# ④ Memory Studio ⭐⭐⭐⭐⭐

我认为：

Memory：

一定：

需要：

GUI。

否则：

企业：

不敢。

例如：

```text
Memory

Facts

Experience

Preference

Knowledge

Semantic
```

支持：

编辑。

删除。

锁定。

---

例如：

```text
User

Always use Rust

Pinned

✓
```

---

# ⑤ Capability Studio ⭐⭐⭐⭐⭐

注意。

不要：

Plugin。

叫：

Capability。

例如：

```text
Filesystem

Version

1.0

Status

Running


Git

Version

2.0
```

以后：

Provider：

切换。

---

# ⑥ Knowledge Studio ⭐⭐⭐⭐☆

企业：

开始：

需要。

例如：

```text
Knowledge

Documents

Git

Wiki

Database

Notion
```

以后：

RAG。

这里。

---

# ⑦ Trace Studio ⭐⭐⭐⭐⭐

这个：

我认为：

整个：

Studio：

最好。

因为：

几乎：

没人：

做好。

例如：

一次：

Agent：

执行：

```text
User

↓

Planner

↓

Task

↓

Tool

↓

LLM

↓

Memory

↓

Response
```

全部：

Timeline。

点击：

展开：

Prompt。

Token。

Latency。

Cost。

---

甚至：

支持：

Flame Graph。

例如：

```text
Request

██████

Planning

██

Tool

████████

LLM

███████
```

非常：

舒服。

---

# ⑧ Model Studio ⭐⭐⭐⭐☆

例如：

```text
Providers

Claude

OpenAI

Gemini

Qwen

GLM

Ollama
```

切换：

Provider。

测试。

Benchmark。

---

# UX设计

我建议：

不要：

聊天软件。

建议：

IDE。

布局：

```text
+----------------------------------------------------------+

Sidebar

----------------------------------------------------------

Center

Workspace

----------------------------------------------------------

Bottom

Trace

Log

Terminal

----------------------------------------------------------

Status

Model

Token

Latency

Cost

```

和：

JetBrains。

一样。

---

# Workspace

Phase3：

正式：

支持：

Workspace。

例如：

用户：

可以：

保存：

```text
Coding Workspace

RCA Workspace

Knowledge Workspace
```

以后：

打开：

恢复：

布局。

---

# Studio API

新增：

```text
GET /agent

POST /agent

GET /workflow

POST /workflow

GET /memory

POST /memory

GET /trace

GET /knowledge

GET /capability
```

全部：

core-agent。

---

# MVP 不做

不要：

* 企业组织
* Marketplace
* Billing
* Team
* Cloud
* SaaS
* 审批
* 多租户

这些：

P4。

---

# Phase3 完成标准

此时：

AgentOS：

已经：

不是：

OpenCode。

而是：

Agent IDE。

例如：

用户：

可以：

```text
创建 Agent

↓

创建 Workflow

↓

配置 Memory

↓

调试 Agent

↓

查看 Trace

↓

分析 Cost

↓

发布 Agent
```

全部：

GUI。

---

# 我建议再做一个提升（我认为这是最大的差异化）

这里我会把 **Studio** 再往前推一步，做成 **Visual Runtime**。

什么意思？

不是：

```text
页面 → API → Runtime
```

而是：

```text
Runtime
     │
     ▼
Visual Node
     │
     ▼
Studio
```

例如：

Memory Runtime：

天然对应一个 Memory Panel。

Workflow Runtime：

天然对应一个 Workflow Panel。

Tool Runtime：

天然对应一个 Tool Panel。

Trace Runtime：

天然对应一个 Timeline Panel。

也就是说：

**每一个 Runtime 自己提供自己的可视化能力（Visual Descriptor）**。

Studio 不需要知道 Runtime 的细节，只负责：

```text
Runtime 注册
        │
        ▼
Visual Descriptor
        │
        ▼
自动生成 Panel
```

这样以后：

新增：

```text
Search Runtime
Browser Runtime
SQL Runtime
RCA Runtime
```

Studio：

几乎：

不用：

改代码。

新的 Runtime：

自动：

出现：

新的：

Panel。

---

## 我认为这是整个 AgentOS 最值得坚持的一个设计原则：

> **Runtime 负责能力，Studio 负责展示；Runtime 可插拔，Studio 自动组装。**

这样未来无论是 Desktop、Web，还是企业版控制台，都能共享同一套可视化协议，而不会随着 Runtime 越来越多导致 Studio 越来越难维护。这也是一个真正长期可演进的平台架构。
