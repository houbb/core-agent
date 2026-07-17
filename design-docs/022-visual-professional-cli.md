我认为 **Phase 1 不应该理解为「CLI 功能增强」**。

很多项目（包括 OpenCode 的很多 Fork）都会走错路：

```text
MVP

↓

加几十个命令

↓

加几十个参数

↓

CLI越来越复杂
```

最后：

```bash
agent \
--model xxx \
--memory \
--approval auto \
--temperature 0.1 \
--tool xxx \
...
```

用户根本记不住。

---

## 我建议 Phase 1 的定位

> **Professional CLI = 真正可以每天工作的 AI Terminal。**

它不是增加命令。

而是增加：

**Developer Experience（DX）**

也就是：

CLI 开始变成：

> 一个 AI IDE（Terminal 版）

---

# Phase 1：Professional CLI Agent ⭐⭐⭐⭐⭐

一句话定位：

> **让 CLI 成为开发者每天打开的第一个工具。**

---

# Phase 1 的核心目标

不是：

```text
聊天
```

而是：

```text
理解整个工程

↓

理解 Git

↓

理解用户

↓

理解历史

↓

理解团队规范
```

Agent 开始真正：

"懂项目"

---

# Phase 1 新增 Runtime

不是新增 Runtime。

而是在已有 Runtime 上增加：

```text
Project Runtime

Command Runtime

Profile Runtime

Review Runtime

Terminal Runtime
```

注意：

这些都属于：

Agent Runtime。

不是 Platform。

---

# 整体架构

```text
               agent-cli

                     │

────────────────────────────────────

Project Runtime

Command Runtime

Profile Runtime

Review Runtime

Terminal Runtime

────────────────────────────────────

Session

Context

Tool

Workspace

Planning

Execution

Memory

────────────────────────────────────

Kernel
```

---

# P1.1 Project Runtime ⭐⭐⭐⭐⭐

这是：

Professional CLI

最重要。

一句话：

> Agent 不再理解文件。

开始理解：

整个项目。

---

例如：

进入：

```bash
cd monolith
```

Agent：

自动：

扫描：

```text
pom.xml

package.json

Cargo.toml

.git

README

Dockerfile
```

然后：

建立：

Project Graph。

---

例如：

```text
Project

├── Language

├── Framework

├── Modules

├── Build

├── Git

├── Test

├── Docs

└── Architecture
```

以后：

所有：

Prompt：

不用：

重复。

---

UX：

第一次：

进入：

```bash
agent
```

自动：

```text
✓ Detect Spring Boot

✓ Detect Maven

✓ Detect Vue3

✓ Detect PostgreSQL

✓ Build Project Context

Done.
```

用户：

很舒服。

---

# P1.2 Command Runtime ⭐⭐⭐⭐⭐

很多 AI CLI：

只有：

聊天。

这是错误。

Professional CLI：

应该：

有：

命令体系。

例如：

```bash
/explain

/review

/test

/plan

/fix

/refactor

/commit

/pr
```

以后：

插件：

也是：

命令。

---

例如：

```bash
/review AuthController.java
```

Agent：

知道：

Review。

不是：

聊天。

---

Command：

建议：

统一：

```rust
trait Command {

execute()

complete()

help()

}
```

以后：

Marketplace：

直接：

注册。

---

# P1.3 Profile Runtime ⭐⭐⭐⭐⭐

这是：

OpenCode

没有做好。

Agent：

应该：

支持：

人格。

例如：

```bash
/profile architect
```

以后：

所有：

Prompt：

切换。

---

例如：

Architect：

```text
Thinking

Architecture

DDD

Scalability
```

Coder：

```text
Implement

Fix

Optimize
```

Reviewer：

```text
Bug

Security

Style
```

SRE：

```text
Observability

Logs

Metrics
```

你的：

RCA：

以后：

直接：

一个：

Profile。

---

UX：

左上角：

```text
Profile

Architect
```

随时：

切换。

---

# P1.4 Review Runtime ⭐⭐⭐⭐⭐

Professional：

必须：

Review。

不是：

聊天。

