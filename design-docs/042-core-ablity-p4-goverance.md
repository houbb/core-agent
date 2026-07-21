# Core-Agent P4 设计

# P4：Agent Enterprise Governance Runtime（企业治理层）

模块：

```text
core-agent-workflow
core-agent-approval
core-agent-audit
core-agent-observability
core-agent-cost
```

---

# 一、P4 目标

前面阶段：

```text
P0:
Agent Runtime
    |
    | 能运行


P1:
Agent Intelligence
    |
    | 会规划


P2:
Multi-Agent
    |
    | 会协作


P3:
Extension Ecosystem
    |
    | 会扩展

```

但是进入企业环境，还缺：

* 谁允许 Agent 做什么？
* Agent 做过什么？
* 为什么这么做？
* 花了多少钱？
* 如何控制风险？
* 如何接入企业流程？

所以 P4：

> 把 Agent 从个人助手升级为企业生产系统。

---

# 二、整体架构

```text
                         core-agent


                             |

                  Enterprise Governance Layer


 ----------------------------------------------------------------


 Workflow        Approval        Audit


    |               |              |


 Process          Human          Record


 Engine           Control        Everything



 Observability                   Cost


    |                              |


 Metrics                         Token


 Trace                           Billing


 Logs                            Budget



 ----------------------------------------------------------------


                             |

                      Agent Runtime

```

---

# 三、core-agent-workflow ⭐⭐⭐⭐⭐

## 定位

Agent 工作流引擎。

解决：

> Agent 如何完成长期、复杂、重复流程？

类似：

* Temporal
* Camunda
* n8n
* Airflow

---

# 1. Workflow Model

```java
Workflow {


 id;


 name;


 version;


 nodes[];


 edges[];


 trigger;


 status;


}
```

---

例如：

RCA 自动处理：

```text
报警触发


   |

创建 Incident


   |

启动 RCA Agent


   |

查询日志


   |

查询指标


   |

生成报告


   |

通知负责人


```

---

# 2. Workflow Node

节点类型：

```text
Trigger Node

Agent Node

Tool Node

Human Node

Condition Node

Action Node

```

---

例如：

```text
                 Alert


                   |

                   v


              Agent Node


                   |

          ----------------


          |              |


       Success        Failed


          |              |


       Notify       Escalate

```

---

# 3. Workflow DSL

建议支持：

JSON/YAML。

例如：

```yaml
workflow:

 name: incident-analysis


steps:


 - type: agent

   agent: rca-agent


 - type: approval

   role: manager


 - type: notification

   channel: slack

```

---

# 4. Workflow Runtime

状态：

```text
CREATED

RUNNING

WAITING

PAUSED

COMPLETED

FAILED

```

---

# UX

Desktop：

Workflow Designer：

```text

+-------------------+

 Alert Trigger

        |

        v

 RCA Agent

        |

        v

 Human Approval

        |

        v

 Notification


+-------------------+

```

拖拽式。

---

# 注意点

不要重新造低代码平台。

P4 初期：

重点：

```text
Agent Workflow

```

不是：

```text
Business BPM

```

---

---

# 四、core-agent-approval ⭐⭐⭐⭐⭐

## 定位

人工审批系统。

企业必须。

---

# 为什么需要？

Agent:

```text
我要执行:

kubectl delete pod

```

系统：

```text
需要人工批准

```

---

# Approval Model

```java
Approval {


id;


requester;


action;


resource;


riskLevel;


approvers;


status;


}

```

---

# Approval 状态

```text
PENDING


 |

APPROVED


 |

EXECUTED



或者


REJECTED

```

---

# Approval 类型

## 1. Tool Approval

例如：

shell：

```bash
rm -rf

```

---

## 2. Data Approval

例如：

访问：

```text
生产数据库

```

---

## 3. Workflow Approval

例如：

发布：

```text
测试

↓

生产

```

---

# Risk Engine

自动判断：

```text
LOW

MEDIUM

HIGH

CRITICAL

```

---

例如：

```text
读取日志

LOW


修改代码

MEDIUM


生产发布

HIGH


删除数据库

CRITICAL

```

---

# UX

弹窗：

```
Agent 请求权限


操作:

kubectl delete pod


原因:

恢复异常服务


风险:

HIGH


影响:

production/order-service


[允许]

[拒绝]

```

---

# 注意点

Approval 不属于 Agent。

属于：

```text
Governance Layer

```

否则 Agent 可以绕过。

---

---

# 五、core-agent-audit ⭐⭐⭐⭐⭐

## 定位

Agent 黑盒记录系统。

企业最重要能力之一。

---

