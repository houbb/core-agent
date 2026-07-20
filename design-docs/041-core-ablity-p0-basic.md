# Core-Agent P0 设计

## 目标

P0 阶段不是做一个完整 AI Agent 平台，而是建立：

> **Agent Operating Runtime 基础内核**

类似：

* Claude Code 的 Agent Runtime
* OpenCode 的 Tool Runtime
* Codex 的 Execution Runtime

但是抽象成企业级通用能力。

P0 完成后：

任何业务：

* Coding Agent
* RCA Agent
* 运维 Agent
* Trading Agent
* Research Agent

都可以基于这一层构建。

---

# 一、整体架构

```text
                         core-agent


                              |
                              |

                    core-agent-runtime

                              |

 ----------------------------------------------------------------

 |                 |                 |                 |

core-llm       core-tool       core-context       core-memory


                              |

                         core-permission


                              |

 ----------------------------------------------------------------

          File        Shell       MCP       Database

          Logs        CMDB        API       Knowledge


                              |

                         Agent Instance

```

---

# 二、P0 模块职责

| 模块                 | 职责              | 优先级   |
| ------------------ | --------------- | ----- |
| core-agent-runtime | Agent 生命周期和执行循环 | ⭐⭐⭐⭐⭐ |
| core-llm           | 统一模型调用层         | ⭐⭐⭐⭐⭐ |
| core-tool          | 工具运行时           | ⭐⭐⭐⭐⭐ |
| core-context       | 上下文管理           | ⭐⭐⭐⭐⭐ |
| core-memory        | 记忆系统            | ⭐⭐⭐⭐  |
| core-permission    | 权限控制            | ⭐⭐⭐⭐⭐ |

---

# 1. core-agent-runtime

## 定位

整个 Agent 的大脑控制器。

类似：

Claude Code:

```
Agent Loop
```

OpenCode:

```
Agent Runtime
```

---

# 核心职责

## 1. Agent 生命周期

状态：

```
CREATED

↓

INITIALIZING

↓

RUNNING

↓

WAITING

↓

COMPLETED

↓

FAILED

↓

STOPPED
```

---

## Agent Model

```java
Agent {

 id;

 name;

 description;

 role;

 systemPrompt;

 model;

 tools;

 memory;

 permissionPolicy;

 status;

}
```

示例：

```json
{
"name":"RCA-Agent",

"role":"incident-analysis",

"model":"gpt-5",

"tools":[
"log.query",
"metric.query"
]
}
```

---

# 2. Agent Loop

核心循环：

```text
User Request


     |

     v


Context Builder


     |

     v


LLM


     |

     v


Need Tool?


   /       \

 Yes        No


 |           |

Tool       Response


 |

Result


 |

LLM


 |

Final Answer

```

---

## Runtime 核心接口

```java
interface AgentRuntime {


 AgentSession create();


 AgentResult execute();


 void stop();


}
```

---

# 3. Session Runtime

一次 Agent 执行。

模型：

```java
AgentSession {


 id;


 agentId;


 userId;


 context;


 messages;


 status;


 createdAt;


}
```

例如：

```
RCA-Agent Session #10001

问题:
订单接口超时


上下文:

日志

指标

Trace

```

---

# 4. Streaming Runtime

必须支持：

```
token streaming


tool streaming


status streaming
```

UI：

```
Agent:

正在分析...


调用:

log.query


发现:

500错误
```

---

# P0 UX

Desktop:

```
+----------------------+

 RCA Agent

 状态:
 ● Running


 思考:
 分析订单服务


 当前:
 调用日志工具


+----------------------+

```

---

Terminal:

```
> agent run rca


Agent:

Analyzing...


Tool:

log.query


Result:

...


```

---

# 注意点

## 不要绑定业务

错误：

```
RCAAgent
CodingAgent
```

应该：

```
Generic Agent Runtime
```

---

# 2. core-llm

## 定位

统一 LLM Gateway。

不要让 Agent 直接调用模型。

架构：

```
Agent

 |

core-llm

 |

-----------------

OpenAI

Claude

Gemini

DeepSeek

Local

```

---

# 核心能力

## Model Provider

```java
interface LLMProvider {


chat();


stream();


embedding();


}
```

---

# 配置

```yaml
llm:

 providers:

  openai:

    api-key:


 models:

  default:
    gpt-5

```

---

# Model Router

未来：

```
简单任务

↓

小模型


复杂任务

↓

大模型
```

---

# Token 管理

记录：

```
request token

response token

cost

latency

```

---

# UX

Desktop:

```
AI Settings


Model:

GPT-5


Fallback:

Claude


Temperature:

0.7


Token:

120k

```

---

# 注意点

不要把 Prompt 写死。

需要：

```
Prompt Template Runtime
```

后续扩展。

