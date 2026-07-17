到了这里，整个 Agent Runtime 开始进入真正的核心。

前面的三层已经完成：

```text
P0 Session Runtime
    ↓
P1 Context Runtime
    ↓
P2 Model Runtime
```

这时候很多项目都会直接开始写 Agent Loop。

**我认为这是一个错误。**

真正决定 Agent 和 ChatBot 区别的，不是 Loop。

而是：

> **Tool Runtime（工具运行时）**

没有 Tool：

就是 ChatGPT。

有 Tool：

才是真正的 Agent。

所以 Tool Runtime 应该作为整个系统最重要的 Runtime 之一，而且以后几乎不会推翻。

---

# Phase 3：Tool Runtime ⭐⭐⭐⭐⭐

> **一句话定位**

**负责统一管理、发现、调用、监控所有 Tool。**

它不知道：

* Session
* Planner
* LLM

它只知道：

> 有一个 Tool，需要执行。

结束。

---

# 第一性原理

很多项目：

Tool = Function

这是错误的。

真正应该是：

```text
Capability

↓

Tool

↓

Execution

↓

Result
```

Tool：

不是函数。

而是一种：

**能力（Capability）**。

例如：

```text
Read File

Write File

Search

Git

Terminal

Browser

SQL

HTTP

MCP
```

其实：

全部都是：

Tool。

---

# 为什么放在 P3？

因为：

前面：

已经拥有：

```text
Session

Context

Model
```

现在：

模型：

已经可以：

```text
Tool Call
```

于是：

下一层：

就是：

真正执行：

Tool。

---

# Runtime职责

只负责：

```text
发现 Tool

↓

注册 Tool

↓

调用 Tool

↓

返回 Result

↓

生命周期管理
```

不要：

Planner。

不要：

Workflow。

不要：

Memory。

---

# Runtime架构

建议：

```text
Tool Runtime

│

├── ToolManager

├── ToolRegistry

├── ToolExecutor

├── ToolProvider

├── ToolCatalog

├── ToolPermission

├── ToolValidator

├── ToolResultMapper

└── ToolLifecycle
```

不要：

全部：

写：

ToolService。

以后：

会炸。

---

# 一、ToolManager

唯一：

入口。

例如：

```rust
execute(tool_request)
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、ToolRegistry

负责：

维护：

全部：

Tool。

例如：

```text
Read File

Write File

Terminal

Git

Browser

MCP

Plugin
```

统一：

注册。

以后：

热更新。

---

建议：

```rust
register()

unregister()

list()

find()
```

---

# 三、ToolProvider

真正：

提供：

Tool。

例如：

以后：

来源：

```text
Builtin

MCP

Plugin

Workflow

Remote

HTTP
```

统一：

```rust
trait ToolProvider
```

以后：

增加：

Provider：

不用：

改 Runtime。

---

# 四、ToolExecutor

真正：

执行：

Tool。

例如：

```text
Tool

↓

Permission

↓

Validation

↓

Execute

↓

Result
```

不要：

Provider：

自己：

执行。

统一。

---

# 五、ToolCatalog

不要：

Registry：

管理：

Metadata。

Catalog：

单独。

例如：

```text
name

description

schema

version

category

icon

tags

capability
```

以后：

Marketplace：

直接：

读取。

---

# 六、ToolPermission

第一版：

就有。

以后：

Agent：

才能：

上线。

例如：

```text
Read File

✓

Write File

Ask

Delete File

Deny

Shell

Confirm
```

统一：

Permission。

---

# 七、ToolValidator

很多项目：

没有。

其实：

非常重要。

例如：

Tool：

参数：

```json
{
    "path":""
}
```

必须：

Validate。

以后：

全部：

统一。

---

# 八、ToolResultMapper

Tool：

返回：

五花八门。

统一：

转换。

例如：

```text
Terminal

↓

String

---------

Browser

↓

HTML

---------

Git

↓

Diff
```

统一：

```text
ToolResult
```

---

# 九、ToolLifecycle

建议：

不要：

Tool：

只有：

Execute。

而是：

生命周期。

例如：

```text
Created

↓

Ready

↓

Running

↓

Success

↓

Failed

↓

Cancelled
```

以后：

Trace。

Replay。

都有。

---

# Tool对象

建议：

```text
Tool

├── Identity

├── Schema

├── Permission

├── Capability

├── Provider

├── Metadata

└── Version
```

不要：

直接：

Function。

---

# ToolRequest

统一：

```text
ToolRequest

├── ToolId

├── Parameters

├── Session

├── Metadata

└── Timeout
```

以后：

Workflow：

直接：

调用。

---

# ToolResult

统一：

```text
ToolResult

├── Status

├── Content

├── Attachments

├── Usage

├── Error

