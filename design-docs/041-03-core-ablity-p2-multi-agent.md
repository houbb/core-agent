# Core-Agent P2 设计

## P2：Multi-Agent Runtime

模块：

```text
core-agent-subagent
core-agent-message
core-agent-orchestrator
```

---

# 一、P2 目标

P0：

```text
Agent 能执行
```

P1：

```text
Agent 能规划 + 管理任务 + 自我检查
```

P2：

```text
多个 Agent 协同完成复杂目标
```

也就是：

从：

```text
Single Agent
```

升级为：

```text
Agent System
```

典型场景：

---

## Coding Agent

```text
Main Agent

    |
    |
-----------------

Architect Agent

Coder Agent

Tester Agent

Reviewer Agent

```

---

## RCA Agent

```text
Incident Agent

       |
--------------------

Log Agent

Metric Agent

Trace Agent

CMDB Agent

Knowledge Agent

       |

Root Cause Agent

```

---

## 企业 Agent OS

```text
CEO Agent

 |

--------------------

Finance Agent

HR Agent

Product Agent

Engineering Agent

```

---

# 二、整体架构

```text
                         core-agent-runtime


                                |

                                |

                     Agent Orchestration Layer


       -------------------------------------------------


          SubAgent Runtime


                |


          Message Runtime


                |


          Agent Communication Bus


                |


          Agent Registry


                |


          Agent Pool



       -------------------------------------------------


                              |

                         P0 / P1 Runtime

```

---

# 三、core-agent-subagent

## 定位

Agent 创建、管理、销毁其他 Agent 的能力。

类似：

* Claude Code Sub Agent
* OpenAI Codex Multi Agent
* AutoGen Agent

---

# 1. SubAgent 生命周期

```text
CREATE

 |

INITIALIZE

 |

RUNNING

 |

WAITING

 |

COMPLETED

 |

DESTROY

```

---

# 2. SubAgent Model

```java
class AgentInstance {


    String id;


    String agentType;


    String parentAgent;


    AgentRole role;


    Context context;


    Permission permission;


    Status status;


}
```

---

例如：

主 Agent：

```json
{
"name":"RCA-Agent",
"type":"manager"
}
```

创建：

```json
{
"name":"Log-Agent",
"type":"worker"
}
```

关系：

```text
RCA-Agent

   parent

      |

Log-Agent

```

---

# 3. Agent Role

不要简单叫 Agent。

需要角色。

```text
Planner

Executor

Researcher

Reviewer

Monitor

DecisionMaker

```

---

例如：

代码：

```text
Architect Agent

负责:
设计方案


Coder Agent

负责:
实现


Tester Agent

负责:
验证

```

---

# 4. Agent Spawn

接口：

```java
interface AgentFactory {


Agent create(
 AgentDefinition definition
);


void destroy(
 String agentId
);


}
```

---

调用：

```text
Main Agent:

需要分析日志


spawn:

Log Agent


spawn:

Metric Agent

```

---

# UX

Desktop：

显示 Agent Tree：

```text
Current Task


RCA Agent
|
+-- Log Agent
|
+-- Metric Agent
|
+-- Trace Agent


```

---

运行状态：

```text
Log Agent

● Running

正在查询:

error.log


```

---

# 注意点

## 不要无限创建 Agent

必须：

```text
max agents

max depth

budget

timeout

```

例如：

```yaml
agent:
  max-sub-agent: 5
  max-depth: 3
```

---

# 四、core-agent-message

## 定位

Agent 之间通信系统。

这是 Multi-Agent 的神经系统。

---

# 1. Message Model

```java
class AgentMessage {


String id;


String from;


String to;


MessageType type;


Object payload;


Timestamp timestamp;


}
```

---

# Message 类型

## Request

请求。

```text
Coder Agent:

请 Reviewer 检查代码

```

---

## Response

回复。

```text
Reviewer:

发现两个问题

```

---

## Event

事件。

```text
Task Completed

Tool Failed

Need Help

```

---

## Broadcast

广播。

```text
所有 Agent:

需求发生变化

```

---

# 2. Message Queue

P2 MVP：

SQLite Event Table

未来：

```text
Kafka

RabbitMQ

NATS

Redis Stream

```

---

表：

```sql
agent_message

id

from_agent

to_agent

type

payload

status

created_time

```

---

# 3. Agent Mailbox

每个 Agent：

拥有：

```text
Mailbox

    |
    |
 receive

 send

```

类似：

Actor Model。

---

# 4. Communication Pattern

