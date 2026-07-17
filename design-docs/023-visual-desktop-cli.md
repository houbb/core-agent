这里我想先修正一个我前面的建议。

> **Phase 2 不应该叫 Desktop Agent。**

因为一旦叫 **Desktop Agent**，大家很容易理解成：

> "把 CLI 套一层 GUI。"

这是很多开源项目（包括不少 OpenCode Fork）最后都会走偏的地方。

我更推荐叫：

> **Phase 2：Desktop Workspace（Developer Workspace）⭐⭐⭐⭐⭐**

为什么？

因为 Desktop 的目标不是聊天。

而是：

> **把 CLI 无法高效展示的信息，以 Workspace 的方式呈现出来。**

所以 Desktop 不是 CLI 的替代品。

而是：

CLI + Visualization。

---

# Phase 2：Desktop Workspace

一句话定位

> **一个围绕 Agent 的开发工作台，而不是聊天软件。**

它不是：

```text
ChatGPT Desktop
```

不是：

```text
Claude Desktop
```

更不是：

```text
Electron Chat
```

它应该像：

```text
VSCode
        +

GitHub Desktop
        +

Raycast
        +

Activity Monitor
```

全部融合。

---

# 第一性原理

CLI 最大的问题：

不是能力。

而是：

信息密度。

例如：

CLI：

```bash
Reading...

Thinking...

Planning...

Done
```

用户：

不知道：

Agent：

到底：

读了哪些文件？

为什么：

修改这里？

Memory：

更新了什么？

Tool：

调用了什么？

所以：

Desktop：

最大的价值：

不是聊天。

而是：

**可视化 Runtime。**

---

# Desktop 架构

建议：

```text
                 Desktop

             Vue3 + TraUI2

                    │

────────────────────────────────

      Desktop Controller

────────────────────────────────

Chat

Workspace

Trace

Memory

Tool

Git

Project

Settings

────────────────────────────────

Agent API

────────────────────────────────

core-agent
```

注意：

Desktop：

不要：

业务。

所有：

Agent：

全部：

走：

API。

---

# 技术架构

推荐：

```
Tauri 2

↓

Rust Bridge

↓

Vue3

↓

TraUI2
```

为什么：

因为：

以后：

CLI：

Desktop：

共享：

Rust。

---

# 页面设计

我建议：

第一版：

不要：

很多页面。

只要：

8 个。

---

# ① Chat Workspace ⭐⭐⭐⭐⭐

这是：

首页。

但是：

不是：

聊天窗口。

而是：

Agent Console。

布局：

```text
+----------------------------------------------------------+

Project

Monolith


Agent

Architect


Model

Claude

-----------------------------------------------------------

Conversation


User:

分析项目


Agent:

正在扫描...


✓ Read README

✓ Index Maven

✓ Build Graph


-----------------------------------------------------------

Input

>
```

注意：

Chat：

只是：

中间。

不是：

全部。

---

# ② Project Explorer ⭐⭐⭐⭐⭐

Professional：

必须。

类似：

IDE。

但是：

不要：

编辑。

展示：

Project。

例如：

```text
Project

├── backend

├── frontend

├── docs

├── scripts

└── README
```

点击：

Agent：

知道：

上下文。

---

# ③ Changes Workspace ⭐⭐⭐⭐⭐

这是：

OpenCode：

没有：

GUI：

最大的缺失。

例如：

Agent：

修改：

5 个文件。

不要：

Terminal：

输出。

应该：

显示：

```text
Changed Files


✓ UserService.java

✓ LoginController.java

✓ README.md
```

点击：

Diff。

类似：

GitHub。

---

# ④ Trace Explorer ⭐⭐⭐⭐⭐

我认为：

这是：

整个：

Desktop：

最重要：

页面。

因为：

Agent：

不是：

黑盒。

例如：

```text
User Prompt

↓

Planner

↓

Tool

↓

LLM

↓

Memory

↓

Tool

↓

Response
```

每一步：

可展开。

例如：

```text
Planner

↓

Generated

3 Tasks
```

点击：

展开：

Prompt。

Token。

耗时。

---

# ⑤ Tool Explorer ⭐⭐⭐⭐☆

对应：

P3。

例如：

