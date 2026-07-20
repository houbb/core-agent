结合你现在的 **core-agent 定位**（不是简单 Copilot，而是往 **Claude Code / OpenCode / Codex + 企业 Agent Runtime** 方向走），你现有 slash 命令已经覆盖了：

* 基础交互生命周期 ✅
* Session 管理 ✅
* Project 感知 ✅
* Plan/Review/Explain ✅
* Code 修改能力 ✅

但是如果目标是做一个 **Professional Agent Operating Runtime**，目前缺少几个关键能力层：

1. **上下文工程（Context Engineering）**
2. **Agent 调度控制（Agent Control Plane）**
3. **代码库理解（Code Intelligence）**
4. **工具执行控制（Tool Governance）**
5. **记忆系统（Memory System）**
6. **多 Agent 协作（Agent Society）**
7. **开发流程集成（Engineering Workflow）**

下面按照 Claude Code / OpenCode / Codex 对标分析。

---

# 一、现有命令能力矩阵

| 能力           | Claude Code | Codex CLI | OpenCode | core-agent |
| ------------ | ----------- | --------- | -------- | ---------- |
| help         | ✅           | ✅         | ✅        | ✅          |
| session      | ✅           | ✅         | ✅        | ✅          |
| project scan | ✅           | ✅         | ⚠️       | ✅          |
| plan         | ✅           | ✅         | ✅        | ✅          |
| review       | ✅           | ✅         | ✅        | ✅          |
| explain      | ✅           | ⚠️        | ⚠️       | ✅          |
| test         | ✅           | ✅         | ✅        | ✅          |
| fix          | ✅           | ✅         | ✅        | ✅          |
| refactor     | ✅           | ⚠️        | ⚠️       | ✅          |
| undo         | ✅           | ❌         | ⚠️       | ✅          |
| memory       | ⚠️          | ❌         | ⚠️       | ✅          |

基础能力已经接近。

但是缺少下面这些高级命令。

---

# 二、P0 必须补充命令（核心 Agent Runtime）

## 1. `/context`

### 用法

```
/context
```

或者

```
/context add src/main/java
```

### 作用

查看当前 Agent 上下文。

类似：

Claude Code:

```
context
```

Codex:

```
/status
```

但是你的设计应该更强。

输出：

```
Context Window

System:
  12k tokens

Project:
  35k tokens

Files:
  UserController.java
  AuthService.java

Memory:
  5k tokens

Tools:
  git
  shell
  filesystem

Remaining:
  120k tokens
```

### 为什么必须有？

Agent 最核心资源不是 CPU，而是：

> Context Budget

未来必须让用户知道：

* Agent 看到了什么
* 为什么这么回答
* 哪些信息被丢弃

这是专业 Agent 和聊天机器人区别。

---

# 2. `/compact`

### 用法

```
/compact
```

### 作用

压缩当前上下文。

类似：

Claude Code:

```
/compact
```

执行：

```
Before:

120k tokens


After:

20k tokens
```

生成：

```
Conversation Summary:

Goal:
Implement authentication

Completed:
- JWT service
- User entity

Pending:
- OAuth
- Test cases
```

### 为什么重要？

长时间 coding：

```
3小时
200轮对话
```

必然爆 context。

这是 Agent Runtime 必备能力。

---

# 3. `/resume`

### 用法

```
/resume session-id
```

恢复：

```
昨天的开发任务
```

Claude Code 有：

```
/resume
```

你的：

```
/sessions
```

只能查看。

缺少恢复。

---

# 4. `/checkpoint`

### 用法

```
/checkpoint save before-auth
```

恢复：

```
/checkpoint restore before-auth
```

虽然已有：

```
/undo
/redo
```

但是：

undo = 时间线

checkpoint = 状态快照

工程 Agent 必须有。

类似：

Git:

```
commit
```

Docker:

```
snapshot
```

---

# 三、代码智能层命令

## 5. `/search`

### 用法

```
/search UserService
```

功能：

代码搜索。

类似：

Claude:

```
grep
```

但是 Agent 原生化。

返回：

```
Found:

UserService.java

called by:

OrderService
PaymentService

dependency:

UserRepository
```

---

## 6. `/trace`

⭐⭐⭐⭐⭐

### 用法

```
/trace login
```

生成：

```
Request

Controller

 ↓

Service

 ↓

Repository

 ↓

Database

```

这是企业 Agent 最大价值。

特别适合你的背景：

* NOC
* RCA
* 链路分析

未来可以连接：

```
trace
log
metric
code
```

---

## 7. `/architecture`

### 用法

```
/architecture
```

输出：

