非常好，这一步其实是整个项目真正落地的开始。

前面我们设计的：

```text
core-kernel
core-agent
core-session
core-memory
core-tool
...
```

这些偏向**平台后端能力**。

而：

> CLI Agent 是第一个真实用户入口。

所以 Phase 0 不应该重新造一个 Agent，而应该：

> **打造一个极致的 CLI Client，连接 core-agent 后端 Runtime。**

类似：

```text
OpenCode CLI
        |
        |
     Agent API
        |
        |
  Agent Runtime
```

---

# Phase 0：CLI Agent（MVP）

## 产品定位

一句话：

> 一个面向开发者的终端 AI Agent 客户端，通过命令行驱动 core-agent Runtime 完成代码理解、修改、执行和任务闭环。

目标：

不是聊天机器人。

而是：

```
User Goal

↓

Agent Planning

↓

Tool Execution

↓

Code Change

↓

Verification

↓

Result
```

---

# 一、整体架构设计

最终结构：

```
                User

                 |
                 v

        +----------------+
        |   agent-cli    |
        | Rust CLI       |
        +----------------+

                 |
                 |
          Agent API Client

                 |
                 v


        +----------------+
        |  core-agent    |
        +----------------+

                 |
 ------------------------------------------------

 Session Runtime

 Context Runtime

 Model Runtime

 Tool Runtime

 Workspace Runtime

 Planning Runtime

 Execution Runtime

 Memory Runtime

 Event Runtime


 ------------------------------------------------

              core-kernel
```

---

核心原则：

CLI：

只负责：

* 用户交互
* 输入输出
* 本地环境采集
* API 调用

不要：

复制：

Agent Runtime。

---

# 二、项目结构设计

建议：

Monorepo：

```
agentos

├── core-kernel

├── core-agent

├── core-session

├── core-memory

├── core-tool

├── core-workspace


├── agent-cli        ⭐

└── agent-desktop
```

---

CLI：

```
agent-cli

├── src

│
├── command
│
├── terminal
│
├── api
│
├── session
│
├── renderer
│
├── config
│
└── main.rs
```

---

# 三、CLI 技术选型

Rust：

推荐：

```
clap
```

命令解析。

---

Terminal UI：

推荐：

```
ratatui
```

做：

* 状态栏
* 面板
* 进度

---

异步：

```
tokio
```

---

HTTP：

```
reqwest
```

---

序列化：

```
serde
```

---

颜色：

```
crossterm
```

---

# 四、CLI 命令设计

不要：

只有：

```bash
agent
```

聊天。

应该：

设计成：

开发工具。

---

## 1. 初始化项目

```bash
agent init
```

作用：

生成：

```
.agent/

├── config.yaml

├── context.yaml

└── memory/
```

---

## 2. 开始 Session

```bash
agent chat
```

进入：

交互模式。

例如：

```
$ agent chat


AgentOS

Project:
monolith


You:

分析这个项目架构


Agent:

正在分析...

✓ Scan files

✓ Build context

✓ Generate summary

```

---

## 3. 一次任务

```bash
agent run "fix login bug"
```

适合：

CI。

脚本。

---

## 4. 查看状态

```bash
agent status
```

输出：

```
Agent:

Running


Session:

abc123


Model:

Claude


Memory:

234 items
```

---

## 5. 查看历史

```bash
agent sessions
```

---

## 6. 配置

```bash
agent config
```

---

# 五、核心交互设计

## Chat Mode

这是 MVP 主入口。

界面：

```
╭──────────────────────────────╮
│ AgentOS                       │
│ Project: backend              │
│ Model: Claude                 │
╰──────────────────────────────╯


> 修复用户登录问题


Agent:

分析代码...

[1] 阅读 AuthController

[2] 检查 JWT


Thinking...


Tool:

read_file

src/Auth.java


Result:

发现 Token 过期问题


修改:

AuthService.java


Running tests...


✓ Test Passed


完成。


Changed:

3 files

+120
-30

```

---

# 六、核心流程设计

一次请求：

## Step 1 用户输入

```
User Prompt
```

CLI:

发送：

```json
{
 "sessionId":"xxx",
 "message":"fix bug"
}
```

---

## Step 2 Session 创建

core-session：

负责：

```
Create Session
```

返回：

```json
{
sessionId:"abc"
}
```

---

## Step 3 Agent Planning

core-agent：

调用：

```
Planner
```

生成：

```json
{
plan:[
 "inspect code",
 "modify file",
 "run test"
]
}
```

---

## Step 4 Tool 调用

例如：

```
read_file
```

CLI：

显示：