例如：

```bash
/review
```

Agent：

自动：

读取：

```bash
git diff
```

输出：

```text
Security

Performance

Maintainability

Risk

Suggestion
```

以后：

CI：

直接：

调用。

---

Review：

接口：

```rust
trait Reviewer {

review()

}
```

以后：

多个：

Reviewer。

---

# P1.5 Terminal Runtime ⭐⭐⭐⭐⭐

这个：

很多：

Agent：

没有。

Terminal：

不是：

stdout。

而是：

Runtime。

负责：

```text
Command History

Autocomplete

Selection

Clipboard

Keyboard

Shortcut
```

以后：

Desktop：

直接：

复用。

---

例如：

支持：

```bash
Ctrl+R
```

搜索：

历史。

---

支持：

```bash
Tab
```

补全：

命令。

---

支持：

```bash
↑
```

恢复：

Prompt。

---

# P1.6 Git Runtime ⭐⭐⭐⭐⭐

MVP：

只是：

Diff。

Professional：

开始：

理解：

Git。

例如：

```text
Branch

Commit

Diff

Conflict

History

Author
```

Agent：

以后：

回答：

```text
为什么：

这个模块：

这么设计？
```

它：

可以：

看：

Git。

---

例如：

```bash
/history UserService
```

Agent：

分析：

过去：

20次：

Commit。

---

# P1.7 Project Memory ⭐⭐⭐⭐⭐

注意：

不是：

Memory Runtime。

而是：

Project Layer。

例如：

Agent：

以后：

记住：

```text
Coding Style

Architecture

Convention

Naming

Dependency
```

以后：

新的：

Session。

自动：

加载。

---

例如：

```text
Project Memory

Spring Boot

DDD

Hexagonal

JUnit5

```

---

# P1.8 Task Runtime ⭐⭐⭐⭐☆

Professional：

开始：

管理：

Task。

例如：

```bash
/tasks
```

输出：

```text
#12

Refactor Login

Running

80%
```

以后：

Resume。

---

# CLI UX（Professional）

我建议：

不要：

OpenCode：

那种：

只有：

聊天。

建议：

变成：

```text
╭────────────────────────────────────────────╮
│ Project : Monolith                         │
│ Profile : Architect                        │
│ Model   : Claude                           │
│ Branch  : feature/auth                     │
╰────────────────────────────────────────────╯

>

```

下面：

Agent：

执行：

实时：

状态。

右边：

以后：

Desktop：

就是：

Sidebar。

---

# 新增命令

建议：

```bash
/project

/profile

/tasks

/history

/review

/plan

/explain

/config

/status

/tools

/memory
```

全部：

统一。

---

# MVP 不做

不要：

❌ Workflow Canvas

❌ GUI

❌ Marketplace

❌ Multi Agent

❌ Team

---

# 与 core-agent 对接

Phase 1 开始：

建议：

增加：

Project API。

例如：

```text
POST /project/index

GET /project/context

GET /project/memory

POST /project/review

GET /project/history
```

CLI：

越来越轻。

所有：

智能：

放：

core-agent。

---

# Phase 1 完成后的效果

此时，CLI 已经不是一个聊天工具，而是一个真正的开发工作台。开发者打开终端后，可以在一个连续的工作流中完成项目理解、架构分析、代码修改、Review 和 Git 操作。

典型的一天可能是：

```bash
$ agent

✓ Project indexed
✓ Git repository detected
✓ Project memory loaded

> /review

→ Reviewing current git diff...
✓ 3 issues found

> /plan "重构认证模块"

→ Architecture plan generated.

> /fix

→ Applying changes...
✓ 5 files modified

> /test

→ Running Maven tests...
✓ All tests passed

> /commit

→ Commit message generated.
```

整个过程中，**CLI 只是交互入口**，真正的 Session、Memory、Planning、Execution、Project Intelligence 等能力，都来自你前面规划的 `core-agent` 各 Runtime。这样，未来迁移到 Tauri Desktop、Web Studio 甚至 IDE 插件时，只需要替换前端交互层，而无需重写任何核心逻辑。
