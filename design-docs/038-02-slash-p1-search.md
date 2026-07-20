# Phase 1：Core-Agent Slash Code Intelligence & Tool Governance Runtime

Phase 0.5 建立了：

```
Slash Runtime
      |
      +-- Context Runtime
      +-- Session Runtime
      +-- Checkpoint Runtime
      +-- Compact Engine
```

Phase 1 进入 **Professional Coding Agent 能力层**。

目标：

> 让 Agent 从“聊天 + 修改文件”升级为“理解整个工程 + 安全执行工程操作”。

本阶段新增：

```
/search
/trace
/architecture
/permissions
/approve
```

对应能力：

| 命令              | 能力                                   |
| --------------- | ------------------------------------ |
| `/search`       | Code Intelligence Runtime            |
| `/trace`        | Program Flow Analysis Runtime        |
| `/architecture` | Architecture Understanding Runtime   |
| `/permissions`  | Agent Security Runtime               |
| `/approve`      | Human-in-the-loop Governance Runtime |

---

# 1. 总体架构设计

新增模块：

```
core-agent

├── slash-runtime
│
├── context-runtime
│
├── code-intelligence-runtime ⭐
│
├── execution-runtime ⭐
│
├── permission-runtime ⭐
│
├── approval-runtime ⭐
│
└── audit-runtime
```

整体：

```
                 Terminal
                    |
                 Desktop
                    |
                   API
                    |
                    v

             Slash Runtime


                    |
        +-----------+------------+
        |                        |
        v                        v

 Code Intelligence        Execution Governance


        |                        |

 Search Engine              Permission Engine

 Trace Engine               Approval Engine

 Architecture Engine        Policy Engine

```

---

# 2. Code Intelligence Runtime

这是 Agent Coding 能力核心。

不要简单：

```
grep
```

而应该类似：

* Sourcegraph
* JetBrains PSI
* LSP
* OpenCode index
* Claude Code codebase understanding

设计：

```
Code Intelligence Runtime


        |

     Index Builder


        |

 +------+-------+

 Symbol       Dependency

 Graph        Graph


        |

 Semantic Index

```

---

# Command 1

# `/search`

## 定位

代码搜索入口。

不是替代 grep。

而是：

> 面向 Agent 的语义搜索。

---

# 使用

简单：

```
/search UserService
```

高级：

```
/search "login authentication"
```

文件：

```
/search UserController --type java
```

---

# 输出设计

Terminal：

```
╭─────────────────────╮
 Search Result
╰─────────────────────╯


Query:

UserService


Found:

1. UserService.java

class UserService


Methods:

login()
createUser()


Referenced by:

AuthController

OrderService


2. UserRepository.java

```

---

# Desktop UX

类似 IDE 搜索：

```
--------------------------------

Search

[ UserService       ]

Results


UserService.java

 AuthController.java

 OrderService.java


--------------------------------

Preview

```

---

# 内部接口

```java
interface CodeSearchService {


List<SearchResult> search(
    SearchQuery query
);


}
```

---

# 数据结构

```java
class SearchResult {


String file;


String symbol;


String type;


double relevance;


List<Reference> references;


}
```

---

# 注意点

不要只保存文本索引。

需要：

## Symbol Index

例如：

```
UserService.login()
```

## Reference Graph

```
Controller

   |
   v

Service

   |
   v

Repository

```

否则无法支持 `/trace`。

---

---

# Command 2

# `/trace`

⭐⭐⭐⭐⭐

这是你的差异化能力。

尤其结合你：

* NOC
* RCA
* 链路分析

---

## 定位

代码调用链分析。

类似：

```
request tracing
```

但是：

```
static + runtime
```

结合。

---

# 使用

```
/trace login
```

或者：

```
/trace UserController.login
```

---

# 输出

```
Login Flow


HTTP Request


   |
   v


AuthController


   |
   v


AuthService.login()


   |
   v


UserRepository.find()


   |
   v


Database


```

---

# 高级：

```
/trace login --include-db
```

输出：

```
API

 |
Service

 |
SQL

 |
Table

```

---

# 实现

需要：

## AST Parser

Java：

```
JavaParser
Eclipse JDT
```

Rust：

```
tree-sitter
```

---

## Dependency Graph

保存：

```
Node:

Class

Method


Edge:

call
inherit
inject
```

---

# 数据结构

```rust
struct CallGraph {


nodes:

Vec<Symbol>,


edges:

Vec<CallEdge>


}
```

