继续按照我们的原则：

> **每一个 Phase 都是一个独立 Runtime，而不是一个功能。**

上一阶段 Session Runtime 已经解决了：

> Agent 活多久、如何保存、如何恢复。

那么接下来，真正决定 Agent 能力上限的就是：

> **Context Runtime**

实际上，我认为**Context Runtime 比 LLM Runtime 更重要**。

为什么？

因为：

> **LLM 只是 CPU，而 Context 才是真正的内存管理器（Memory Management）。**

Claude、Cursor、Codex、OpenCode、Grok Build……真正的竞争力，大部分都来自 Context，而不是 Model。

---

# Phase 1：Context Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责构建 Agent 每一次推理所需要的完整上下文（Context）。**

注意：

Context ≠ Prompt

Context 是：

```text
User Input
+
Conversation
+
Workspace
+
Memory
+
System Prompt
+
Environment
+
Plugin Context
+
Tool Result
```

最后：

```
↓

Prompt
↓

LLM
```

所以：

**Prompt 只是最终产物。**

---

# 为什么放在第二阶段？

因为：

所有后面的 Runtime：

```text
Model

Tool

Planner

Workflow

Memory

Plugin
```

最终：

都需要：

```text
ContextBuilder
```

如果：

Context 没设计好。

后面：

全部重写。

所以：

Context 必须先做。

---

# MVP目标

不要做：

❌ RAG

❌ Embedding

❌ 向量数据库

❌ Prompt Engineering

只做：

```text
Context

↓

Builder

↓

Composer

↓

Provider

↓

Reducer
```

---

# Runtime职责

Context Runtime：

只负责：

```text
收集

↓

整理

↓

裁剪

↓

排序

↓

生成 Context
```

不会：

调用 LLM。

---

# Runtime架构

建议：

```text
Context Runtime

│

├── ContextBuilder

├── ContextProvider

├── ContextComposer

├── ContextReducer

├── ContextSnapshot

└── ContextPipeline
```

这是整个 Runtime。

---

# 为什么不要只有 Builder？

OpenCode：

很多逻辑：

Builder：

越来越大。

最后：

几千行。

建议：

拆。

---

# 一、ContextBuilder

Builder：

负责：

```text
开始构建 Context
```

例如：

```text
Builder

↓

collect()

↓

compose()

↓

reduce()

↓

snapshot()

↓

return Context
```

Builder：

只负责流程。

不要：

真正收集数据。

---

# 二、ContextProvider

真正提供：

Context。

例如：

以后：

会有：

```text
ConversationProvider

WorkspaceProvider

MemoryProvider

PluginProvider

EnvironmentProvider

ToolProvider
```

统一：

```rust
trait ContextProvider
```

以后：

增加：

Provider。

零修改。

---

例如：

```text
Builder

↓

ConversationProvider

↓

WorkspaceProvider

↓

MemoryProvider

↓

EnvironmentProvider

↓

PluginProvider
```

---

# 三、ContextComposer

Composer：

负责：

排序。

例如：

不要：

```text
Conversation

Workspace

System
```

而是：

统一：

```text
System

↓

Environment

↓

Workspace

↓

Memory

↓

Conversation

↓

User
```

以后：

模型：

Claude

Gemini

OpenAI

不同。

这里只需要：

换 Composer。

---

# 四、ContextReducer

MVP：

一定要预留。

为什么？

以后：

Context：

100MB。

怎么办？

Reducer。

例如：

```text
超过限制

↓

删除旧消息

↓

压缩日志

↓

保留 Summary

↓

继续
```

MVP：

可以：

只有：

```text
Last N Message
```

以后：

升级。

---

# 五、ContextSnapshot

很多项目：

没有。

其实：

应该有。

例如：

一次：

Agent：

真正送给模型的是：

```json
{
    "system": "...",

    "history":[...],

    "workspace":[...],

    "memory":[...]
}
```

保存：

Snapshot。

以后：

Replay。

Debug。

Audit。

全部：

依赖。

---

# 六、ContextPipeline

Builder：

不要：

写：

```text
if

if

if

if
```

建议：

Pipeline。

例如：

```text
Conversation

↓

Workspace

↓

Memory

↓

Plugin

↓

Reducer

↓

Composer
```

以后：

增加：

Plugin。

不用：

改代码。

---

# Context对象

建议：

不要：

String。

而是：

```text
Context

├── SystemContext

├── ConversationContext

├── WorkspaceContext

├── MemoryContext

├── EnvironmentContext

├── PluginContext

└── UserContext
```

