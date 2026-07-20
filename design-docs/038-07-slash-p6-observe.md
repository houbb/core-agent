# Phase 6：Core-Agent Observability & Evaluation Runtime（Agent 可观测与评估层）

## 目标

前面 Phase 0～5：

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

        ↓

Phase 5
Workflow Runtime
```

现在 Core-Agent 已经可以：

* 自主执行任务
* 调度多个 Agent
* 使用工具
* 运行 Workflow
* 保存经验

但是进入企业场景，会出现一个关键问题：

> Agent 为什么这么做？做得好不好？如何持续优化？

传统软件：

```text
Application

↓

Logs

↓

Metrics

↓

Tracing

```

Agent 系统需要：

```text
Agent

↓

Reasoning Trace

↓

Decision Trace

↓

Tool Trace

↓

Evaluation

↓

Optimization

```

因此 Phase 6 建立：

# Agent Observability & Evaluation Runtime

目标：

让 Agent 从：

```text
Black Box AI
```

变成：

```text
Observable Intelligent System
```

---

# 新增 Slash 命令

```text
/trace-agent
/evaluate
/benchmark
/debug
/replay
/score
```

对应：

| 命令             | 能力          |
| -------------- | ----------- |
| `/trace-agent` | Agent 执行链追踪 |
| `/evaluate`    | 任务质量评估      |
| `/benchmark`   | 能力基准测试      |
| `/debug`       | Agent 调试    |
| `/replay`      | 历史执行回放      |
| `/score`       | Agent 评分    |

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
├── workflow-runtime
│
├── observability-runtime       ⭐
│
├── tracing-engine              ⭐
│
├── evaluation-engine           ⭐
│
├── benchmark-engine            ⭐
│
├── replay-engine               ⭐
│
└── quality-engine              ⭐

```

---

# 2. Agent Observability Architecture

```text
                 User Task


                    |

                    v


              Agent Runtime


                    |

       +------------+-------------+

       |            |             |

    Reasoning    Tool Call     Decision


       |            |             |


       +------------+-------------+

                    |

                    v


             Trace Collector


                    |

        +-----------+------------+

        |                        |

   Evaluation              Storage


        |                        |

 Quality Score          core-audit


```

---

# 3. 核心设计理念

## Agent Trace ≠ 普通 Log

传统：

```text
INFO request started
INFO database query
```

Agent：

需要记录：

```text
Goal

Context

Plan

Decision

Tool

Observation

Reflection

Result

```

---

# 4. Agent Trace 数据模型

核心：

```rust
struct AgentTrace {


trace_id:String,


session_id:String,


agent_id:String,


goal:String,


steps:Vec<TraceStep>,


result:String,


score:Option<f32>,


created_at:Timestamp


}

```

---

# Trace Step

```rust
struct TraceStep {


type:TraceType,


input:String,


output:String,


tool:String,


duration:u64,


token_usage:u32


}

```

---

# Trace Type

```text
Planning

Reasoning

Delegation

ToolCall

Observation

Decision

Reflection

Response

```

---

# Command 1

# `/trace-agent`

## 定位

查看 Agent 执行过程。

---

使用：

```text
/trace-agent
```

或者：

```text
/trace-agent run-001
```

---

输出：

```text
╭────────────────────╮
 Agent Trace
╰────────────────────╯


Task:

Fix authentication bug


Timeline:


10:01

Planner Agent

Created plan


10:03

Coder Agent

Modified AuthService


10:05

Test Agent

Executed tests


10:06

Reviewer Agent

Approved


Result:

Success


```

---

# Desktop UX

类似：

OpenTelemetry Trace View

```text
------------------------------------------------

Agent Execution


Planner
 █████


Coder
      ███████


Reviewer
             ███


Tools


git
     ██


test
        ███


------------------------------------------------

```

---

# 5. Trace Storage

MVP：

SQLite：

```text
agent_trace

trace_step

tool_execution

decision_record

```

未来：

```text
ClickHouse

+

OpenTelemetry

```

---

# 6. Command 2

# `/evaluate`

⭐⭐⭐⭐⭐

## 定位

评价一次 Agent 任务质量。

---

使用：

```text
/evaluate run-001
```

---

输出：

```text
Evaluation Result


Task:

Implement login


Score:

8.6 / 10


Criteria:


Correctness

9


Security

8


Code Quality

9


Efficiency

8



Issues:


Missing rate limit

```

---

# Evaluation Engine

不是简单 LLM 打分。

采用：

## Multi Dimension Evaluation

```text
Quality Score


=

Correctness

+

Safety

+

Efficiency

+

Maintainability

+

User Satisfaction

```

---

# Evaluator Agent

类似：

```text
Reviewer Agent

+

Judge Model

```

---

# 7. Evaluation 数据模型

```rust
struct Evaluation {


trace_id:String,


criteria:


Vec<ScoreItem>,


overall:f32,


feedback:String


}

```

