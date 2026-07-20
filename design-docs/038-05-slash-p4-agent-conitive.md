# Phase 4：Core-Agent Cognitive Runtime（Agent 智能认知层）

## 目标

Phase 0～3：

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
```

已经具备：

* 能理解代码
* 能调用工具
* 能保存知识
* 能调度多个 Agent

但是目前 Agent 仍然偏：

> “任务执行器”

而不是：

> “具备分析、反思、决策能力的智能体”

Phase 4 引入：

# Agent Cognitive Runtime

目标：

让 Agent 具备：

* 思考过程管理
* 自我检查
* 假设推理
* 决策记录
* 经验沉淀
* 失败学习

新增 Slash：

```text
/reason
/think
/reflect
/critic
/hypothesis
/decision
```

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
├── agent-runtime
│
├── society-runtime
│
├── cognitive-runtime          ⭐
│
│
├── reasoning-engine            ⭐
├── reflection-engine           ⭐
├── decision-engine             ⭐
├── critique-engine             ⭐
└── learning-engine             ⭐

```

---

# 2. Cognitive Runtime 总体架构

```text
                    User Goal


                       |

                       v


              Cognitive Runtime


                       |

        +--------------+---------------+

        |              |               |

   Reasoning       Critique       Reflection


        |              |               |

        +--------------+---------------+

                       |

                 Decision Engine


                       |

                 Action Plan


                       |

                 Agent Society

```

---

# 3. 核心设计理念

## 不暴露 Chain of Thought

非常重要。

不要设计：

```text
/think

显示完整思考过程
```

原因：

* 安全问题
* 不稳定
* 无必要

正确设计：

输出：

* reasoning summary
* assumptions
* evidence
* decisions

类似：

```text
Reasoning Trace
```

而不是：

```text
Private Thought
```

---

# 4. Cognitive Object 模型

核心对象：

```rust
struct CognitiveProcess {


goal:String,


context:Context,


hypothesis:Vec<Hypothesis>,


evidence:Vec<Evidence>,


decision:Decision,


confidence:f32,


created_at:Timestamp

}
```

---

# 5. Command 1

# `/reason`

## 定位

问题分析 Runtime。

不是执行。

用于：

> 为什么？

---

## 使用

```text
/reason
```

或者：

```text
/reason Why API latency increased?
```

---

输出：

```text
╭────────────────────╮
 Reasoning Summary
╰────────────────────╯


Problem:

API latency increased


Evidence:


1.
DB query latency +300ms


2.
Connection pool exhausted


3.
Traffic spike


Possible Causes:


A.
Database bottleneck


B.
Application thread blocking


C.
Network issue



Confidence:

0.82

```

---

## 内部流程

```text
Question


 |

Evidence Collector


 |

Hypothesis Generator


 |

Evaluator


 |

Reasoning Summary

```

---

# 6. Command 2

# `/think`

## 定位

复杂任务分析模式。

---

使用：

```text
/think redesign auth module
```

---

输出：

```text
Thinking Mode


Goal:

Redesign authentication


Constraints:

- Keep compatibility
- No Redis


Options:


A.

JWT


B.

Session


C.

OAuth2



Evaluation:


A:

Low complexity


B:

Need storage


C:

External dependency



Recommendation:

JWT + Refresh Token

```

---

# 注意

不是输出内部思维链。

应该输出：

```text
Decision Analysis
```

---

# 7. Reasoning Engine 设计

核心：

## Problem Decomposition

例如：

用户：

```text
重构订单系统
```

拆：

```text
Order System


├── API

├── Domain

├── Database

├── Event

├── Testing

```

---

## Constraint Extraction

提取：

```text
Constraints:


Language:

Java


Database:

SQLite first


Architecture:

No MQ

```

---

## Option Evaluation

形成：

```text
Option Matrix


|方案|成本|风险|
|-|-|-|
A|低|中|
B|高|低|

```

---

# 8. Command 3

# `/hypothesis`

⭐⭐⭐⭐⭐

## 定位

假设管理。

特别适合：

* RCA
* Debug
* Architecture

---

使用：

```text
/hypothesis
```

---

输出：

```text
Current Hypothesis


H1:

Database bottleneck


Confidence:

70%


Evidence:

slow query log


Against:

CPU normal



H2:

Thread blocking


Confidence:

30%

```

---

# 数据模型

