# Core-Agent P1 设计

## 目标

P0 完成后，Agent 已经具备：

```
理解输入
调用 LLM
调用 Tool
管理 Context
保存 Memory
控制 Permission
```

但是它更像：

> 一个会执行命令的智能助手

P1 的目标：

让 Agent 从：

```
Reactive Agent（响应式）
```

升级为：

```
Proactive Agent（规划式）
```

核心能力：

```
理解目标
↓
拆解任务
↓
制定计划
↓
执行步骤
↓
询问人类
↓
自我检查
↓
修正方案
```

---

# P1 总体架构

```text
                         core-agent-runtime


                                |

                     Intelligence Runtime


      -------------------------------------------------


      Planner          Task            Todo

        |               |               |

        |               |               |

   Plan Graph      Task State      User View



      Question                         Reflection

        |                                  |

 Human Loop                         Self Evaluation



      -------------------------------------------------


                         Executor

                              |

                       P0 Runtime


```

---

# P1 模块职责

| 模块                    | 作用     | 核心价值        |
| --------------------- | ------ | ----------- |
| core-agent-planner    | 任务规划   | 让 Agent 会思考 |
| core-agent-task       | 任务执行管理 | 让复杂任务可追踪    |
| core-agent-question   | 人机协作   | 避免错误决策      |
| core-agent-todo       | 用户可见进度 | 提升 UX       |
| core-agent-reflection | 自我检查   | 提升质量        |

---

# 1. core-agent-planner ⭐⭐⭐⭐⭐

## 定位

Agent 的项目经理 + 架构师。

类似：

* Claude Code Plan Mode
* OpenAI Codex planning
* AutoGPT planning

---

# 核心能力

## 1. Goal Understanding

输入：

```
帮我增加 OAuth 登录
```

Planner:

分析：

```
目标：

增加 OAuth


涉及：

认证模块

数据库

前端页面

配置

测试

```

---

# 2. Task Decomposition

拆解：

```
OAuth 登录

 |
 +-- 设计数据库
 |
 +-- 后端接口
 |
 +-- Provider 接入
 |
 +-- 前端页面
 |
 +-- 测试
```

---

# Plan 数据结构

```java
Plan {

 id;

 goal;

 description;

 tasks[];

 strategy;

 status;

}
```

例如：

```json
{
"goal":"add oauth",

"tasks":[

{
"name":"create oauth table"
},

{
"name":"implement callback"
}

]

}
```

---

# 3. Plan Graph

不要简单 List。

推荐：

DAG：

```
Task A
   |
   v
Task B
   |
 +----+
 |    |
Task C Task D

```

原因：

支持并行 Agent。

---

# 4. Planner Strategy

支持：

```text
AUTO

PLAN_ONLY

STEP_BY_STEP

INTERACTIVE
```

---

## UX

Desktop：

用户：

```
实现支付系统
```

Agent：

显示：

```
正在制定计划...


Plan:

1.
设计订单模型

2.
支付接口

3.
支付回调

4.
测试


[开始执行]

[修改计划]

```

---

# 注意点

## 不要让 Planner 直接执行。

错误：

```
Planner
 |
修改代码
```

正确：

```
Planner

 |
生成 Plan

 |

Executor
```

---

# 2. core-agent-task ⭐⭐⭐⭐⭐

## 定位

任务生命周期管理。

Planner 产生 Task。

Runtime 执行 Task。

---

# Task Model

```java
Task {


id;


parentId;


name;


description;


status;


priority;


executor;


dependencies;


result;


}
```

---

# 状态机

```text

CREATED

 |
 v

READY

 |
 v

RUNNING

 |
 +------+

 |      |

SUCCESS FAILED


 |
 v

COMPLETED

```

---

# Task 类型

```text
LLM Task

Tool Task

Human Task

Agent Task

Workflow Task

```

---

例如：

RCA：

```
Task:

查询日志


type:

ToolTask


tool:

log.query

```

---

# Task Execution

流程：

```
Task

 |

Executor


 |

Result


 |

Update State

```

---

# Task Queue

P1 简单：

```
SQLite Queue
```

未来：

```
Redis
Kafka
Temporal
```

---

# UX

任务面板：

```
任务:


✓ 分析日志

✓ 查询指标

⏳ 分析 Trace

○ 输出报告


```

---

# 注意点

Task 必须独立。

不要：

```
Agent Context = Task
```

应该：

```
Agent

 |

Task

 |

Context

```

---

# 3. core-agent-question ⭐⭐⭐⭐⭐

## 定位

Human-in-the-loop。

解决：

