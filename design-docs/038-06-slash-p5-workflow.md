# Phase 5：Core-Agent Workflow Runtime（Agent 工作流与自动化层）

## 目标

前面 Phase 0～4：

```text
Phase 0.5
Slash Runtime Foundation

        ↓

Phase 1
Code Intelligence + Tool Governance

        ↓

Phase 2
Memory + Knowledge Runtime

        ↓

Phase 3
Agent Society Runtime

        ↓

Phase 4
Cognitive Runtime
```

此时：

Core-Agent 已经具备：

* 理解任务
* 分析问题
* 调度 Agent
* 保存经验
* 做出决策

但是仍然存在一个问题：

> Agent 主要还是被用户主动调用。

例如：

```text
用户：

帮我分析昨天线上故障


Agent:

执行一次


结束
```

企业场景需要：

* 定时执行
* 事件触发
* 长期运行
* 自动恢复
* 自动巡检
* 自动响应

所以 Phase 5 引入：

# Agent Workflow Runtime

目标：

> 让 Agent 从 Interactive Agent 变成 Autonomous Agent。

---

# 新增 Slash 命令

```text
/workflow
/trigger
/schedule
/run
/observe
/retry
```

对应能力：

| 命令          | 能力    |
| ----------- | ----- |
| `/workflow` | 工作流管理 |
| `/trigger`  | 事件触发  |
| `/schedule` | 定时任务  |
| `/run`      | 手动执行  |
| `/observe`  | 运行观察  |
| `/retry`    | 失败恢复  |

---

# 1. 总体架构设计

新增：

```text
core-agent

├── slash-runtime
│
├── context-runtime
│
├── memory-runtime
│
├── knowledge-runtime
│
├── society-runtime
│
├── cognitive-runtime
│
├── workflow-runtime          ⭐
│
├── trigger-engine            ⭐
│
├── scheduler-engine          ⭐
│
├── execution-engine          ⭐
│
├── state-machine-engine      ⭐
│
└── observability-runtime     ⭐

```

---

# 2. Workflow Runtime 总体架构

```text
                 Event


                  |

                  v


            Trigger Engine


                  |

                  v


           Workflow Engine


                  |

        +---------+---------+

        |         |         |

     Agent     Tool     Human


        |         |         |


        +---------+---------+


                  |

              Result


                  |

             Memory / Audit

```

---

# 3. 核心设计理念

## Workflow 不是脚本

传统：

```text
if xxx:

 call api

else:

 do something
```

Agent Workflow：

```text
Goal

+

Reasoning

+

Agent

+

Tool

+

Human Approval

```

---

# 4. Workflow 数据模型

```rust
struct Workflow {


id:String,


name:String,


description:String,


version:String,


trigger:Trigger,


nodes:Vec<Node>,


state_machine:StateMachine,


policy:Policy


}
```

---

# Workflow Node

```rust
struct WorkflowNode {


id:String,


type:NodeType,


action:String,


agent:String,


next:Vec<String>


}
```

---

# Node 类型

```text
Agent

Tool

Condition

Approval

Wait

Loop

Parallel

```

---

# 5. Command 1

# `/workflow`

## 定位

Workflow 管理入口。

---

使用：

```text
/workflow
```

输出：

```text
╭────────────────────────╮
│ Workflows              │
╰────────────────────────╯


Active:


Production RCA

status:
running


Daily Code Review

status:
scheduled


Security Scan

status:
paused

```

---

查看详情：

```text
/workflow show rca-flow
```

输出：

```text
Production RCA


Trigger:

Alert Event


Steps:


1.

Collect Metrics


2.

Analyze Logs


3.

RCA Agent


4.

Generate Report


5.

Notify Team

```

---

# Desktop UX

新增：

Workflow Canvas

类似：

* n8n
* Temporal UI
* LangGraph Studio

```text
+---------+
| Trigger |
+---------+

      |

+---------+
| Agent   |
+---------+

      |

+---------+
| Action  |
+---------+

```

---

# 6. Workflow Plugin Interface

```java
interface WorkflowPlugin {


WorkflowDefinition load();


WorkflowResult execute();


}
```

---

# 7. Command 2

# `/trigger`

## 定位

事件触发管理。

---

使用：

```text
/trigger
```

输出：

```text
Available Triggers


Git Push

HTTP Webhook

File Change

Alert


Schedule

Manual

```

---

创建：

```text
/trigger create alert-rca
```

---

配置：

```yaml
trigger:

 type:
   alert


condition:

 severity >= critical


workflow:

 production-rca

```

---

# Trigger Engine

```text
Event

 |

Parser

 |

Filter

 |

Match Workflow

 |

Execute

```

---

# 支持事件

第一阶段：

```text
HTTP

Webhook

File

Timer

Manual

```

后续：

```text
Kafka

MQ

Cloud Event

Prometheus Alert

```

---

# 8. Command 3

