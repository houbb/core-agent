# 说明

在我们的 core-agent 开发中，有很多核心的概念。agent subagent tools llm context memory plan mcp tools skills slash(/) 还有 @(AT) 梳理一下这些核心概念+使用时的关系+区别

# 解释

在设计 **core-agent** 时，这些概念其实属于不同层次的问题。很多 Agent 框架混乱的原因，就是把「能力」「执行者」「知识」「工具」「协议」「交互入口」混在一起。

我建议按照 **Agent OS 分层模型** 来理解：

```
                 User Interface Layer
        ┌──────────────────────────────┐
        │ Slash / Command / @ Mention   │
        └──────────────┬───────────────┘
                       │
                 Agent Orchestration
        ┌──────────────▼───────────────┐
        │ Agent / SubAgent / Plan      │
        └──────────────┬───────────────┘
                       │
              Cognitive Runtime
        ┌──────────────▼───────────────┐
        │ Context / Memory / Reasoning │
        └──────────────┬───────────────┘
                       │
              Capability Runtime
        ┌──────────────▼───────────────┐
        │ Tools / Skills / MCP / APIs  │
        └──────────────┬───────────────┘
                       │
                    Models
        ┌──────────────▼───────────────┐
        │ LLM / Embedding / VLM        │
        └──────────────────────────────┘
```

下面逐个拆。

---

# 1. Agent

## 定义

Agent 是一个「自主完成目标的智能执行主体」。

核心：

> Agent = Goal + Reasoning + Memory + Tools + Action Loop

例如：

用户：

> 帮我分析线上故障原因

Agent：

```
理解目标
 ↓
制定计划
 ↓
查询监控
 ↓
分析日志
 ↓
调用知识库
 ↓
生成 RCA
 ↓
输出报告
```

---

## Agent 内部结构

```
Agent
 |
 |-- Identity
 |     名称
 |     角色
 |
 |-- Goal
 |     目标
 |
 |-- Brain
 |     LLM
 |
 |-- Memory
 |     记忆
 |
 |-- Tools
 |     能力
 |
 |-- Planner
 |     规划
 |
 |-- Executor
       执行
```

---

# 2. SubAgent

## 定义

SubAgent 是 Agent 的子智能体。

为什么需要？

因为复杂任务需要专业分工。

例如：

一个 RCA Agent：

```
RCA-Agent

    |
    |
    +-- Log-Agent
    |
    +-- Metric-Agent
    |
    +-- Trace-Agent
    |
    +-- Knowledge-Agent
```

每个 SubAgent：

* 有自己的 prompt
* 有自己的工具
* 有自己的记忆
* 有自己的职责

---

## Agent 和 SubAgent 区别

|      | Agent  | SubAgent |
| ---- | ------ | -------- |
| 身份   | 主智能体   | 辅助智能体    |
| 目标   | 完成用户任务 | 完成子任务    |
| 生命周期 | 长期     | 短期/动态    |
| 权限   | 更多     | 受限       |
| 上下文  | 完整     | 局部       |

---

类似公司：

```
CEO Agent

   CTO Agent

   CFO Agent

   Engineer Agent
```

---

# 3. LLM

## 定义

LLM 是大脑模型。

比如：

* GPT
* Claude
* Gemini
* DeepSeek

它负责：

```
输入
 |
理解
 |
推理
 |
生成
```

但是：

LLM 本身不是 Agent。

区别：

```
LLM

不会主动行动

不会调用工具

不会保存记忆

不会规划


Agent

LLM
+
Memory
+
Tools
+
Loop
```

---

# 4. Context（上下文）

## 定义

Context 是：

> 当前一次 Agent 思考时看到的信息

例如：

用户：

```
帮我优化代码
```

Context：

```
当前代码
+
错误日志
+
用户需求
+
历史对话
+
工具结果
```

结构：

```
Context Window

--------------------------------

System Prompt

Agent Identity


User Message


Conversation


Tool Result


Memory Retrieval


Current Plan


--------------------------------
```

---

## Context 特点

短生命周期。

一次任务结束：

消失。

例如：

今天：

```
帮我写代码
```

Context 有：

```
代码
错误
讨论
```

明天：

不存在。

---

# 5. Memory（记忆）

Memory 是：

> 跨 Context 保存的信息

类似人的长期记忆。

分类：

## 1. Short Memory

短期记忆

```
当前任务状态
```

例如：

```
正在分析订单服务
步骤3完成
```

---

## 2. Long Memory

长期记忆

例如：

用户喜欢：

```
Java
Spring Boot
黑金 UI
```

---

## 3. Knowledge Memory

知识库：

```
公司文档
代码
技术规范
```

---

Memory 架构：

```
             Memory

              |
     -------------------
     |        |        |
 Working   Long    Knowledge
 Memory    Memory    Base

```

---

# 6. Plan（计划）

## 定义

Plan 是 Agent 的任务分解方案。

例如：

用户：

> 创建一个用户系统

Plan：

```
Step1:
设计数据库

Step2:
创建 API

Step3:
实现认证

Step4:
测试

Step5:
部署
```