## Request/Reply

```text
Agent A

 |

Request

 |

Agent B

 |

Response

```

---

## Publish Subscribe

```text
Incident Agent


      |

 Event Bus


      |

----------------


Log Agent


Metric Agent

```

---

# UX

Agent Chat：

类似：

多人聊天室：

```text
RCA Agent:

发现接口异常


Log Agent:

发现 timeout


Metric Agent:

CPU 正常


Trace Agent:

DB耗时增加

```

---

# 注意点

消息必须结构化。

不要：

```json
{
"text":"帮我看看"
}
```

应该：

```json
{
"type":"ANALYSIS_REQUEST",

"goal":

"find root cause"

"context":

{}

}
```

---

# 五、core-agent-orchestrator

## 定位

多 Agent 总调度器。

这是 P2 核心。

---

# 作用：

决定：

* 创建哪些 Agent
* 谁执行
* 谁等待
* 谁合并结果

---

# Orchestration Model

```java
class Orchestration {


id;


goal;


agents;


workflow;


strategy;


status;


}
```

---

# 1. Agent Workflow

例如：

RCA：

```text
Incident Agent


      |

      v


Create Agents


      |

--------------------

Log

Metric

Trace


      |

      v


Aggregate


      |

      v


Root Cause


```

---

# 2. Execution Strategy

支持：

---

## Sequential

串行：

```text
A

↓

B

↓

C

```

---

## Parallel

并行：

```text
      A

      |

Start

      |

---------------

B       C       D

```

---

## Debate

多个 Agent 讨论。

```text
Architect A

提出方案1


Architect B

提出方案2


Judge Agent

选择

```

---

## Supervisor Pattern

推荐 MVP。

结构：

```text

Supervisor Agent


       |

-------------------

Worker1

Worker2

Worker3

```

---

# 3. Result Aggregation

多个结果：

```text
Log Agent

发现:

timeout


Metric Agent

发现:

CPU正常


Trace Agent

发现:

DB慢

```

Orchestrator:

合并：

```text
Root Cause:

Database latency

Confidence:

92%

```

---

# UX

任务视图：

```text
AI Team


Supervisor Agent


Progress:

80%


Workers:


✓ Log Agent

✓ Metric Agent

⏳ Trace Agent


Result:


正在生成最终分析


```

---

# 六、P2 和 P1 的关系

```text

User Goal


   |

Planner


   |

Task


   |

Orchestrator


   |

----------------------------


Agent A        Agent B       Agent C


 |              |             |


Task           Task          Task


 |              |             |


Result        Result        Result


   |

Message Bus


   |

Aggregation


   |

Reflection


```

---

# 七、数据模型关系

```text

AgentDefinition


       |

       |


AgentInstance


       |

       |

AgentSession


       |

       |

Task


       |

       |

Message


```

---

# 八、Repo 设计

保持你的 core 标准：

```text
core-agent


├── core-agent-runtime


├── core-agent-planner


├── core-agent-task


├── core-agent-question


├── core-agent-todo


├── core-agent-reflection


├── core-agent-subagent

├── core-agent-message

├── core-agent-orchestrator

```

每个：

```text
backend

frontend

admin

sdk

```

---

# 九、P2 MVP 实现范围

不要一开始做复杂 Agent Society。

## Phase 0

实现：

```text
Agent Registry

SubAgent Spawn

Message Bus

Supervisor Pattern

Result Aggregation

```

---

## Demo

实现一个 RCA：

用户：

```
订单服务 500
```

Supervisor Agent:

创建：

```
Log Agent

Metric Agent

Trace Agent

```

并行：

```
Log Agent:
发现 SQL timeout


Metric:
DB latency升高


Trace:
SQL占90%

```

Aggregator:

```
Root Cause:

Database slow query

Confidence:

95%

```

---

# 十、P2 完成后的能力

你的 Agent 平台从：

P0:

```text
Tool Calling Agent
```

P1:

```text
Planning Agent
```

升级：

P2:

```text
Multi-Agent System
```

具备：

```text
Agent 创建

Agent 协作

Agent 通讯

Agent 调度

Agent 分工

Agent 汇总
```

这一步完成后，已经接近：

* AutoGen
* CrewAI
* LangGraph
* Devin Multi-Agent
* Claude Code Sub-Agent

但架构更加偏 **企业 Agent OS**。

下一阶段 P3 应该进入：

```text
core-agent-mcp
core-agent-plugin
core-agent-skill
core-agent-slash
```

也就是 **Agent 能力生态层**。