```
Reading:

src/Auth.java
```

---

## Step 5 Execution

core-execution：

执行。

---

## Step 6 Event Stream

重点。

CLI 不轮询。

应该：

实时：

订阅事件。

例如：

```
AgentStarted

↓

PlanCreated

↓

ToolStarted

↓

ToolFinished

↓

ExecutionFinished
```

技术：

MVP：

推荐：

SSE。

不要：

WebSocket。

原因：

CLI 单向接收为主。

---

# 七、CLI 与 core-agent API设计

建议：

REST + SSE。

---

## REST

创建 Session：

```
POST

/api/session
```

发送消息：

```
POST

/api/session/{id}/message
```

获取状态：

```
GET

/api/session/{id}
```

---

## SSE

事件：

```
GET

/api/session/{id}/events
```

返回：

```
event: tool_started

data:
{
tool:"read_file"
}
```

---

# 八、CLI 本地能力

MVP：

CLI 需要：

采集：

## Workspace

例如：

```
pwd

git status

project files
```

发送：

给：

core-workspace。

---

但是：

不要：

把所有代码上传。

默认：

Agent Server：

本地运行。

推荐模式：

第一版：

```
CLI

↓

localhost core-agent

↓

Local Workspace
```

类似：

Claude Code。

---

# 九、配置设计

.agent/config.yaml

```yaml
server:

  url: http://localhost:8080


model:

  provider: claude


workspace:

  root: .


memory:

  enabled: true
```

---

# 十、UX设计原则

## 1. 少打扰

不要：

每一步：

问确认。

默认：

自动。

危险操作：

确认。

例如：

删除：

```
⚠ Delete database?

[y/N]
```

---

## 2. 永远显示状态

用户必须知道：

Agent：

在干什么。

不要：

```
Thinking...
```

30秒。

应该：

```
Analyzing project...

Reading 24 files

Building context

Planning solution
```

---

## 3. Diff 优先

修改：

必须：

显示。

例如：

```
Changed:

AuthService.java

+ refreshToken()

- old logic
```

---

## 4. 可恢复

中断：

Ctrl+C。

再次：

```
agent resume
```

继续。

---

# 十一、MVP 必须实现

我认为：

只有这些：

## P0.1

CLI Shell

⭐⭐⭐⭐⭐

## P0.2

Session

⭐⭐⭐⭐⭐

## P0.3

Model 调用

⭐⭐⭐⭐⭐

## P0.4

Tool Calling

⭐⭐⭐⭐⭐

## P0.5

File Workspace

⭐⭐⭐⭐⭐

## P0.6

Execution Stream

⭐⭐⭐⭐⭐

## P0.7

Git Diff

⭐⭐⭐⭐⭐

## P0.8

Session Resume

⭐⭐⭐⭐

---

# 十二、MVP 不做

不要：

❌ GUI

❌ Workflow Designer

❌ Marketplace

❌ Multi Agent

❌ Plugin SDK

❌ Cloud Account

❌ Billing

❌ Team

---

# 十三、未来演进接口预留

虽然 MVP 简单：

但是接口必须有：

## Client Interface

```rust
trait AgentClient {

 send()

 stream()

 resume()

 cancel()

}
```

---

## Event Interface

```rust
trait EventHandler {

 on_event()

}
```

---

## Renderer Interface

未来：

Terminal:

```
TerminalRenderer
```

Desktop:

```
DesktopRenderer
```

统一：

```rust
trait Renderer
```

---

# 十四、第一版 UI 风格建议

结合你之前喜欢的：

黑金科技风。

Terminal：

不要花。

建议：

```
Black background

Gold highlight

White text

Gray metadata
```

类似：

Bloomberg Terminal + AI。

---

# 十五、Phase 0 完成标准

完成后：

用户可以：

```bash
agent init

cd project

agent chat


> 帮我分析这个项目

> 修改登录逻辑

> 运行测试

> 提交 git diff

```

整个闭环：

完成：

```
Human

↓

CLI

↓

core-agent

↓

Tool

↓

Workspace

↓

Execution

↓

Result
```

---

## 最重要的一点

**CLI Agent 不应该成为一个独立项目。**

它应该是：

> core-agent 的第一个官方 Client。

未来：

```
agent-cli

agent-desktop

agent-web

agent-ide
```

全部：

调用：

同一个：

```
core-agent API
```

这样你的路线才会自然：

```
CLI MVP
    |
    v
Desktop
    |
    v
Studio
    |
    v
Enterprise AgentOS
```

这条路线和 OpenCode 类似，但底层架构会更接近一个长期演进的 Agent Operating System。