---

# 注意

不要完全依赖 LLM。

正确：

```
AST

+

Static Analysis

+

LLM Explanation

```

LLM 负责解释。

不是分析。

---

---

# Command 3

# `/architecture`

## 定位

工程架构理解。

---

使用：

```
/architecture
```

或者：

```
/architecture auth-module
```

---

输出：

```
System Architecture


                 API

                  |

             Controller

                  |

             Service Layer

                  |

             Domain

                  |

             Repository

                  |

             Database

```

---

# 进一步：

生成：

```
Architecture.md

```

内容：

```
Modules

Dependencies

Patterns

Risks

Suggestions

```

---

# 实现

依赖：

```
Architecture Analyzer


      |

Package Scanner

      |

Dependency Graph

      |

Pattern Detector

```

---

# Pattern Detector

识别：

```
Spring MVC

DDD

MVC

Hexagonal

Microservice

```

---

# UX

Desktop：

自动生成：

```
Architecture View

[graph]

[modules]

[issues]

```

---

# 注意

不要做：

```
AI 猜架构
```

必须：

```
Evidence First

```

即：

每个结论：

```
Based on:

src/service/*
pom.xml
application.yml

```

---

# 4. Permission Runtime

进入企业级安全。

---

# Command 4

# `/permissions`

## 定位

查看 Agent 权限。

---

输出：

```
Agent Permissions


Filesystem


read      ✓

write     ✓


Shell


execute  approval required


Network


disabled


Git


commit    approval required

```

---

# 数据模型

```java
class AgentPermission {


Resource resource;


Action action;


Policy policy;


}
```

---

# 权限模型

推荐：

RBAC + Capability

类似：

```
User

 |
Role

 |
Agent Capability

 |
Tool Permission

```

---

例如：

```
Coder Agent


allow:

read file

modify file


deny:

delete repository


```

---

# 生命周期

```
Request Tool


      |

Permission Check


      |

Policy Engine


      |

Allow / Deny


```

---

# 5. Approval Runtime

# `/approve`

## 定位

Human-in-the-loop。

---

为什么需要？

Agent 会执行：

```
git commit

delete file

database migration

shell command

```

必须控制。

---

# 使用

当 Agent 要执行：

```
rm config.yml

```

显示：

```
⚠️ Approval Required


Action:

Delete file


Target:

config.yml


Risk:

HIGH


[Approve]

[Deny]

```

---

# Slash:

查看：

```
/approve
```

---

输出：

```
Pending Actions


1

Delete config.yml


2

Execute mvn clean install



```

---

确认：

```
/approve 1
```

拒绝：

```
/deny 1
```

---

# 内部接口

```java
interface ApprovalService {


ApprovalRequest create();


ApprovalResult approve();


ApprovalResult deny();


}
```

---

# 6. 五个命令统一生命周期

统一进入：

```
SlashCommandExecutor


          |

CommandInterceptor


          |

+---------+----------+

Permission

Audit

Metric


          |

Execute

```

---

# 7. 插件设计

Phase 1 开始支持：

```
core-agent-plugin-code


提供:

SearchCommand

TraceCommand

ArchitectureCommand

```

以及：

```
core-agent-plugin-security


提供:

PermissionCommand

ApprovalCommand

```

---

# 8. 与其他 Core 平台连接

这一步开始和你的 Core Platform 接轨。

## core-storage

保存：

```
Code Index

Architecture Snapshot

Checkpoint

```

---

## core-audit

记录：

```
/search

/trace

/approve

```

---

## core-ai

负责：

```
Architecture Explanation

Summary

Recommendation

```

---

## core-config

配置：

```
index path

permission policy

approval rule

```

---

# 9. Phase 1 完成后的能力

完成：

```
core-agent

可以：

✓ 理解项目结构

✓ 搜索代码

✓ 分析调用链

✓ 生成架构视图

✓ 控制工具权限

✓ 请求人工批准


```

能力接近：

```
Claude Code

+
Sourcegraph

+
JetBrains AI

+
Enterprise Governance

```

---

# Phase 1 后建议进入 Phase 2：

## Agent Memory & Knowledge Runtime

新增：

```
/memory-show
/memory-save
/memory-clear
/knowledge
/learn
```

因为：

真正长期 Agent 的核心不是上下文，而是：

> 从一次性 Coding Agent → 长期进化的 Engineering Agent。
