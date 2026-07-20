# Phase 3：Core-Agent Agent Society Runtime

## 目标

Phase 0～2 完成：

```text
Phase 0.5
Slash Runtime Foundation

        |
        v

Phase 1
Code Intelligence + Governance

        |
        v

Phase 2
Memory + Knowledge Runtime
```

此时：

> 一个 Agent 已经具备“理解、执行、记忆”的能力。

但是现实复杂任务：

* 大型系统重构
* RCA 根因分析
* 安全审计
* 架构评审
* 产品设计
* 自动化运维

单 Agent 会出现：

* 上下文过载
* 角色冲突
* 推理质量下降
* 缺少专业分工

因此 Phase 3 引入：

> Agent Society（Agent 社会系统）

目标：

从：

```text
User
 |
Agent
 |
Tool
```

升级：

```text
User

 |
 |
Agent Society

 |
 +-------------+
 |             |
Planner     Researcher

Coder       Reviewer

Tester      Security

```

---

# 1. 总体架构设计

新增模块：

```text
core-agent

├── slash-runtime
│
├── context-runtime
│
├── code-intelligence-runtime
│
├── memory-runtime
│
├── knowledge-runtime
│
├── agent-runtime
│
├── agent-registry          ⭐
│
├── agent-orchestrator      ⭐
│
├── agent-message-bus       ⭐
│
├── agent-role-system       ⭐
│
└── collaboration-runtime   ⭐

```

---

# 2. Agent Society 架构

```text
                 User


                  |
                  v


           Supervisor Agent


                  |
        +---------+---------+

        |         |         |

    Planner    Coder    Reviewer


        |         |         |

     Tools     Tools     Tools



                  |

             Shared Memory

                  |

             Knowledge Base

```

---

# 3. 核心设计原则

## Agent 不是 Chat Bot

Agent 是：

```text
Agent

=

Identity

+

Role

+

Goal

+

Memory

+

Tools

+

Policy

+

Lifecycle

```

---

# 4. Agent 数据模型

```rust
struct Agent {


id:String,


name:String,


role:String,


description:String,


model:String,


system_prompt:String,


tools:Vec<Tool>,


memory_scope:MemoryScope,


policy:AgentPolicy,


status:AgentStatus


}

```

---

# Agent Role

```rust
enum AgentRole {


Planner,

Developer,

Reviewer,

Tester,

Security,

Researcher,

Operator


}

```

---

# Agent 生命周期

类似 Kubernetes Pod：

```text
Created

   |

Initialized

   |

Running

   |

Waiting

   |

Completed

   |

Archived

```

---

# 5. Command 1

# `/agents`

## 定位

Agent Registry 查看入口。

---

使用：

```text
/agents
```

输出：

```text
╭────────────────────────╮
│ Available Agents       │
╰────────────────────────╯


System Agents


planner

Status:

ready


coder

Status:

running


reviewer

Status:

ready



Custom Agents:


java-expert

security-auditor

```

---

Desktop UX：

新增：

Agent Panel

```text
--------------------------------

Agents


🧠 Planner

🛠 Coder

🔍 Reviewer


Status


Running


--------------------------------

```

---

# 6. Agent Registry

类似：

```text
Plugin Registry

+
Service Registry
```

接口：

```java
interface AgentRegistry {


register(Agent agent);


remove(String id);


find(String id);


list();


}

```

---

# Agent Plugin

未来：

```text
core-agent-plugin-java


提供:

Java Expert Agent



core-agent-plugin-k8s


提供:

Kubernetes Operator Agent

```

---

# 7. Command 2

# `/delegate`

⭐⭐⭐⭐⭐

这是 Phase 3 核心命令。

## 定位

任务委派。

---

使用：

简单：

```text
/delegate security review
```

含义：

创建：

```text
Security Agent

执行

Review

```

---

复杂：

```text
/delegate

Analyze production incident

```

自动拆：

```text
Supervisor


 |
 +-- RCA Agent

 +-- Log Agent

 +-- Metric Agent

 +-- Code Agent

```

---

# Delegation Pipeline

```text
Task


 |

Task Analyzer


 |

Capability Matching


 |

Agent Selection


 |

Execution


 |

Result Merge

```

---

# Agent Selection

不是随机。

评分：

```text
Agent Score


=

Capability

+

Experience

+

Cost

+

Availability

```

---

# 数据模型

```rust
struct Delegation {


task:String,


from_agent:String,


target_agent:String,


priority:u8,


status:Status


}

```

---

# 8. Command 3

# `/team`

## 定位

创建 Agent Team。

---

使用：

```text
/team start
```

---

创建：

```text
Software Refactor Team


Members:


Planner

Coder

Reviewer

Tester


```

---

查看：

```text
/team status
```

输出：

```text
Team:


Refactor-Team


Planner

completed


Coder

running


Reviewer

waiting


```

