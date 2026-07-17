这里我想先调整一下命名。

**Phase 4 我不建议叫 Team Agent Platform。**

因为到了这个阶段，平台已经不仅仅是在"团队协作"了。

真正发生变化的是：

> **一个人使用 Agent → 多个人共同运营 Agent。**

所以我建议叫：

# Phase 4：Collaborative Agent Platform ⭐⭐⭐⭐⭐

或者更直白一点：

> **Agent Collaboration Platform**

因为这里不仅有：

* Team
* Project
* Review
* Approval
* Sharing

未来还有：

* Organization
* Department
* Enterprise

Team 只是其中一个概念。

---

# Phase 4 定位

一句话：

> **让 Agent 从个人工具变成团队资产。**

OpenCode：

今天：

```text
Developer

↓

Agent
```

Phase4：

变成：

```text
Developer A

        │

Developer B

        │

Architect

        │

Tester

        │

Agent Platform
```

第一次：

Agent：

开始：

共享。

---

# 第一性原理

个人：

关注：

```text
聊天
```

团队：

关注：

```text
协作
```

企业：

关注：

```text
治理
```

所以：

Phase4：

核心：

不是：

AI。

而是：

**Collaboration。**

---

# 整体架构

```text
                Collaboration Platform

------------------------------------------------------

Project Center

Agent Registry

Shared Workspace

Task Center

Review Center

Approval Center

Knowledge Center

Notification Center

------------------------------------------------------

Studio API

------------------------------------------------------

core-agent
```

注意：

仍然：

没有：

AI。

AI：

全部：

core-agent。

---

# 左侧导航

开始：

增加：

团队概念。

```text
🏠 Home

📁 Projects

🤖 Agents

👥 Team

📋 Tasks

🔍 Reviews

📚 Knowledge

📊 Trace

🔔 Notifications

⚙ Settings
```

---

# ① Project Center ⭐⭐⭐⭐⭐

这是：

整个：

平台：

核心。

不是：

Workspace。

而是：

Project。

例如：

```text
Monolith

Members

8

Agents

5

Tasks

21

Knowledge

Ready
```

进入：

Project。

所有：

Agent：

自动：

切换。

---

建议：

结构：

```text
Project

├── Members

├── Agents

├── Workflows

├── Knowledge

├── Memory

├── Tasks

└── Settings
```

以后：

RCA。

也是：

Project。

---

# ② Agent Registry ⭐⭐⭐⭐⭐

注意。

不是：

Marketplace。

Registry。

例如：

团队：

拥有：

```text
Coding Agent

Architect Agent

Reviewer Agent

QA Agent

RCA Agent
```

全部：

共享。

点击：

进入：

Agent。

例如：

```text
Name

Coding Agent

Owner

Echo

Version

1.3

Model

Claude

Memory

Shared
```

以后：

团队：

直接：

使用。

---

# ③ Shared Workspace ⭐⭐⭐⭐⭐

这是：

Desktop：

升级。

不是：

个人：

Workspace。

而是：

团队：

Workspace。

例如：

```text
Project

Chat

Trace

Tasks

Knowledge

Review
```

别人：

打开。

继续。

---

例如：

Architect：

昨天：

分析：

系统。

今天：

Developer：

继续。

不用：

重新：

Prompt。

---

# ④ Task Center ⭐⭐⭐⭐⭐

Professional：

必须：

Task。

例如：

```text
#102

Refactor Login

Running

Owner

Coding Agent

Reviewer

Architect

Progress

70%
```

支持：

暂停。

恢复。

转交。

---

以后：

Workflow：

这里。

---

# ⑤ Review Center ⭐⭐⭐⭐⭐

这里：

不是：

Code Review。

而是：

AI Review。

例如：

Agent：

完成：

任务。

自动：

进入：

Review。

```text
Pending Review


Coding Agent

✓


Architect Agent

Waiting


Human

Required
```

点击：

Diff。

Approve。

Reject。

Comment。

---

以后：

企业：

必须。

---

# ⑥ Approval Center ⭐⭐⭐⭐☆

例如：

危险：

操作。

Agent：

不能：

执行。

例如：

```text
Delete Database
```

自动：

进入：

Approval。

```text
Approve


✓

Reject

×
```

以后：

企业：

直接。

---

# ⑦ Knowledge Center ⭐⭐⭐⭐⭐

Phase3：

只是：

查看。

现在：

团队：

维护。

例如：

```text
Knowledge


Architecture

Coding Guide

API

Runbook

RCA

FAQ
```

支持：

评论。

版本。

审核。

---

# ⑧ Notification Center ⭐⭐⭐⭐☆

例如：

Agent：

完成：

任务。

通知：

```text
✓ Coding Agent

Finished

Refactor Login
```

Review：

通知。

Approval：

通知。

以后：

Webhook。

Slack。

邮件。

---

# UX

首页：

建议：

Dashboard。

```text
+-----------------------------------------------------------+

Projects

Recent Tasks

Running Agents

Pending Reviews

------------------------------------------------------------

My Tasks

Waiting Approval

Recent Sessions

------------------------------------------------------------

Knowledge

Trace

Notification

```

不是：

聊天。

---

# Project 页面

例如：

```text
+-----------------------------------------------------------+

Project

Monolith

-----------------------------------------------------------

Agents

Tasks

Knowledge

Workflow

Review

-----------------------------------------------------------

Activity

09:10

Coding Agent Finished

09:12

Review Created

09:15

Architect Approved

```

类似：

GitHub。

---

# Review UX

例如：

```text
Changed Files

UserService.java


Risk

Medium


Suggestion

Use transaction


Approve

Reject

Comment
```

类似：

GitHub PR。

---

# API

新增：

```text
GET /project

POST /project

GET /task

POST /task

GET /review

POST /review

GET /approval

POST /approval

GET /knowledge
```

---

# 数据模型

开始：

真正：

进入：

团队。

```text
Project

Agent

Task

Review

Approval

Knowledge

Notification
```

以后：

Organization。

直接：

升级。

---

# MVP 不做

不要：

* 多租户
* Billing
* Marketplace
* Cluster
* 企业权限
* SaaS
* 审计

这些：

P5。

---

# 完成标准

此时：

平台：

已经：

不是：

个人：

Agent。

而是：

团队：

Agent。

例如：

```text
Architect

↓

Create Task

↓

Coding Agent

↓

Generate Code

↓

Reviewer Agent

↓

Review

↓

Human

↓

Approve

↓

Merge
```

整个：

研发流程：

Agent：

参与。

---

# 我认为还应该增加一个核心能力：Activity Stream

这是 GitHub、Linear、Notion、Jira 做得都很好的地方，也是 Agent 平台特别需要的。

不要让用户四处找：

* 哪个 Agent 做了什么？
* 谁批准了？
* 哪个任务失败了？

建议引入统一的 **Activity Runtime（活动流）**（它属于 Collaboration 层，而不是 Agent Runtime）。

例如首页：

```text
09:10  Coding Agent completed Task #102
09:12  Reviewer Agent requested changes
09:15  Alice approved deployment
09:18  RCA Agent created incident report
09:20  Knowledge Base updated
```

所有 Agent、Workflow、Review、Approval、Knowledge 的事件都汇聚到一个活动流。

这样，团队每天打开平台时，不是先点 Chat，而是先看：

> **今天 Agent 团队发生了什么。**

我认为这会比传统聊天入口更符合团队协作产品的使用方式，也更容易向后续企业级平台演进。
