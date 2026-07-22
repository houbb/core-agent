# Core-Agent P5 设计

# P5：Agent Intelligence Evolution Layer（智能进化层）

模块：

```text
core-agent-learning
core-agent-evaluation
core-agent-agent-marketplace
core-agent-agent-network
core-agent-autonomous
```

---

# 一、P5 目标

前面阶段：

```text
P0:
Runtime

        Agent 能运行


P1:
Planning

        Agent 会规划


P2:
Multi-Agent

        Agent 会协作


P3:
Extension

        Agent 会扩展


P4:
Governance

        Agent 企业可控

```

但是：

现在 Agent 仍然是：

```text
人配置能力

↓

Agent 执行

```

P5 目标：

变成：

```text
Agent 使用经验

↓

评估效果

↓

学习优化

↓

发现能力缺口

↓

组合新的 Agent

↓

自主完成更多任务

```

---

# 二、整体架构

```text
                         core-agent


                             |


                Intelligence Evolution Layer


 ----------------------------------------------------------------


 Learning          Evaluation          Marketplace


    |                  |                    |


 Experience          Quality              Agent Assets



 Agent Network                         Autonomous


    |                                      |


 Multi Agent Society              Self Improvement



 ----------------------------------------------------------------


                             |

                      Agent Runtime

```

---

# 三、core-agent-evaluation ⭐⭐⭐⭐⭐

## 定位

Agent 质量评价系统。

这是 Agent 商业化必须能力。

类似：

* LLM Evaluation
* OpenAI Evals
* LangSmith Evaluation

---

# 为什么需要？

普通软件：

```text
Unit Test

↓

判断质量

```

Agent：

输出不固定。

需要：

```text
Input

↓

Agent

↓

Output

↓

Evaluator

↓

Score

```

---

# Evaluation Model

```java
class Evaluation {


id;


agentId;


taskId;


criteria;


score;


feedback;


}

```

---

# Evaluation 类型

## 1. Correctness

结果是否正确。

例如：

RCA:

```text
根因:

DB慢查询

正确:

是

```

---

## 2. Quality

质量。

例如：

代码：

```text
可维护性

规范

性能

```

---

## 3. Safety

安全。

例如：

是否泄露敏感信息。

---

## 4. Cost

成本。

例如：

结果质量：

90分

成本：

$0.2

---

# Evaluation Pipeline

```text
Task


 |

Agent


 |

Result


 |

Evaluator Agent


 |

Score


 |

Feedback

```

---

# Evaluator Agent

特殊 Agent：

```text
Judge Agent
```

例如：

Coder：

生成代码。

Reviewer：

评价代码。

---

# UX

Dashboard：

```text
Agent Quality


Coding Agent


Success:

92%


Average Score:

87


Regression:

-3%


```

---

# 注意点

评价体系必须独立。

不要：

```text
Agent 自己评价自己
```

---

---

# 四、core-agent-learning ⭐⭐⭐⭐⭐

## 定位

Agent 经验学习系统。

不是训练模型。

重点：

> 学习行为和经验。

---

# Memory vs Learning

区别：

|         | Memory | Learning |
| ------- | ------ | -------- |
| 保存事实    | ✅      | ❌        |
| 优化行为    | ❌      | ✅        |
| 用户偏好    | ✅      | 部分       |
| Agent策略 | ❌      | ✅        |

---

# Learning Flow

```text
Execution


 |

Feedback


 |

Analysis


 |

Learning


 |

Improve Policy

```

---

# Learning 类型

## 1. Skill Learning

例如：

发现：

```text
排查 Redis 问题

总是先看:

slowlog

```

形成：

```text
Redis Diagnosis Skill
```

---

## 2. Workflow Learning

优化：

```text
原流程:


5步


优化:


3步

```

---

## 3. Prompt Learning

优化：

```text
System Prompt

```

---

# Learning Record

```java
class LearningRecord {


source;


experience;


improvement;


confidence;


}

```

---

# UX

Agent Evolution：

```text
Agent Learned


新增能力:


Database Slow Query Analysis


来源:

100次 RCA

成功率:

95%

```

---

# 注意点

不要自动修改核心能力。

必须：

```text
Candidate

↓

Review

↓

Apply

```

---

---

# 五、core-agent-agent-marketplace ⭐⭐⭐⭐⭐

## 定位

Agent 能力市场。

类似：

* GPT Store
* VS Marketplace
* npm

---

# Marketplace 内容

不仅 Agent。

包括：

```text
Agent

Skill

Plugin

Workflow

Prompt

MCP

```

---

# Asset Model

```java
AgentAsset {


id;


type;


name;


version;


author;


rating;


downloads;


}

```

---

# 类型

```text
Agent

Skill

Plugin

Workflow

Template

```

---

# 发布流程

