结合前面分析的 **OpenCode / Claude Code / Codex**，如果我们重新设计一个企业级 `core-agent`，核心能力不能只看 Coding Agent，而应该抽象成一个 **通用 Agent Operating Runtime**。

我建议按照建设优先级排序：

---

# Core-Agent 能力总览（Roadmap）

```text
                         core-agent

                             |
        ------------------------------------------------

        P0 Foundation Runtime（必须）
        
        Agent Runtime
        LLM Runtime
        Tool Runtime
        Context Runtime
        Memory Runtime
        Permission Runtime


        P1 Intelligence Runtime（核心增强）

        Planner
        Task
        Todo
        Question
        Reflection
        Evaluation


        P2 Multi-Agent Runtime

        Agent
        SubAgent
        Agent Communication
        Agent Orchestration


        P3 Extension Runtime

        MCP
        Plugin
        Skill
        Slash Command


        P4 Enterprise Runtime

        Workflow
        Approval
        Audit
        Cost
        Observability


        P5 Advanced

        Learning
        Agent Marketplace
        Agent Network

        ------------------------------------------------
```

---

# P0：Agent Runtime ⭐⭐⭐⭐⭐

这是整个系统的核心。

## 1. Agent

核心对象：

```java
Agent {

 id;

 name;

 role;

 systemPrompt;

 tools[];

 memory[];

 model;

 policy;

}
```

能力：

* 创建 Agent
* 生命周期管理
* 状态管理
* 执行任务

例如：

```text
Developer Agent

RCA Agent

Research Agent

Trading Agent
```

---

## 2. LLM Runtime ⭐⭐⭐⭐⭐

负责模型抽象。

支持：

```text
OpenAI

Claude

Gemini

DeepSeek

Qwen

Local Model
```

统一：

```java
LLMProvider {

 chat()

 stream()

 embedding()

}
```

能力：

* 模型路由
* Token 管理
* Cost 控制
* Fallback

---

## 3. Tool Runtime ⭐⭐⭐⭐⭐

Agent 的手脚。

统一：

```java
Tool {

 name;

 description;

 inputSchema;

 execute();

 permission();

}
```

内置：

```text
file.read

file.write

shell.exec

http.request

database.query

browser.search

git.operation

docker.operation

k8s.operation
```

这是 OpenCode / Claude Code 最核心部分。

---

## 4. Context Runtime ⭐⭐⭐⭐⭐

上下文管理。

解决：

> Agent 如何知道当前任务相关信息？

包括：

```text
System Context

User Context

Task Context

Project Context

Conversation Context

Environment Context
```

例如：

RCA Agent:

```text
当前故障:

订单服务

错误:

timeout

相关:

日志

trace

metric

CMDB
```

---

## 5. Memory Runtime ⭐⭐⭐⭐⭐

长期记忆。

分层：

```
Memory

├── Short Memory
│
├── Session Memory
│
├── User Memory
│
├── Agent Memory
│
└── Knowledge Memory
```

类似：

Claude memory

ChatGPT memory

---

## 6. Permission Runtime ⭐⭐⭐⭐⭐

企业必须。

控制：

```
Agent

 |
 Permission

 |
 Tool
```

例如：

开发 Agent:

允许：

```
read code
write code
run test
```

禁止：

```
delete database
production deploy
```

包括：

* RBAC
* ABAC
* Policy Engine
* Approval

---

# P1：Agent Intelligence ⭐⭐⭐⭐⭐

## 7. Planner Runtime

复杂任务拆解。

例如：

输入：

```
实现支付系统
```

Planner:

```
Task1:
设计数据库

Task2:
实现 API

Task3:
测试

Task4:
部署
```

对应 Claude Code Plan Mode。

---

## 8. Task Runtime ⭐⭐⭐⭐⭐

任务生命周期。

状态：

```
CREATED

RUNNING

WAITING

SUCCESS

FAILED

CANCELLED
```

---

## 9. Todo Runtime

简单任务管理。

例如：

```
[x] 分析代码

[x] 修改接口

[ ] 编写测试

[ ] 发布
```

---

## 10. Question Runtime

Human-in-loop。

Agent 遇到：

```
方案A

方案B

无法判断
```

请求用户。

例如：

```
是否允许修改生产数据库？
```

---

## 11. Reflection Runtime

Agent 自我检查。

流程：

```
Plan

↓

Execute

↓

Review

↓

Improve
```

类似：