# `/schedule`

## 定时 Agent。

---

使用：

```text
/schedule
```

输出：

```text
Schedules


Daily Report

09:00


Security Scan

02:00


Dependency Check

Sunday

```

---

创建：

```text
/schedule create


daily-code-review


cron:

0 9 * * *

```

---

内部：

不要自己实现 Cron。

参考：

* Quartz
* Temporal Scheduler
* Kubernetes CronJob

设计抽象：

```java
interface Scheduler {


register(Task task);


cancel(id);


}
```

---

# 9. Command 4

# `/run`

## 手动运行 Workflow

---

使用：

```text
/run production-rca
```

输出：

```text
Starting Workflow


Production RCA


Execution ID:

run-20260720-001


Current Step:


Analyze Logs


```

---

# Workflow Execution Model

类似：

Temporal：

```text
Workflow Instance


id


state


history


checkpoint


```

---

# 状态机

```text
Created

 |

Running

 |

Waiting Approval

 |

Completed


Failed


Cancelled

```

---

# 10. Command 5

# `/observe`

⭐⭐⭐⭐⭐

## 定位

Workflow 可观测性。

---

使用：

```text
/observe production-rca
```

输出：

```text
Execution:


Step 1

Collect Metrics

✓


Step 2

Analyze Logs

✓


Step 3

RCA Agent

running


Tokens:

20k


Cost:

$0.3

```

---

# Observability Runtime

记录：

```text
Workflow Event


Agent Event


Tool Event


Decision Event

```

最终连接：

```text
core-audit

+
core-ai analytics

```

---

# 11. Command 6

# `/retry`

## 失败恢复。

---

使用：

```text
/retry run-001
```

---

策略：

```text
Retry


|

Analyze Failure


|

Resume From Checkpoint


|

Continue

```

---

不能简单：

```text
重新跑全部
```

必须结合：

Phase 0.5:

```text
Checkpoint Runtime

```

---

# 12. Workflow 与 Agent Society

核心关系：

```text
Workflow

负责：

什么时候做


        +

Agent Society

负责：

谁来做


        +

Cognitive Runtime

负责：

怎么决定


```

例如：

生产故障：

```text
Trigger:

Alert


Workflow:


Collect Data


        |

RCA Agent


        |

Coder Agent


        |

Reviewer Agent


        |

Human Approval


        |

Fix

```

---

# 13. Human-Agent Collaboration

企业必须支持：

```text
Agent


 |

Need Approval


 |

Human


 |

Continue

```

节点：

```text
Approval Node

```

例如：

生产发布：

```text
AI:

建议发布


Human:

Approve


AI:

Deploy

```

---

# 14. 与 Core 平台连接

## core-workflow

实际上 Phase 5 会成为：

```text
core-agent workflow runtime

↓

core-workflow enterprise runtime

```

---

## core-notification

通知：

```text
Workflow Failed

Approval Required

Task Completed

```

---

## core-audit

记录：

```text
Workflow Start

Agent Decision

Tool Execution

```

---

## core-billing

统计：

```text
Workflow Cost

Agent Token Usage

```

---

# 15. 插件设计

新增：

```text
core-agent-plugin-workflow


提供:


/workflow

/trigger

/schedule

/run

/observe

/retry


```

---

# 16. Phase 5 完成能力

完成后：

```text
core-agent


拥有：


✓ Workflow Engine

✓ Event Trigger

✓ Scheduler

✓ Long Running Task

✓ Human Approval

✓ Failure Recovery

✓ Observability


```

能力：

```text
Claude Code

+

Devin

+

Temporal

+

n8n

+

LangGraph

```

---

# Phase 5 关键注意点

## 1. Workflow Engine 不要绑定 Agent

错误：

```
Workflow Node = Prompt
```

正确：

```
Workflow Node

    |
    +-- Agent

    |
    +-- Tool

    |
    +-- Human

```

未来才能扩展。

---

## 2. 必须 Event First

不要：

```text
定时任务中心
```

应该：

```text
Event Driven Automation
```

因为未来：

* 监控告警
* Git事件
* 用户事件
* 数据事件

都会进入。

---

## 3. 必须有 Execution History

类似：

Temporal：

```text
Workflow History

+
Event Sourcing

```

否则无法：

* 回放
* 调试
* 审计

---

完成 Phase 5 后，core-agent 的演进路径：

```text
                 Agent OS


                     |

              Workflow Runtime


                     |

              Agent Society


                     |

             Cognitive Runtime


                     |

          Memory / Knowledge


                     |

           Tools / Code Runtime

```

下一阶段建议：

# Phase 6：Agent Observability & Evaluation Runtime（Agent 可观测与评估层）

新增：

```text
/trace-agent
/evaluate
/benchmark
/debug
/replay
/score
```

解决企业真正落地最大的痛点：

> 如何知道 Agent 为什么这样做，以及如何持续提升 Agent 质量。
