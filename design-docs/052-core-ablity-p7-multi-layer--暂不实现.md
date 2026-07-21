# Core-Agent P7 设计

# P7：Agent Experience Layer（多端体验层）

模块：

```text
core-agent-ui
core-agent-desktop
core-agent-terminal
core-agent-ide
core-agent-web
core-agent-mobile
```

---

# 一、P7 目标

前面：

```text
P0 Runtime

Agent 能运行


P1 Intelligence

Agent 会规划


P2 Multi-Agent

Agent 会协作


P3 Extension

Agent 会扩展


P4 Governance

Agent 企业可控


P5 Evolution

Agent 会学习


P6 Knowledge

Agent 拥有知识大脑

```

但是：

这些能力目前还是：

```text
Backend Capability
```

用户无法自然使用。

---

P7 目标：

> 把 Agent 从一个后台能力，变成用户每天使用的产品。

类似：

* Claude Desktop
* Claude Code CLI
* Cursor IDE
* ChatGPT App
* Devin Workspace

---

# 二、整体架构

```text
                         core-agent


                              |


                  Experience Runtime Layer


 ----------------------------------------------------------------


       UI Runtime


            |


 ------------------------------------------------


 Desktop       Terminal       IDE       Web       Mobile


    |              |            |          |          |


 Vue/Tauri      CLI          Plugin     Browser    App


 ------------------------------------------------


            |


       core-agent SDK


            |


       Agent Runtime

```

---

# 三、核心设计原则

## 1. 多端统一 Runtime

不要：

```text
Desktop Agent

Terminal Agent

IDE Agent

```

分别实现。

应该：

```text
              core-agent-sdk


                    |


        ----------------------


        Desktop

        CLI

        IDE

        Web

```

---

# 四、core-agent-ui ⭐⭐⭐⭐⭐

## 定位

统一 Agent UI 组件体系。

类似：

* ChatGPT UI Framework
* Claude UI
* Cursor UI

---

# 为什么需要？

所有端都有：

* Chat
* Context
* Tool 状态
* Plan
* Todo
* Approval
* Diff

如果每个重新开发：

成本巨大。

---

# UI Runtime

```text
Agent Event


    |


UI Renderer


    |


Component

```

---

# Agent UI Event

例如：

```json
{
"type":"TOOL_START",

"tool":"file.read"
}
```

UI：

显示：

```
正在读取文件...
```

---

# Component 类型

```text
ChatMessage

ToolCard

PlanView

TodoList

DiffViewer

ApprovalDialog

ContextChip

AgentTree

TraceViewer

```

---

# UX

统一：

```text
Agent Workspace


--------------------------------

Chat


你好，我需要分析这个问题


--------------------------------


Plan

✓ 分析日志

✓ 查询指标


--------------------------------


Tools

log.query running...


--------------------------------

```

---

# 注意点

UI 不应该知道业务。

不要：

```
RCA Dashboard Component
```

应该：

```
AgentTask Component
```

---

---

# 五、core-agent-desktop ⭐⭐⭐⭐⭐

## 定位

桌面 Agent 应用。

技术：

建议：

```text
Tauri

+

Vue3

+

Rust

```

符合你之前技术路线。

---

# 核心能力

## 1. Workspace

类似：

Claude Desktop：

```text
Workspace


Project

Files

Agent

History

```

---

## 2. Local Runtime

支持：

```text
Local Agent

Remote Agent

Hybrid Agent

```

---

例如：

本地：

```text
File Tool

Shell Tool

Git Tool

```

远程：

```text
Cloud LLM

Enterprise Agent

```

---

# 3. 文件上下文

重点结合之前：

core-context。

体验：

选中文件：

```
UserService.java

Line 20-50

```

右键：

```
Ask Agent

Comment

Fix

Review

```

---

# 4. Agent Sidebar

类似 Cursor：

```
Project


Agent


Tasks


Memory


Tools


History

```

---

# UX

整体：

黑金科技风：

```
+------------------------------------------------+

 Files              Agent Chat


 src/

 User.java          这里发现空指针


                    Context:

                    User.java L20-50


                    Plan:

                    ✓ Analyze

                    ⏳ Fix


+------------------------------------------------+

```

---

# 注意点

Desktop 不做 Agent 逻辑。

只是：

```
Experience Shell

```

---

---

# 六、core-agent-terminal ⭐⭐⭐⭐⭐

## 定位

CLI Agent。

类似：

* Claude Code
* OpenCode
* Codex CLI

---

# 为什么重要？