Self Reflection。

---

# P2：Multi-Agent Runtime ⭐⭐⭐⭐⭐

这是未来重点。

---

## 12. SubAgent Runtime

一个 Agent 创建其他 Agent。

例如：

```
Main Agent


    |
 ------------------

 |        |        |

Coder   Tester   Reviewer

```

---

## 13. Agent Communication ⭐⭐⭐⭐⭐

Agent 间通信。

需要：

Message:

```json
{
 from:"coder",
 to:"tester",
 content:"代码完成"
}
```

能力：

* send
* receive
* broadcast
* mailbox

---

## 14. Agent Orchestrator

管理多个 Agent。

例如：

```
Incident Agent

        |

-----------------

Log Agent

Metric Agent

Trace Agent

Knowledge Agent

        |

Root Cause Agent
```

---

# P3：Extension Runtime ⭐⭐⭐⭐

## 15. MCP Runtime ⭐⭐⭐⭐⭐

外部能力连接。

类似 Claude MCP。

例如：

```
Agent

 |

MCP

 |

----------------

GitHub

Jira

Slack

CMDB

Grafana

Database
```

---

## 16. Plugin Runtime ⭐⭐⭐⭐⭐

插件体系。

例如：

```
core-plugin

     |

----------------

RCA Plugin

Trading Plugin

Code Plugin

```

---

## 17. Skill Runtime

技能封装。

区别：

Tool：

```
执行能力
```

Skill：

```
解决方案
```

例如：

Tool:

```
query_log()
```

Skill:

```
RCA故障分析流程
```

---

## 18. Slash Command Runtime

类似 Claude Code：

```
/review

/test

/explain

/deploy
```

用户快捷入口。

---

# P4：Enterprise Runtime ⭐⭐⭐⭐⭐

## 19. Workflow Runtime

Agent 工作流。

例如：

```
收到报警

↓

分析

↓

定位

↓

生成报告

↓

通知人员

```

---

## 20. Approval Runtime

审批。

例如：

Agent:

```
我要执行生产发布
```

Human:

```
Approve
```

---

## 21. Audit Runtime

审计。

记录：

```
Agent做了什么

调用什么工具

修改什么文件

花费多少token
```

---

## 22. Observability Runtime

Agent 自身监控。

包括：

Metrics:

```
token

latency

success rate
```

Trace:

```
Agent

 |
 Tool

 |
 LLM

 |
 Result
```

---

## 23. Cost Runtime

成本。

统计：

```
Agent:

调用 GPT-5

50000 tokens

$0.5
```

---

# P5：高级能力

## 24. Agent Learning

学习用户习惯。

例如：

用户喜欢：

```
Java

Spring Boot

SQLite
```

Agent 自动适应。

---

## 25. Agent Marketplace

Agent 商店。

类似：

```
GPT Store

Claude Skills
```

---

## 26. Agent Network

Agent 之间发现。

未来：

```
你的 RCA Agent

        |

寻找

        |

别人部署的 Database Agent

```

---

# 最终 Core-Agent 架构

```text
                         CORE AGENT OS


                             Agent Runtime

                                  |

        ------------------------------------------------

        Intelligence Layer

        Planner
        Task
        Todo
        Question
        Reflection


        Execution Layer

        Tool Runtime
        MCP Runtime
        Plugin Runtime
        Skill Runtime


        Memory Layer

        Context
        Short Memory
        Long Memory
        Knowledge


        Collaboration Layer

        Agent
        SubAgent
        Communication
        Orchestrator


        Governance Layer

        Permission
        Approval
        Audit
        Cost
        Observability


        Model Layer

        LLM Gateway
        Model Router
        Embedding


        ------------------------------------------------
```

---

如果按照你之前 **core 平台路线**，我认为优先级应该调整为：

```
P0:
core-agent-runtime
core-llm
core-tool
core-context
core-memory
core-permission


P1:
core-task
core-planner
core-question
core-todo


P2:
core-subagent
core-message
core-orchestrator


P3:
core-mcp
core-plugin
core-skill


P4:
core-workflow
core-approval
core-audit
core-observability


P5:
core-learning
core-marketplace
core-agent-network
```

这个顺序基本就是把 **OpenCode + Claude Code + Codex + 企业 Agent 平台** 融合后的最小 Agent Operating System 路线。你之前规划的 `core-ai + core-workflow + core-plugin + core-openapi` 可以自然演化成这一套。