> AI 不确定怎么办？

---

# 触发场景

## 1. 信息不足

例如：

```
数据库地址？
```

---

## 2. 多方案

例如：

```
缓存方案：

A Redis

B Caffeine

请选择
```

---

## 3. 高风险操作

例如：

```
是否允许删除生产数据？
```

---

# Question Model

```java
Question {


id;


type;


content;


options;


required;


timeout;


answer;


}
```

---

# Question 类型

```text
CHOICE

CONFIRM

INPUT

APPROVAL

REVIEW

```

---

# UX

弹窗：

```
Agent 需要你的决定


请选择数据库：

○ MySQL

○ PostgreSQL

○ SQLite


[确认]

```

---

# Terminal UX

```
Question:

是否执行 mvn clean?


(y/n)

```

---

# 注意点

Question 不应该只是异常。

它应该成为：

```
Agent 协作接口
```

---

# 4. core-agent-todo ⭐⭐⭐⭐

## 定位

用户可见任务列表。

类似：

* Claude Code Todo
* ChatGPT Canvas checklist

---

# Todo Model

```java
Todo {


id;


taskId;


content;


status;


order;


}
```

---

# Todo 和 Task 区别

非常重要：

|      | Todo | Task |
| ---- | ---- | ---- |
| 用户视角 | ✅    | ❌    |
| 执行实体 | ❌    | ✅    |
| 简单   | ✅    | ❌    |
| 复杂   | ❌    | ✅    |

例如：

Task:

```
实现登录模块
```

Todo:

```
[x]
创建 Controller


[ ]
添加测试

```

---

# UX

实时显示：

```
Agent Progress


[x] 分析需求

[x] 创建方案

[ ] 修改代码

[ ] 测试

```

---

# 注意点

Todo 不参与执行。

不要：

```
Todo = Workflow
```

---

# 5. core-agent-reflection ⭐⭐⭐⭐⭐

## 定位

Agent 自我评价。

这是 Agent 从：

```
执行者

↓

专家

```

的关键。

---

# Reflection Loop

```
Execute


 |

Evaluate


 |

Improve


 |

Retry


```

---

# Reflection 类型

## 1. Result Reflection

检查：

```
结果是否满足目标？
```

---

## 2. Code Reflection

例如：

```
代码是否符合规范？
```

---

## 3. Reasoning Reflection

检查：

```
推理是否合理？
```

---

## 4. Tool Reflection

检查：

```
工具结果是否可信？
```

---

# Reflection Model

```java
Reflection {


taskId;


criteria;


score;


issues;


suggestions;


}
```

---

# Example

任务：

```
修复 Bug
```

执行：

```
修改代码

测试通过
```

Reflection:

```
检查:

是否新增测试?

是否影响其他模块?


评分:

85

建议:

增加异常测试

```

---

# UX

执行结束：

```
完成


质量检查:

代码:
90


测试:
80


风险:
中


建议:
增加缓存测试


[接受]

[继续优化]

```

---

# 注意点

Reflection 不要无限循环。

必须：

```text
max retry

budget

score threshold

```

---

# P1 模块关系

```text

                 User Goal

                    |

                    v


              Planner


                    |

                    v


              Plan Graph


                    |

                    v


              Tasks


                    |

        ----------------------

        |                    |

     Todo                 Executor


                              |

                            P0


                              |

                           Result


                              |

                        Reflection


                              |

                    Success / Retry



                    Question

                       |

                     Human

```

---

# P1 Repo 设计

保持你的 Core Platform 规范：

```
core-agent


├── core-agent-runtime

├── core-agent-planner

├── core-agent-task

├── core-agent-question

├── core-agent-todo

├── core-agent-reflection


```

每个：

```
backend
frontend
admin
sdk
```

---

# P1 MVP 范围建议

第一版不要复杂。

## 必须：

```
Planner

Task

Todo

Question

Reflection
```

实现：

一个 Agent 可以：

```
用户输入目标

↓

生成计划

↓

展示 Todo

↓

执行 Task

↓

遇到问题询问

↓

执行完成

↓

自检

```

---

# P1 完成后的能力

你的 Agent 从：

P0：

```
Chat + Tool
```

升级：

P1：

```
Goal-driven Agent
```

具备：

```
Planning
Execution
Human Collaboration
Self Evaluation
```

这一步完成后，才真正接近：

* Claude Code Agent Loop
* Codex Agent
* Devin 类 Agent

下一阶段 P2：

```
core-agent-subagent
core-agent-message
core-agent-orchestrator
```

会进入 **Multi-Agent System（多智能体系统）**。