```rust
struct Hypothesis {


statement:String,


confidence:f32,


supporting_evidence:


Vec<Evidence>,


contradicting_evidence:


Vec<Evidence>


}
```

---

# 9. Command 4

# `/critic`

⭐⭐⭐⭐⭐

## 定位

批判 Agent 自己结果。

类似：

第二个 Reviewer Agent。

---

使用：

```text
/critic
```

---

输出：

```text
Critique Result


Current Solution:

JWT Authentication


Issues:


1.

Token Revocation missing


2.

No rate limit


3.

Refresh token security risk



Score:

7.5/10

```

---

# Critic Engine

流程：

```text
Solution


 |

Find Weakness


 |

Security Check


 |

Architecture Check


 |

Generate Feedback

```

---

# 10. Command 5

# `/reflect`

## 定位

任务完成后的反思。

---

使用：

```text
/reflect
```

---

输出：

```text
Reflection


Task:

Implement OAuth


Result:

Success


What worked:

- Clear module boundary


Problems:

- Test coverage insufficient


Learned:


Add OAuth checklist to memory?


Yes / No

```

---

# Reflection → Memory

关键闭环：

```text
Task


 |

Reflection


 |

Experience


 |

Memory


 |

Future Agent

```

---

# 11. Command 6

# `/decision`

⭐⭐⭐⭐⭐

## 定位

决策记录。

---

使用：

```text
/decision
```

---

输出：

```text
Decision Record


Decision:

Use SQLite initially


Reason:


- Simple deployment

- Easy migration


Alternatives:


MySQL


Rejected:


Too heavy for MVP


Confidence:

0.9

```

---

# 对接 ADR

自动生成：

```text
docs/adr/


001-use-sqlite.md

```

---

# 12. Cognitive Loop

核心闭环：

```text
              Goal


               |

          Understand


               |

          Hypothesis


               |

          Reason


               |

          Critique


               |

          Decision


               |

          Action


               |

          Reflection


               |

          Memory

```

这就是 Agent 自我进化循环。

---

# 13. 与 Agent Society 集成

Phase 3：

Supervisor 分配任务。

Phase 4：

Supervisor 增加认知能力。

例如：

```text
User:

Fix production issue


       |


Supervisor


       |


Reason


       |


Hypothesis


       |

Delegate


       |

RCA Agent

Coder Agent

Reviewer Agent

```

---

# 14. 与 Memory 集成

新增：

```text
Cognitive Memory
```

保存：

不是事实：

```text
Java uses Spring
```

而是经验：

```text
When migrating auth:

avoid changing token strategy together with database migration

```

---

# 15. 插件设计

新增：

```text
core-agent-plugin-cognitive


提供:


/reason

/think

/hypothesis

/critic

/reflect

/decision

```

---

# 16. 与 Core 平台连接

## core-ai

负责：

* Reasoning Model
* Critic Model
* Embedding
* Summarization

---

## core-memory

保存：

* Decision
* Experience
* Reflection

---

## core-audit

记录：

```text
Agent Decision

Confidence

Reason

Reviewer

```

---

## core-workflow

未来：

自动触发：

```text
Failure

↓

Reflection

↓

Learning

```

---

# 17. Phase 4 完成能力

完成后：

```text
core-agent


具备：


✓ Problem Reasoning

✓ Hypothesis Management

✓ Self Critique

✓ Reflection

✓ Decision Records

✓ Experience Learning


```

能力：

```text
Claude Code

+
AutoGPT

+
Devin

+
RCA Expert System

+
Decision Intelligence
```

---

# Phase 4 关键注意点

## 1. 不做 CoT 暴露

错误：

```text
显示完整思维链
```

正确：

```text
Reasoning Summary

Evidence

Decision

Confidence
```

---

## 2. Reflection 必须进入 Memory

否则：

```text
每次失败重新开始
```

---

## 3. Decision 必须结构化

不要：

```text
聊天记录
```

应该：

```text
ADR

Decision Object

Knowledge Item
```

---

完成 Phase 4 后，core-agent 的能力模型会变成：

```text
Agent OS


        Cognitive Layer


              ↑


Agent Society


              ↑


Memory / Knowledge


              ↑


Code / Tool Runtime


              ↑


Slash Runtime

```

下一阶段 Phase 5 建议进入：

# Agent Workflow Runtime（任务流与自动化层）

新增：

```text
/workflow
/trigger
/schedule
/run
/observe
/retry
```

把 Agent 从“主动对话”升级为“长期运行的自动化智能系统”。