# Audit 记录什么？

## Agent

```text
谁创建

什么时候运行

使用哪个模型

```

---

## Tool

```text
调用什么工具

输入什么参数

结果是什么

```

---

## Decision

```text
为什么选择方案 A

```

---

## Permission

```text
谁批准

```

---

# Audit Event

```java
AuditEvent {


id;


actor;


action;


resource;


payload;


timestamp;


}

```

---

# Event 示例

```json
{

"actor":

"RCA-Agent",


"action":

"CALL_TOOL",


"tool":

"log.query",


"time":

"2026-07-20"

}

```

---

# Audit Storage

P4 MVP：

SQLite。

未来：

```text
ClickHouse

Elasticsearch

```

---

# UX

审计页面：

```
Agent Activity


10:01

RCA-Agent


CALL:

log.query


10:02

CALL:

metric.query


10:03

Generate Report


```

---

# 注意点

Audit 必须不可修改。

建议：

append-only。

---

---

# 六、core-agent-observability ⭐⭐⭐⭐⭐

## 定位

Agent 自身监控。

类似：

OpenTelemetry。

---

# 三大信号

## 1. Metrics

指标：

```text
Agent Count

Success Rate

Latency

Token

Tool Failure

```

---

## 2. Logs

例如：

```
Agent started

Planning completed

Tool failed

```

---

## 3. Trace

最重要。

完整链路：

```
User Request


 |

Agent


 |

Planner


 |

LLM


 |

Tool


 |

Result

```

---

# Trace Model

```java
Trace {


traceId;


spanId;


parentId;


operation;


duration;


}

```

---

# 示例

```
trace-001


Agent.execute

  |

  LLM.chat

  |

  Tool.call

      |

      SQL.query

```

---

# UX

Agent Trace Viewer：

```
Request


 |

Planner
  2s


 |

GPT-5
  5s


 |

Tool:
log.query
  1s


 |

Response
```

---

# 注意点

建议直接兼容：

```text
OpenTelemetry

```

不要自定义。

---

---

# 七、core-agent-cost ⭐⭐⭐⭐⭐

## 定位

Agent 成本控制。

企业必备。

---

# 为什么？

一个 Agent：

可能：

```
调用 GPT-5

10000次

```

成本不可控。

---

# Cost Model

```java
CostRecord {


agent;


model;


inputTokens;


outputTokens;


price;


time;


}

```

---

# 统计维度

## Agent

```
RCA-Agent

$50

```

---

## User

```
张三

$20

```

---

## Project

```
Trading Platform

$200

```

---

# Budget

例如：

```yaml
budget:

 user:

   monthly:100


 agent:

   monthly:500

```

---

# Cost Control

策略：

```text
超过预算

↓

降低模型

↓

限制调用

↓

停止

```

---

# UX

Dashboard：

```
AI Usage


Today:

$12.5


Top Agent:


RCA-Agent


Tokens:

2.3M


```

---

# 八、P4 五模块关系

```text

                    Agent


                      |

                      |

                 Workflow


                      |

              ----------------


              |              |


        Approval          Audit


              |              |


              ----------------


                      |


              Observability


                      |


                    Cost


```

---

# 九、P4 数据关系

```text

Agent

 |

Session

 |

Workflow Instance

 |

Task

 |

ToolCall

 |

AuditEvent

 |

Trace

 |

CostRecord

```

---

# 十、Repo 设计

继续你的统一规范：

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


├── core-agent-mcp

├── core-agent-plugin

├── core-agent-skill

├── core-agent-slash


├── core-agent-workflow

├── core-agent-approval

├── core-agent-audit

├── core-agent-observability

├── core-agent-cost

```

---

# 十一、P4 MVP 建议

不要一次全部企业化。

## Phase 1

优先：

```
core-agent-audit

core-agent-observability

core-agent-approval

```

原因：

企业上线必须。

---

## Phase 2

增加：

```
core-agent-workflow

```

---

## Phase 3

增加：

```
core-agent-cost

```

---

# 十二、P4 完成后的能力

整个 Agent 平台演进：

```
P0
Tool Calling Agent


      ↓


P1
Planning Agent


      ↓


P2
Multi-Agent System


      ↓


P3
Agent Ecosystem


      ↓


P4
Enterprise Agent Platform

```

达到：

```text
安全
可控
可审计
可运营
可规模化
```

---

P5 下一阶段建议：

```text
core-agent-learning
core-agent-evaluation
core-agent-agent-marketplace
core-agent-agent-network
core-agent-autonomous
```

进入：

**Agent Intelligence Evolution Layer（自主进化层）**。