```text
Developer


 |

Package


 |

Review


 |

Publish


 |

Install


```

---

# UX

Marketplace：

```text
Agent Store


🔥 Popular


RCA Expert Agent


⭐ 4.9


Install


```

---

# 注意点

企业环境：

需要：

```text
Private Marketplace

```

例如：

公司内部 Agent。

---

---

# 六、core-agent-agent-network ⭐⭐⭐⭐

## 定位

Agent 网络。

让 Agent 发现 Agent。

---

# 为什么？

未来：

不是：

```text
一个超级 Agent

```

而是：

```text
Agent Society

```

---

# Agent Registry

```java
AgentRegistry {


id;


capabilities;


availability;


endpoint;


}

```

---

例如：

注册：

```text
Java Expert Agent

Capability:

Spring Boot

```

---

# Discovery

用户：

```text
解决数据库问题

```

系统：

寻找：

```text
Database Expert Agent

```

---

# Agent Communication

基于 P2：

```text
Message Runtime

```

增强：

```text
Discovery

Routing

Trust

```

---

# Agent Capability 描述

类似：

MCP。

```yaml
agent:

name:

Database-Agent


capabilities:

- mysql

- performance

- sql

```

---

# UX

Agent Network:

```text
Available Agents


Java Agent

● Online


Database Agent

● Online


Security Agent

● Busy

```

---

# 注意点

必须有：

```text
Trust

Permission

Reputation

```

否则风险很高。

---

---

# 七、core-agent-autonomous ⭐⭐⭐⭐⭐

## 定位

自主 Agent。

最终目标。

---

# 从：

```text
User:

执行任务

```

到：

```text
Agent:

发现任务

规划任务

执行任务

优化任务

```

---

# Autonomous Loop

```text
Observe


 |

Understand


 |

Plan


 |

Act


 |

Evaluate


 |

Learn


 |

Repeat

```

---

# Autonomous Trigger

来源：

## 1. Event

例如：

```text
CPU > 90%

```

启动 Agent。

---

## 2. Schedule

例如：

每天：

```text
检查系统健康

```

---

## 3. Goal

例如：

```text
保持系统稳定

```

---

# Goal Model

```java
class Goal {


id;


description;


priority;


constraints;


deadline;


}

```

---

# Autonomous Agent

例如：

NOC：

```text
目标:

保证服务 SLA


Agent:


持续:

监控

分析

修复

优化

```

---

# 安全限制

必须：

```text
Autonomy Level
```

---

等级：

```text
L0

只建议


L1

自动分析


L2

自动执行低风险


L3

自动执行生产任务


L4

完全自主

```

---

# UX

Autonomy Setting:

```text
RCA Agent


Autonomy:


○ Suggest Only


● Auto Diagnose


○ Auto Fix


○ Full Auto


```

---

# 八、P5 模块关系

```text
                 Agent Execution


                       |


                    Result


                       |


              Evaluation


                       |


              Learning


                       |


        ---------------------------


        |                         |


 Marketplace               Agent Network


        |                         |


        ---------------------------


                       |


                 Autonomous Agent


```

---

# 九、P5 数据关系

```text
Agent


 |

Execution


 |

Evaluation


 |

Feedback


 |

Learning


 |

Skill/Workflow Update


 |

New Capability

```

---

# 十、Repo 设计

继续：

```text
core-agent


├── core-agent-learning

├── core-agent-evaluation

├── core-agent-marketplace

├── core-agent-network

├── core-agent-autonomous

```

完整：

```text
core-agent

├── runtime

├── intelligence

├── extension

├── governance

├── evolution

```

---

# 十一、P5 MVP 顺序建议

## Phase 1

先做：

```text
core-agent-evaluation
```

原因：

没有评价，就无法优化。

---

## Phase 2

```text
core-agent-learning
```

建立经验闭环。

---

## Phase 3

```text
core-agent-agent-marketplace
```

形成生态。

---

## Phase 4

```text
core-agent-network
```

多 Agent 社会。

---

## Phase 5

```text
core-agent-autonomous
```

自主运行。

---

# 十二、P5 完成后的能力

整个演进：

```text
P0

Tool Agent


↓

P1

Planning Agent


↓

P2

Multi-Agent


↓

P3

Agent Ecosystem


↓

P4

Enterprise Agent


↓

P5

Self-Evolving Agent

```

最终：

```text
Agent

不仅执行任务

还能:

学习

评价

发现能力

组合能力

自主运行

```

---

P6 下一阶段建议进入：

```text
core-agent-knowledge
core-agent-rag
core-agent-vector
core-agent-document
core-agent-semantic
```

即：

**Agent Knowledge Intelligence Layer（知识智能层）**

这一层会把 Agent 从“会做事”提升到“拥有企业知识大脑”。
