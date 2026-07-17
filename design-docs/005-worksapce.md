到了 **P4**，开始进入整个 Agent Runtime 的**真正灵魂**。

很多开源项目（包括很多 Agent Framework）都会在 P3 后直接写：

```text
while(true){
   LLM
   Tool
   LLM
   Tool
}
```

这就是所谓 Agent Loop。

**我不建议。**

因为这里缺少一个最重要的抽象：

> **Workspace Runtime**

实际上，OpenCode、Claude Code、Cursor、Grok Build，都有一个共同特点：

> **Agent 并不是在"聊天"，而是在一个 Workspace 中工作。**

Workspace 才是整个 Agent 的世界（World）。

---

# Phase 4：Workspace Runtime ⭐⭐⭐⭐⭐

> **一句话定位**

> **Workspace Runtime = Agent 对外部世界（Environment）的统一抽象。**

Agent：

不能直接：

```text
File

Git

Shell

Project

Index

Terminal
```

否则：

整个 Runtime：

会越来越耦合。

---

# 第一性原理

很多项目认为：

Workspace = 文件夹

这是最大的错误。

真正应该是：

```text
Workspace

↓

Project

↓

Environment

↓

Resources

↓

Capabilities
```

Workspace：

其实就是：

Agent 的：

Operating Environment（运行环境）。

---

# 为什么放 P4？

因为：

Tool：

已经有了。

现在：

Agent：

终于：

开始：

真正工作。

例如：

```text
读取项目

↓

修改文件

↓

Git

↓

Terminal

↓

运行测试
```

这些：

都发生：

Workspace。

---

# Runtime职责

只负责：

```text
Workspace 生命周期

↓

资源管理

↓

项目管理

↓

环境发现

↓

统一访问
```

不要：

Planner。

不要：

Memory。

不要：

LLM。

---

# Runtime架构

建议：

```text
Workspace Runtime

│

├── WorkspaceManager

├── WorkspaceRegistry

├── WorkspaceProvider

├── ProjectManager

├── ResourceManager

├── EnvironmentManager

├── WorkspaceSnapshot

├── WorkspaceIndex

└── WorkspaceLifecycle
```

---

# 为什么不要只有 Workspace？

因为：

以后：

Workspace：

越来越大。

必须：

拆。

---

# 一、WorkspaceManager

唯一：

入口。

例如：

```rust
open()

close()

reload()

snapshot()

list()
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、WorkspaceRegistry

负责：

维护：

全部：

Workspace。

例如：

```text
Java Project

Python

Rust

Docker

Remote

GitHub
```

统一：

注册。

以后：

多 Workspace。

---

# 三、WorkspaceProvider

真正：

提供：

Workspace。

例如：

以后：

来源：

```text
Local

Remote

Docker

SSH

GitHub

Cloud

ZIP
```

统一：

```rust
trait WorkspaceProvider
```

以后：

新增：

Provider。

不用：

改 Runtime。

---

# 四、ProjectManager

不要：

Workspace：

自己：

管理：

Project。

例如：

```text
Workspace

↓

Project

↓

Module

↓

File
```

以后：

Maven。

Gradle。

Cargo。

统一。

---

# 五、ResourceManager

整个：

Workspace：

里面：

不仅：

File。

还有：

```text
File

Directory

Image

PDF

Markdown

Binary

Terminal

Database
```

统一：

Resource。

以后：

Tool：

全部：

操作：

Resource。

不是：

File。

---

# 六、EnvironmentManager

这是：

OpenCode：

没有：

抽象好的地方。

建议：

独立。

例如：

```text
OS

Shell

Git

Language

Runtime

Docker

Python

Node

Java
```

Agent：

不用：

自己：

探测。

统一：

Environment。

---

# 七、WorkspaceSnapshot

第一版：

就有。

例如：

```text
Workspace

↓

Snapshot

↓

Restore
```

以后：

Undo。

Checkpoint。

Replay。

全部：

依赖。

---

# 八、WorkspaceIndex

不要：

以后：

RAG：

再做。

第一版：

预留。

例如：

```text
File Index

Project Index

Git Index

Symbol Index
```

以后：

搜索：

极速。

---

# 九、WorkspaceLifecycle

建议：

生命周期：

```text
Created

↓

Loaded

↓

Ready

↓

Modified

↓

Snapshot

↓

Closed
```

以后：

恢复：

非常方便。

---

# Workspace对象

建议：

```text
Workspace

├── Identity

├── Provider

├── Project

├── Environment

├── Resources

├── Metadata

└── State
```

不要：

path。

结束。

---

# Resource对象

统一：

```text
Resource

├── Id

├── Type

├── URI

├── Metadata

├── Capability