---

# 3. core-tool

## 定位

Agent 的手。

这是最重要模块之一。

---

# Tool Model

```java
Tool {


name;


description;


inputSchema;


execute();


permission;


}
```

---

# 内置 Tool

P0：

```
file.read

file.write

file.edit

grep

glob

shell.exec

http.request

```

---

# Tool Registry

```text

Tool Registry


 |

-----------------

file

shell

database

mcp

```

---

# Tool 调用流程

```
LLM

 |

Tool Call


 |

Permission Check


 |

Execute


 |

Return Result

```

---

# Tool Result

不要直接返回。

需要：

```
ToolResult

{

success;

data;

error;

metadata;

}
```

---

# UX

Desktop:

Tools 管理：

```
Tools


[x] File


[x] Shell


[x] Web


[x] Database


[ ] Kubernetes


```

---

Terminal:

```
/tools


Available:


file.read

shell.exec

```

---

# 注意点

## Tool 必须无状态

不要：

```
Tool 保存用户状态
```

状态交给：

```
context/memory
```

---

# 4. core-context

## 定位

Agent 的短期工作记忆。

非常关键。

---

# Context 类型

```
Context


├── System Context

├── User Context

├── Task Context

├── File Context

├── Message Context

├── Tool Context

└── Environment Context

```

---

# Context Package

```java
Context {


system;


messages;


references;


tools;


metadata;

}
```

---

# Context Builder

流程：

```
Request


 |

Context Builder


 |

Relevant Context


 |

LLM

```

---

# Context Annotation

支持：

```
@file

@line

@message

@session

@terminal

```

---

# UX

输入框：

```
---------------------------------

帮我分析这个问题


Context:

[UserService.java L20-L50] x

[Error.log] x


---------------------------------

```

---

# 注意点

Context != Memory

不要混。

Context:

```
当前任务
```

Memory:

```
长期知识
```

---

# 5. core-memory

## 定位

长期记忆系统。

---

# Memory 分层

```
Memory


├── Session Memory

├── User Memory

├── Agent Memory

├── Knowledge Memory

```

---

# Memory Item

```java
Memory {


id;


type;


content;


embedding;


metadata;


importance;


}
```

---

# Memory Flow

```
Conversation


 |

Memory Extractor


 |

Store


 |

Recall


 |

Context

```

---

# P0 实现范围

不要做复杂 RAG。

只做：

```
Session Memory

User Preference

Manual Memory

```

---

# UX

设置：

```
Memory


[x] Remember preference


Saved:


"User prefers Java"


```

---

# 注意点

必须：

用户可见

用户可删除

不要黑盒记忆。

---

# 6. core-permission

## 定位

Agent 安全边界。

企业必须。

---

# 权限模型

采用：

RBAC + Policy

---

# Permission Model

```java
Permission {


subject;


resource;


action;


effect;


}
```

---

例如：

Agent:

```
RCA-Agent

```

权限：

```
ALLOW

log.read


ALLOW

metric.query


DENY

production.deploy

```

---

# Tool Permission

调用链：

```
Agent

 |

Tool

 |

Permission


 |

Execute

```

---

# Approval

P0 简单实现：

```
needApproval=true
```

例如：

shell:

```
rm

deploy

database write

```

必须确认。

---

# UX

执行危险操作：

弹窗：

```
Agent wants:


execute:


kubectl delete pod


Reason:


Restart failed service


[Allow]

[Deny]

```

---

# 注意点

权限必须在 Tool 层。

不要只靠 Agent Prompt。

---

# P0 数据模型关系

```
User

 |

Agent

 |

Session

 |

Context

 |

Message


 |

LLM


 |

ToolCall


 |

Permission


 |

ToolResult


 |

Memory

```

---

# P0 Repo 结构建议

```
core-agent


├── core-agent-runtime

│
├── core-llm

│
├── core-tool

│
├── core-context

│
├── core-memory

│
├── core-permission


```

每个：

```
backend

frontend

admin

sdk

```

保持你之前 Core Platform 统一规范。

---

# P0 MVP 交付标准

完成后：

可以运行：

```bash
core-agent chat
```

创建：

```
General Agent
```

支持：

```
用户输入

↓

Agent

↓

调用 Tool

↓

读取文件

↓

修改文件

↓

记忆上下文

↓

权限控制

↓

返回结果

```

---

# P0 最终能力

达到：

```
一个基础 Claude Code + OpenCode Runtime

但是:

更加通用

更加企业化

支持 Desktop/Terminal 一致

支持未来 RCA/NOC/Trading Agent

```

下一阶段 P1 应该设计：

```
core-agent-planner
core-agent-task
core-agent-question
core-agent-todo
core-agent-reflection

```

也就是让 Agent 从“能执行”进化到“会规划”。