---

# 8. Command 3

# `/benchmark`

## 定位

Agent 能力测试。

---

使用：

```text
/benchmark coder-agent
```

---

输出：

```text
Benchmark


Agent:

Coder


Tasks:

100


Success:

87


Average Score:

8.2


Average Cost:

$0.12


```

---

# Benchmark Dataset

类似：

* SWE-bench
* HumanEval
* AgentBench

但是企业内部：

```text
company benchmark


```

---

# Benchmark 类型

```text
Coding

RCA

Security

Architecture

Documentation

```

---

# 9. Command 4

# `/debug`

## 定位

Agent Debugger。

---

使用：

```text
/debug run-001
```

---

输出：

```text
Debug


Failure Point:


Step 7


Agent:

Coder


Problem:


Wrong file selected



Root Cause:


Context retrieval failed


Recommendation:


Improve search ranking

```

---

# Debug Pipeline

```text
Failure


 |

Trace Analysis


 |

Pattern Detection


 |

Root Cause


 |

Fix Suggestion

```

---

# 10. Command 5

# `/replay`

⭐⭐⭐⭐⭐

## 定位

执行回放。

---

为什么重要？

Agent 是概率系统。

必须支持：

```text
昨天成功

今天失败

为什么？
```

---

使用：

```text
/replay run-001
```

---

输出：

```text
Replay


Original:

GPT-5


Context:

same


Tools:

same


Result:


Difference detected


Step 5 changed


```

---

# Replay 架构

类似：

* Temporal Replay
* Event Sourcing

保存：

```text
Event History


↓

Reconstruct State


↓

Replay

```

---

# 11. Command 6

# `/score`

## 定位

快速查看 Agent 健康度。

---

使用：

```text
/score
```

---

输出：

```text
Agent Health


Coder Agent


Success Rate:

92%


Avg Score:

8.7


Cost:

$0.15/task


Latency:

45s


```

---

# 12. Agent Quality Loop

Phase 6 核心闭环：

```text
              Execute


                 |

              Trace


                 |

             Evaluate


                 |

              Score


                 |

             Improve


                 |

              Memory


                 |

              Better Agent

```

---

# 13. 与 Phase 4 Cognitive Runtime 集成

认知：

```text
为什么这样决定
```

可观测：

```text
记录这个决定
```

关系：

```text
Cognitive Runtime

        |

        v

Decision Trace

        |

        v

Evaluation

```

---

# 14. 与 Phase 5 Workflow 集成

Workflow：

```text
执行流程
```

Observability：

```text
观察流程
```

例如：

```text
RCA Workflow


Trigger

 ↓

Agent


 ↓

Tool


 ↓

Decision


 ↓

Fix


 ↓

Evaluation

```

---

# 15. 插件设计

新增：

```text
core-agent-plugin-observability


提供:


/trace-agent

/evaluate

/benchmark

/debug

/replay

/score

```

---

# 16. 与 Core 平台连接

## core-audit

记录：

```text
Agent Action

Decision

Approval

Tool Usage

```

---

## core-ai

负责：

```text
Evaluation Model

Judge Model

Summary

```

---

## core-storage

保存：

```text
Trace

Benchmark Dataset

Evaluation Report

```

---

## core-billing

统计：

```text
Token Cost

Workflow Cost

Agent Cost

```

---

# 17. Phase 6 完成能力

完成后：

```text
core-agent


拥有：


✓ Agent Trace

✓ Execution Replay

✓ Quality Evaluation

✓ Benchmark

✓ Debugging

✓ Performance Metrics


```

能力：

```text
Claude Code

+

OpenTelemetry

+

LangSmith

+

Datadog APM

+

Enterprise AI Governance

```

---

# Phase 6 关键注意点

## 1. Trace 必须从第一天设计

错误：

后期：

```text
增加日志
```

正确：

```text
Agent Event Model First
```

因为：

Agent 行为不可预测。

---

## 2. Evaluation 不等于打分

真正价值：

```text
Score

+

Why

+

How Improve

```

---

## 3. Replay 是企业信任基础

企业问：

> 为什么 AI 修改了这个文件？

必须回答：

```text
完整执行链

完整上下文

完整决策

完整工具调用
```

---

完成 Phase 6 后，core-agent 基本形成：

```text
                  Agent OS


                       |

          Observability & Evaluation


                       |

             Workflow Runtime


                       |

             Agent Society


                       |

            Cognitive Runtime


                       |

          Memory / Knowledge


                       |

          Code / Tool Runtime

```

下一阶段建议：

# Phase 7：Agent Marketplace & Extension Runtime（Agent 生态扩展层）

新增：

```text
/plugin
/install
/publish
/registry
/capability
```

开始进入类似：

* VS Code Extension Marketplace
* Claude Code Plugin
* OpenAI Apps
* MCP Ecosystem

的生态阶段。