└── Metadata
```

以后：

LLM：

统一：

消费。

---

# Capability

建议：

不要：

只有：

Category。

增加：

Capability。

例如：

```text
Filesystem

Terminal

Git

Search

Network

Browser

Database

AI

Workflow
```

以后：

Planner：

按：

Capability：

选 Tool。

不是：

名字。

---

# API设计

Manager：

```rust
execute()

cancel()

```

Registry：

```rust
register()

remove()

list()
```

Catalog：

```rust
find()

categories()
```

Executor：

```rust
invoke()
```

Permission：

```rust
check()
```

Validator：

```rust
validate()
```

---

# 生命周期

建议：

```text
ToolRequest

↓

Validate

↓

Permission

↓

Execute

↓

Result

↓

Complete
```

不要：

直接：

Execute。

---

# SQLite

建议：

第一版：

四张表：

```text
tool

tool_provider

tool_execution

tool_permission
```

以后：

直接：

升级。

---

# UX设计

建议：

Tool：

独立。

例如：

左边：

```text
Tools

──────────────

Filesystem

Git

Terminal

Browser

Search

SQL

Plugin

MCP
```

点击：

Filesystem：

```text
Read File

Write File

List Directory

Copy File

Delete File
```

以后：

Marketplace：

直接：

增加。

---

Tool：

详情：

```text
Read File

──────────────

Provider

Builtin

Permission

Allow

Timeout

30s

Version

1.0.0
```

以后：

不用：

改。

---

执行：

建议：

右侧：

增加：

```text
Tool Timeline

──────────────

Read File

120ms

↓

Terminal

800ms

↓

Git Diff

90ms
```

用户：

知道：

Agent：

干了什么。

---

再增加：

Tool Result：

例如：

```text
Result

──────────────

Status

Success

Latency

123ms

Output

README.md
```

企业：

非常喜欢。

---

# MVP 不做什么

不要：

* ❌ Planner
* ❌ Agent Loop
* ❌ Workflow
* ❌ Multi Tool Scheduling
* ❌ Parallel Tool
* ❌ Tool Cache
* ❌ Tool Retry Strategy
* ❌ Tool Marketplace
* ❌ Remote Agent

以后：

做。

---

# 扩展点（第一版就预留）

```text
Tool Runtime
│
├── ToolProvider         // Builtin、MCP、Plugin...
├── ToolRegistry         // 注册中心
├── ToolExecutor         // 执行器
├── ToolPermission       // 权限控制
├── ToolValidator        // 参数校验
├── ToolResultMapper     // 统一返回结构
├── ToolLifecycle        // 生命周期
├── ToolInterceptor      // 前后置拦截
├── ToolObserver         // Trace、Metrics、Audit
└── ToolPolicy           // 企业策略
```

---

# 企业版演进路线

| Phase    | 能力                       | 为什么           |
| -------- | ------------------------ | ------------- |
| **P3.0** | Builtin Tool             | MVP           |
| **P3.1** | Tool Schema              | 自动生成 Tool 描述  |
| **P3.2** | MCP Provider             | 接入 MCP Server |
| **P3.3** | Plugin Provider          | 插件提供 Tool     |
| **P3.4** | Tool Permission          | 企业权限控制        |
| **P3.5** | Parallel Execution       | 多 Tool 并行     |
| **P3.6** | Tool Cache               | 避免重复执行        |
| **P3.7** | Tool Policy              | 白名单、黑名单、额度    |
| **P3.8** | Marketplace              | Tool 市场       |
| **P3.9** | Distributed Tool Runtime | 分布式 Tool 执行   |

---

# 我建议增加一个比 OpenCode、Claude Code 更底层的抽象

这是我认为以后所有 Agent 都会需要的一层。

## 新增：Tool Capability Graph（工具能力图）

不要：

```text
Filesystem

Terminal

Git

Browser
```

平铺。

而是：

```text
Capability

│

├── FileSystem
│      ├── Read
│      ├── Write
│      └── Delete
│
├── Git
│      ├── Commit
│      ├── Diff
│      └── Branch
│
├── Network
│      ├── HTTP
│      ├── Download
│      └── Upload
│
└── Database
       ├── Query
       ├── Update
       └── Schema
```

然后：

Planner：

不是：

找：

```text
ReadFile
```

而是：

找：

```text
Capability:

FileSystem.Read
```

Registry：

自动：

找到：

```text
Builtin.ReadFile

或者

MCP.ReadFile

或者

Plugin.ReadFile
```

这样：

**Planner 与具体 Tool 解耦。**

未来：

* Tool 可以替换
* MCP 可以替换
* 插件可以替换
* 企业版可以按策略切换 Provider

整个 Agent Runtime 将真正具备**长期可扩展性**。这也是我认为相比目前大多数开源 Agent Framework，更适合作为企业级平台底座的一种设计。