这样：

以后：

任何部分：

可以：

单独优化。

---

# ContextSource

建议：

每一个：

Context：

都有：

来源。

例如：

```text
SYSTEM

USER

PLUGIN

WORKSPACE

MEMORY

TOOL

ENVIRONMENT
```

以后：

Debug：

很好看。

---

# API设计

Builder：

```rust
build(session)
```

Provider：

```rust
collect()
```

Reducer：

```rust
reduce()
```

Composer：

```rust
compose()
```

Snapshot：

```rust
save()

load()
```

接口：

非常少。

---

# 生命周期

建议：

```text
Request

↓

Collect

↓

Compose

↓

Reduce

↓

Snapshot

↓

Context

↓

Destroy
```

注意：

不要：

长期保存：

Context。

重新生成。

这样：

最稳定。

---

# SQLite

MVP：

其实：

只有：

一张：

```text
context_snapshot
```

例如：

```text
id

session_id

conversation_id

created_at

content

token_count

hash
```

以后：

Debug：

Replay。

全部：

直接读取。

---

# UX设计

建议：

增加：

Context Inspector。

例如：

Agent：

右侧：

```
Context

────────────────────

✔ System

✔ Workspace

✔ Memory

✔ Conversation

✔ User
```

点击：

Conversation：

展开：

```
Message 1

Message 2

Message 3
```

点击：

Workspace：

```
pom.xml

User.java

README.md
```

以后：

用户：

知道：

Agent：

为什么：

回答这样。

---

再增加：

Token：

可视化。

例如：

```
Conversation

■■■■■■■■■ 62%

Workspace

■■■ 20%

Memory

■ 8%

Plugin

■ 5%

System

■ 5%
```

企业：

特别喜欢。

---

# MVP 不做什么

不要：

* ❌ Embedding
* ❌ Vector Database
* ❌ Knowledge Base
* ❌ Long Memory
* ❌ AI Summary
* ❌ 自动压缩
* ❌ RAG
* ❌ Graph Memory
* ❌ Multi-Agent Context

全部：

以后。

---

# 扩展点（第一版就预留）

```text
Context Runtime
│
├── ContextProvider      // Conversation、Workspace、Memory...
├── ContextComposer      // 模型适配
├── ContextReducer       // Token 裁剪策略
├── ContextSerializer    // JSON、MessagePack...
├── ContextSnapshotStore // SQLite、Postgres...
├── ContextCache         // 后续引入缓存
└── ContextObserver      // Trace、Metrics、Audit
```

---

# 企业版演进路线

我建议未来整个 Context Runtime 按下面的路线逐步升级，而不是一次性做复杂：

| Phase    | 能力                            | 为什么                      |
| -------- | ----------------------------- | ------------------------ |
| **P1.0** | Conversation + System Context | 最小可运行                    |
| **P1.1** | Workspace Context             | Coding Agent 必备          |
| **P1.2** | Environment Context           | Git、OS、Terminal 等环境感知    |
| **P1.3** | Plugin Context                | MCP、插件统一注入               |
| **P1.4** | Token Budget Manager          | 不同 Context 自动分配 Token 配额 |
| **P1.5** | Context Compression           | 摘要、裁剪、增量压缩               |
| **P1.6** | Context Cache                 | 避免重复构建，提高响应速度            |
| **P1.7** | Context Versioning            | 每次请求的 Context 可回放、可比较    |
| **P1.8** | Context Policy                | 企业级脱敏、过滤、权限控制            |
| **P1.9** | Intelligent Context Engine    | 根据任务类型自动选择最优 Context 组合  |

---

## 我建议增加一个比 OpenCode 和 Grok Build 都更通用的抽象

新增一个**Context Slot（上下文槽位）**概念。

不要让 Builder 直接拼接 Provider 的结果，而是让每一种 Context 先进入固定的 Slot：

```text
Context
│
├── System Slot
├── Environment Slot
├── Workspace Slot
├── Memory Slot
├── Conversation Slot
├── Tool Slot
├── Plugin Slot
└── User Slot
```

每个 Slot：

* 独立计算 Token
* 独立裁剪（Reducer）
* 独立排序（Priority）
* 独立启用/禁用
* 可配置预算（Budget）

这样未来新增一种 Context（例如 RCA Context、CMDB Context、Git Context）时，只需要增加一个新的 Slot 和对应 Provider，而无需修改 Builder 和 Composer。这种设计在企业级平台中扩展性会明显优于直接维护一个不断膨胀的 Context 列表。