---

Plan 不等于 Workflow。

区别：

|     | Plan  | Workflow |
| --- | ----- | -------- |
| 产生者 | Agent | 人设计      |
| 动态  | 动态调整  | 固定       |
| 目标  | 解决问题  | 执行流程     |

---

Agent：

```
思考:

这个任务怎么完成？

生成 Plan
```

---

# 7. Tools（工具）

## 定义

Tool 是 Agent 可以调用的外部能力。

例如：

```
查询数据库

搜索网页

执行代码

发送邮件

调用 API
```

形式：

```
Tool

name

description

input schema

output schema

handler
```

例如：

```json
{
"name":"search_database",
"description":"查询订单",
"input":{
"id":"string"
}
}
```

---

Agent：

```
我不会查数据库

但是我有 database_tool

所以调用它
```

---

# 8. Skills（技能）

这是容易混淆的。

Skill 是：

> 一组解决某类问题的方法论+工具组合

例如：

Tool:

```
git_commit
```

只是能力。

Skill:

```
Software Engineering Skill

包含：

git
代码分析
测试
review
发布
```

---

关系：

```
Skill

  |
  +-- Prompt
  |
  +-- Tools
  |
  +-- Workflow
  |
  +-- Rules
```

---

类似：

人：

工具：

```
锤子
```

技能：

```
木工技能
```

---

# 9. MCP

MCP:

Model Context Protocol

它不是工具。

它是：

> 工具连接标准协议

解决：

以前：

```
Agent

 |
 |-- OpenAI API
 |-- GitHub API
 |-- DB API
 |-- Slack API

每个不同
```

MCP:

```
Agent

 |
 MCP Client

 |
 MCP Server


 |
Tools
Resources
Prompts
```

---

类似 USB：

以前：

```
每个设备不同接口
```

USB:

```
统一接口
```

---

MCP 提供：

## Tools

可调用能力

## Resources

数据资源

## Prompts

模板

---

# 10. Slash (/)

Slash 是：

> 人类主动控制 Agent 的命令入口

例如：

```
/plan

/compact

/reset

/review

/explain
```

它属于 UX 层。

流程：

```
用户

输入:

/plan


↓

Command Router


↓

Agent Runtime


↓

执行 Plan
```

---

Slash 特点：

主动。

用户知道自己要什么。

---

# 11. @ (AT Mention)

@ 是：

> 指定上下文对象或者 Agent

例如：

```
@database

帮我分析这个表
```

或者：

```
@frontend-agent

优化页面
```

---

作用：

选择：

* Agent
* 文件
* 人
* Skill
* Resource

---

例如：

```
@code-review

检查这个 PR
```

等价：

```
选择 Code Review Skill
+
加载相关 Context
```

---

# 12. 它们之间完整关系

一个完整请求：

用户：

```
@RCA-Agent

分析订单服务异常

/plan
```

流程：

```
User
 |
 |
 |  /plan
 |
Command Layer
 |
 |
Agent Router
 |
 |
RCA-Agent
 |
 |
Create Plan
 |
 |
--------------------
|
|-- Log SubAgent
|
|-- Metric SubAgent
|
|-- Trace SubAgent
|
--------------------
 |
 |
Need data

 |
Tools

 |
MCP

 |
Database API

 |
Result


 |
Memory Update


 |
Final Answer

```

---

# 13. Core-Agent 推荐最终模型

我建议你的 core-agent 定义如下：

```
core-agent

├── Agent Runtime
│
├── SubAgent Runtime
│
├── Planner Runtime
│
├── Context Runtime
│
├── Memory Runtime
│
├── Tool Runtime
│
├── Skill Runtime
│
├── MCP Runtime
│
├── Command Runtime
│
└── Mention Runtime

```

对应：

| 概念       | Core模块             |
| -------- | ------------------ |
| Agent    | core-agent-runtime |
| SubAgent | core-agent-runtime |
| LLM      | core-ai            |
| Context  | core-context       |
| Memory   | core-memory        |
| Plan     | core-planner       |
| Tools    | core-tool          |
| Skills   | core-skill         |
| MCP      | core-mcp           |
| Slash    | core-command       |
| @        | core-mention       |

---

# 14. 一句话总结

```
LLM
= 大脑


Agent
= 有目标的大脑


SubAgent
= 专业分工的大脑


Context
= 当前思考内容


Memory
= 长期记忆


Plan
= 思考路线


Tool
= 手脚


Skill
= 方法论


MCP
= 手脚标准接口


Slash
= 人主动下命令


@
= 指定对象/能力

```

对于你的 **core-agent**，下一步最关键的是设计：

```
Agent Runtime
      |
      +-- Planner
      +-- Context Engine
      +-- Memory Engine
      +-- Tool Runtime
      +-- Skill Runtime
      +-- MCP Runtime
      +-- Command Runtime
```

这基本就是一个 **Agent Operating System 的核心内核**。你前面设计的 core-platform，其实正好可以承载这些 runtime。