```
System Architecture

api
 |
service
 |
domain
 |
infra
```

类似：

Claude Code:

```
explain project
```

但是独立。

---

# 四、工具控制层

## 8. `/permissions`

⭐⭐⭐⭐⭐

### 用法

```
/permissions
```

显示：

```
Filesystem

[x] read
[x] write

Shell

[x] execute

Network

[ ] enabled
```

Claude Code 有：

```
permissions
```

企业 Agent 必须。

---

## 9. `/approve`

### 用法

```
/approve
```

批准：

```
Execute:

rm file

git commit

npm install
```

你现在 approval 在 runtime 内部。

应该暴露。

---

## 10. `/deny`

对应：

```
拒绝执行
```

---

# 五、Memory 系统

你已有：

```
/memory
```

但是不够。

应该拆：

---

## 11. `/memory-show`

```
/memory show
```

查看：

```
Project Memory

Architecture:
SpringBoot

Preference:
SQLite

Rules:
No Redis
```

---

## 12. `/memory-save`

```
/memory save "Use SQLite first"
```

主动写入。

---

## 13. `/memory-clear`

```
/memory clear
```

清理。

---

# 六、多 Agent 能力（你的 Agent Society Layer）

未来非常关键。

## 14. `/agents`

查看 Agent。

```
/agents
```

输出：

```
Available Agents


planner

coder

reviewer

tester

security
```

---

## 15. `/delegate`

⭐⭐⭐⭐⭐

用法：

```
/delegate security review
```

产生：

```
Main Agent

 |
 +-- Security Agent

 +-- Test Agent

```

这是未来 Claude Code 最大方向。

---

## 16. `/team`

```
/team start
```

创建 Agent Team。

类似：

Claude Code Agent Teams。

---

# 七、开发流程命令

## 17. `/git`

统一 git。

```
/git status

/git diff

/git branch
```

不要依赖 shell。

---

## 18. `/commit`

已有。

建议升级：

```
/commit auto
```

生成：

```
feat(auth):

- add jwt
- add refresh token
```

---

## 19. `/pr`

已有。

增加：

```
/pr review
```

自动检查：

* tests
* security
* breaking change

---

# 八、企业级能力（你的差异化）

这里 Claude Code / Codex 目前弱。

## 20. `/audit`

⭐⭐⭐⭐⭐

```
/audit
```

输出：

```
Agent Actions

10:02 read User.java

10:05 modified Auth.java

10:10 executed test
```

和你的：

```
core-audit
```

打通。

---

## 21. `/policy`

```
/policy
```

显示：

```
Organization Rules

禁止:
rm -rf

必须:
review before merge

```

对应企业治理。

---

## 22. `/cost`

```
/cost
```

AI 消耗。

输出：

```
Today:

GPT-5:
120k tokens

Cost:
$1.2
```

对应：

core-ai
core-billing

---

# 九、最终建议 Slash 命令体系

我建议 core-agent 最终：

```
/
├── help
├── new
├── clear
├── exit
├── profile
│
├── context ⭐
├── compact ⭐
├── resume ⭐
├── checkpoint ⭐
│
├── project
├── architecture ⭐
├── search ⭐
├── trace ⭐
│
├── tasks
├── sessions
├── history
├── status
│
├── tools
├── permissions ⭐
├── approve ⭐
├── deny ⭐
│
├── memory
├── memory-save
├── memory-show
├── memory-clear
│
├── agents ⭐
├── delegate ⭐
├── team ⭐
│
├── plan
├── review
├── explain
├── test
├── fix
├── refactor
│
├── commit
├── pr
├── git ⭐
│
├── audit ⭐
├── policy ⭐
├── cost ⭐
```

---

# 优先级排序

如果只做 MVP，我建议新增：

## Phase 0.5

必须：

```
/context
/compact
/resume
/checkpoint
/search
```

## Phase 1 Professional Agent

加入：

```
/trace
/architecture
/permissions
/approve
/memory-save
```

## Phase 2 Agent Society

加入：

```
/agents
/delegate
/team
```

## Phase 3 Enterprise Agent OS

加入：

```
/audit
/policy
/cost
```

---

结合你前面规划的：

```
core-agent-planner
core-agent-task
core-agent-question
core-agent-todo
core-agent-reflection
Agent Society Layer
Agent Cognitive Layer
```

我认为 **slash 命令应该不是简单 CLI 快捷键，而应该成为 Agent OS 的控制面（Control Plane）**。

你现在的设计大概是 **Claude Code 80% 能力**，补上 Context / Memory / Agent Society / Governance 后，会更接近一个真正的 **Enterprise Agent Runtime**。