开发者效率最高。

---

# CLI 架构

```text
agent command


 |

CLI Parser


 |

core-agent-sdk


 |

Agent Runtime

```

---

# 命令设计

```bash
agent chat

agent run

agent plan

agent task

agent tools

agent memory

agent config

```

---

# Slash 支持

结合 P3：

```bash
/review

/debug

/explain

/test

```

---

# Terminal UI

类似：

```
> analyze this bug


Agent:


Planning...


✓ Read logs


✓ Analyze code


Need approval:


run test?


(y/n)

```

---

# 注意点

CLI 是：

高级用户入口。

不要做成：

聊天窗口搬到终端。

---

---

# 七、core-agent-ide ⭐⭐⭐⭐⭐

## 定位

IDE Agent。

类似：

* Cursor
* GitHub Copilot
* JetBrains AI

---

# 支持 IDE

P0：

VS Code

未来：

```
IntelliJ

Vim

Neovim

```

---

# 核心能力

## Code Context

依赖：

P6:

```text
Semantic

AST

Knowledge

```

---

## Inline Edit

例如：

代码：

```java
userService.save()
```

选中：

```
优化性能
```

Agent：

生成：

```diff
-
+

```

---

## Code Review

结合：

P1 Reflection。

---

# IDE 架构

```text
IDE Plugin


 |

core-agent-sdk


 |

Agent Runtime

```

---

# UX

右侧：

```
Agent


Explain

Fix

Review


```

代码：

```
Ctrl + K

Ask Agent

```

---

# 注意点

IDE 插件不要复制 Cursor。

核心优势：

连接你的：

```
core-agent ecosystem

```

---

---

# 八、core-agent-web ⭐⭐⭐⭐

## 定位

浏览器 Agent 平台。

类似：

ChatGPT Web。

---

# 能力

普通用户：

```
Chat

Knowledge

Agent Store

History

```

企业：

```
Agent Console

Workflow

Audit

Cost

```

---

# 页面

```
Agent Hub


------------------

My Agents


Knowledge


Marketplace


History


Settings

```

---

# 注意点

Web 是入口。

不是核心。

---

---

# 九、core-agent-mobile ⭐⭐⭐

## 定位

移动端 Agent。

场景：

* 查看任务
* 审批
* 通知
* 简单问答

---

# 不建议做：

完整 Desktop。

---

# Mobile MVP

支持：

```
Chat

Approval

Notification

Task Status

```

---

# 十、多端统一 SDK

核心：

```text
core-agent-sdk

```

---

# API

```typescript
AgentClient {


chat()


execute()


approve()


context()


subscribe()


}

```

---

# Event Stream

所有端：

统一：

```json
{
event:

"TASK_UPDATE",

data:{}

}

```

---

# 十一、P7 交互统一模型

所有端：

```text
                 User


                  |


             Experience


                  |


              SDK


                  |


        core-agent-runtime


                  |


       Agent / Tool / Memory

```

---

# 十二、P7 Repo 设计

```text
core-agent


├── core-agent-ui


├── core-agent-desktop


├── core-agent-terminal


├── core-agent-ide


├── core-agent-web


├── core-agent-mobile


├── core-agent-sdk

```

---

# 十三、P7 MVP 顺序

## Phase 1

优先：

```text
core-agent-terminal

core-agent-sdk

```

原因：

最快验证 Agent。

---

## Phase 2

```text
core-agent-desktop

```

形成产品。

---

## Phase 3

```text
core-agent-ide

```

开发者场景。

---

## Phase 4

```text
core-agent-web

```

企业入口。

---

## Phase 5

```text
core-agent-mobile

```

生态补充。

---

# 十四、P7 完成后的能力

Agent 演进：

```
P0
Runtime


↓

P1
Planning


↓

P2
Multi-Agent


↓

P3
Extension


↓

P4
Governance


↓

P5
Evolution


↓

P6
Knowledge


↓

P7
Experience

```

最终：

```
一个 Agent OS

同时支持:

CLI

Desktop

IDE

Web

Mobile

```

---

# P7 完成后，你的 core-agent 已经接近：

```
Claude Code
+
Cursor
+
ChatGPT
+
Devin
+
Enterprise Agent Platform
```

下一阶段 P8 建议进入：

```
core-agent-cloud
core-agent-cluster
core-agent-runtime-scheduler
core-agent-deployment
core-agent-scale
```

即：

**Agent Infrastructure Layer（云原生运行基础设施层）**

解决：

> 大规模 Agent 如何稳定运行。