```text
Filesystem

Running


Git

Idle


Shell

Running


Database

Disabled
```

以后：

Extension。

也：

这里。

---

# ⑥ Memory Explorer ⭐⭐⭐⭐⭐

企业：

必须。

否则：

用户：

不知道：

AI：

记住：

什么。

例如：

```text
Project Memory


Spring Boot

DDD

Hexagonal


User Preference


Use Rust

Use Vue3
```

支持：

删除。

固定。

编辑。

---

# ⑦ Session Explorer ⭐⭐⭐⭐☆

例如：

```text
Today


Fix Login

Review PR

Refactor Auth

```

点击：

恢复：

Session。

---

# ⑧ Settings ⭐⭐⭐⭐☆

统一：

```text
Model

Server

Workspace

Theme

Shortcut

Extension
```

---

# UX

第一版：

建议：

类似：

JetBrains。

布局：

```text
+------------------------------------------------------------+

Sidebar

Explorer

Session

Memory

Trace

Settings

-------------------------------------------------------------

Center

Conversation

-------------------------------------------------------------

Bottom

Execution

Tool

Log

-------------------------------------------------------------

Status

Claude

Session

Token

Latency

```

不要：

很多：

浮窗。

---

# 与 core-agent 通信

Desktop：

全部：

HTTP。

例如：

```
GET /session

POST /chat

GET /trace

GET /memory

GET /project

GET /tool

GET /event
```

Event：

继续：

SSE。

不要：

WebSocket。

---

# Desktop Runtime

建议：

Desktop：

增加：

Controller。

例如：

```rust
DesktopController

ChatController

ProjectController

TraceController

MemoryController
```

不要：

Vue：

直接：

API。

---

# 本地状态

建议：

只保存：

UI。

例如：

SQLite：

```text
window

layout

recent project

theme

shortcut
```

不要：

保存：

Memory。

Memory：

还是：

core-agent。

---

# MVP 不做

不要：

* ❌ Monaco 编辑器
* ❌ Workflow Canvas
* ❌ 多窗口
* ❌ 插件市场
* ❌ 多 Agent 协同
* ❌ 云同步
* ❌ 企业管理

---

# Phase 2 新增 API

建议：

```text
GET  /project/tree
GET  /project/changes
GET  /trace/{session}
GET  /memory/list
GET  /tool/status
GET  /session/list
```

全部：

core-agent。

---

# Phase 2 完成标准

用户打开 Desktop 后，不需要阅读日志，就能直观看到 Agent 的完整工作过程。

例如：

```text
┌────────────────────────────────────────────────────┐
│ Project: Monolith                                  │
│ Profile: Architect                                 │
├────────────────────────────────────────────────────┤
│ Chat                │ Trace                         │
│---------------------│-------------------------------│
│ 帮我重构登录模块      │ ✓ Scan Project               │
│                     │ ✓ Build Context              │
│ Agent 正在执行...    │ ✓ Plan Tasks                │
│                     │ ✓ Read 18 Files              │
│                     │ ✓ Modify 4 Files             │
├─────────────────────┴───────────────────────────────┤
│ Changes                                             │
│ • AuthService.java                                 │
│ • LoginController.java                             │
├────────────────────────────────────────────────────┤
│ Status: Running | Model: Claude | Cost: $0.03      │
└────────────────────────────────────────────────────┘
```

---

# 我还会再做一个关键升级（区别于 OpenCode、Claude Code）

我建议 **Desktop 从 Phase 2 开始就不要做传统的页面导航，而是采用「Workspace（工作区）」概念。**

也就是说：

不是：

```text
Chat
Memory
Trace
Settings
```

而是：

```text
Workspace
│
├── Chat Panel
├── Trace Panel
├── Memory Panel
├── Tool Panel
├── Changes Panel
└── Terminal Panel
```

用户可以像 IDE 一样自由拖拽、停靠、保存布局。

这样，CLI、Desktop、未来的 Web Studio 将共享同一套 **Workspace 模型**，以后增加 Workflow、Multi-Agent、Marketplace 等能力时，只需要增加新的 Panel，而不是重构整个界面。这种 Workspace 化的设计，会比传统聊天软件更适合一个长期发展的 AgentOS。