└── Provider
```

以后：

所有：

Tool。

统一。

---

# Environment对象

例如：

```text
Environment

├── OS

├── Git

├── Shell

├── Language

├── Runtime

├── Package Manager

└── Variables
```

以后：

Context：

直接：

引用。

---

# API设计

Workspace：

```rust
open()

close()

reload()

snapshot()
```

Registry：

```rust
register()

list()

find()
```

Provider：

```rust
load()
```

Project：

```rust
scan()

refresh()
```

Environment：

```rust
detect()
```

Index：

```rust
build()

search()
```

---

# 生命周期

建议：

```text
Workspace

↓

Load

↓

Detect

↓

Index

↓

Ready

↓

Snapshot

↓

Close
```

不要：

Open。

结束。

---

# SQLite

建议：

第一版：

```text
workspace

project

resource

environment

workspace_snapshot
```

五张：

够。

---

# UX设计

左边：

不要：

Projects。

而是：

```text
Workspace

──────────────

Monolith

OpenCode

RCA

Agent

Website
```

每一个：

Workspace。

---

点击：

Monolith：

右边：

```text
Project

──────────────

Java

SpringBoot

Modules

42
```

下面：

```text
Environment

──────────────

Java 21

Maven

Git

Docker
```

再下面：

```text
Resources

──────────────

Files

3021

Markdown

34

Images

8
```

Agent：

知道：

自己：

在哪里。

---

增加：

Workspace Explorer：

例如：

```text
Workspace

──────────────

README

src

docs

pom.xml

Dockerfile
```

以后：

Desktop：

直接：

共用。

---

再增加：

Snapshot：

例如：

```text
Snapshots

──────────────

Today

10:12

Yesterday

18:00
```

以后：

恢复。

一键。

---

# MVP 不做什么

不要：

* ❌ Git Diff Engine
* ❌ Symbol Index
* ❌ AST
* ❌ Embedding
* ❌ Vector Search
* ❌ Workspace Sync
* ❌ Remote Workspace
* ❌ Cloud Workspace
* ❌ 多 Workspace 调度

以后。

---

# 扩展点（第一版就预留）

```text
Workspace Runtime
│
├── WorkspaceProvider      // Local、SSH、Docker...
├── ResourceProvider       // File、DB、HTTP...
├── EnvironmentDetector    // Java、Node、Git...
├── ProjectScanner         // Maven、Cargo...
├── WorkspaceSnapshot      // 快照
├── WorkspaceIndexer       // 索引
├── WorkspaceObserver      // Metrics、Trace
├── WorkspacePolicy        // 企业策略
└── WorkspaceInterceptor   // Hook
```

---

# 企业版演进路线

| Phase    | 能力                    | 为什么                |
| -------- | --------------------- | ------------------ |
| **P4.0** | Local Workspace       | MVP                |
| **P4.1** | Project Detection     | 自动识别项目             |
| **P4.2** | Environment Detection | Java、Node、Python 等 |
| **P4.3** | Workspace Snapshot    | 快照恢复               |
| **P4.4** | Resource Index        | 文件、目录统一索引          |
| **P4.5** | Symbol Index          | 代码导航               |
| **P4.6** | Remote Workspace      | SSH、容器、云端          |
| **P4.7** | Incremental Index     | 增量索引               |
| **P4.8** | Workspace Policy      | 企业访问控制             |
| **P4.9** | Distributed Workspace | 多节点工作空间            |

---

# 我建议增加一个比 OpenCode、Claude Code 更底层的设计

这是我认为未来所有 Coding Agent 都会需要的一层：

## Workspace Graph（工作空间图）

不要把 Workspace 看成：

```text
Workspace
 ├── File
 ├── File
 ├── File
```

而应该看成一张图：

```text
Workspace
│
├── Project
│      ├── Module
│      │      ├── Package
│      │      │      ├── File
│      │      │      └── Symbol
│      │      └── Dependency
│      └── Build
│
├── Git
│      ├── Branch
│      ├── Commit
│      └── Diff
│
├── Environment
│      ├── Java
│      ├── Maven
│      └── Docker
│
└── Resources
       ├── Markdown
       ├── Image
       └── Database
```

以后：

* Context Runtime 可以直接读取 Workspace Graph，而不是扫描文件。
* Planner 可以按 `Module`、`Dependency`、`Symbol` 规划任务，而不是按路径。
* Tool Runtime 可以直接操作图中的节点，而不是依赖字符串路径。
* 后续加入 AST、LSP、RAG、代码索引，也是在这张图上不断增加节点和关系，而不是推翻原有设计。

**我认为这是一个比"Workspace=目录"更适合作为未来企业级 Agent 平台底座的抽象，也是后续实现大型代码仓、多模块项目、RCA 分析等能力的重要基础。**