---

# Team Runtime

```text
Team


 |

Member Agents


 |

Shared Objective


 |

Communication Channel


 |

Result Aggregation

```

---

# 9. Agent Communication Runtime

Agent 之间不能靠：

```text
直接调用
```

需要消息协议。

---

设计：

```json
{
"from":"planner",

"to":"coder",

"type":"task",

"payload":

"Implement login API"

}
```

---

# Message Type

```text
TASK

RESULT

QUESTION

FEEDBACK

APPROVAL

ERROR

```

---

# 10. Command 4

# `/roles`

查看角色。

使用：

```text
/roles
```

输出：

```text
Available Roles


Planner

负责:

Task decomposition


Coder

负责:

Implementation


Reviewer

负责:

Quality


Security

负责:

Risk

```

---

# Role 与 Agent 区别

非常重要：

Role:

```text
能力模板
```

Agent:

```text
具体实例
```

类似：

```text
Class

  |

Object

```

---

# 11. Command 5

# `/collaborate`

查看协作过程。

---

使用：

```text
/collaborate
```

输出：

```text
Current Collaboration


Planner:

Created plan


Coder:

Writing code


Reviewer:

Analyzing diff


Tester:

Preparing cases


```

---

# 12. Supervisor Agent

Phase 3 最重要组件。

类似：

* AutoGen
* CrewAI
* LangGraph

但是核心自己实现。

---

职责：

## Task Decomposition

```text
需求

↓

任务树

↓

Agent 分配

```

---

## Conflict Resolution

例如：

Coder:

> 快速实现

Security:

> 不允许

Supervisor:

> 修改方案

---

## Result Aggregation

多个结果：

```text
Coder Result

+

Reviewer Result

+

Tester Result


        |

        v


Final Answer

```

---

# 13. Agent Workflow

一个典型流程：

```text
User:

重构认证模块


        |

Supervisor


        |

Planner


        |

Task Graph


        |

+-------+-------+

Coder          Security


 |               |


Reviewer


 |


Tester


 |


Final Merge

```

---

# 14. 与前面 Runtime 集成

## Context Runtime

每个 Agent：

独立 Context。

```text
Planner Context

Coder Context

Reviewer Context

```

---

## Memory Runtime

共享：

```text
Project Memory


但是：

Agent Private Memory

```

---

## Knowledge Runtime

所有 Agent：

共享知识。

---

## Permission Runtime

关键：

不同 Agent 不同权限。

例如：

```text
Coder Agent

write code


Security Agent

read only


Deploy Agent

execute


```

---

# 15. 安全设计

Agent Permission Matrix：

| Agent    | Read | Write | Execute |
| -------- | ---- | ----- | ------- |
| Planner  | ✅    | ❌     | ❌       |
| Coder    | ✅    | ✅     | ⚠️      |
| Reviewer | ✅    | ❌     | ❌       |
| Operator | ✅    | ⚠️    | ✅       |

---

# 16. Slash Plugin

新增：

```text
core-agent-plugin-society


提供:

/agents

/delegate

/team

/roles

/collaborate

```

---

# 17. 与 Core 平台连接

## core-storage

保存：

```text
Agent Profile

Team Definition

Conversation

```

---

## core-ai

负责：

```text
Model Routing

Agent Reasoning

Prompt Template

```

---

## core-workflow

未来：

```text
Agent Workflow

Approval Flow

Automation

```

---

## core-audit

记录：

```text
Agent Created

Task Delegated

Decision Made

```

---

# 18. Phase 3 完成能力

完成后：

```text
core-agent


具备：

✓ Agent Registry

✓ Agent Lifecycle

✓ Multi-Agent

✓ Task Delegation

✓ Agent Team

✓ Collaboration Protocol

✓ Supervisor Agent


```

能力达到：

```text
Claude Code Agent Teams

+

AutoGen

+

CrewAI

+

Enterprise Agent Platform

```

---

# Phase 3 关键注意点

## 1. 不要过早创建大量 Agent

错误：

```text
100 个 Agent
```

正确：

MVP：

```text
Planner

Coder

Reviewer

Tester

```

---

## 2. Agent 通信协议比 Agent 数量重要

未来生态核心：

不是：

> 有多少 Agent

而是：

> Agent 如何协作。

---

## 3. Supervisor 是未来 Agent OS Kernel

最终：

```text
User

 |

Supervisor Agent

 |

Agent Society

 |

Tools / Systems

```

这一步完成后，core-agent 已经从：

> AI 编程助手

升级为：

> Agent Operating System 的雏形。

下一阶段 Phase 4 建议进入：

# Agent Cognitive Runtime（智能认知层）

新增：

```text
/reason
/think
/reflect
/critic
/hypothesis
/decision
```

把 Agent 从“执行任务”提升到“具备认知闭环”。
