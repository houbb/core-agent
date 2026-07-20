# 目标

为了方便统一管理+拓展。期望我们的 slash 在设计的时候，使用接口+插件式+全生命周期管理。terminal 和 desktop 只是不同的入口，底层都是同一套 slash 实现。

一步步来，先给出下面的详细设计+交互设计+UX+注意点。  

特别的，比如 compact 压缩算法其实影响很大，可以让其参考优秀的实现策略，而不是全部从零开始。   

Phase 0.5

必须：

/context
/compact
/resume
/checkpoint

# 核心目标 2

将已有的 slash 命令，统一纳入到我们最新的 slash 标准之中，方便统一的管理+拓展。

# 设计

# Phase 0.5：Core-Agent Slash Runtime Foundation

目标：

> 建立统一 Slash Command Runtime，让 CLI Terminal、Desktop、Web、API 等入口共享同一套命令系统。

这一阶段不是简单增加 4 个命令，而是建立未来：

* `/agents`
* `/delegate`
* `/audit`
* `/policy`
* `/memory`
* `/workflow`

等所有 Agent 控制能力的基础设施。

---

# 1. 总体设计目标

## 当前问题

很多 Agent 产品：

```
Claude Code CLI
        |
        |
     Command Parser
        |
     Handler
```

简单直接。

但是长期会遇到：

```
大量命令

/context
/compact
/resume
/checkpoint
/memory
/agent
/team
/audit
...
```

导致：

* CLI 有一套实现
* Desktop 又复制一套
* Web 再复制
* 权限无法统一
* 生命周期无法管理
* 插件无法扩展

---

# 2. Core-Agent Slash Runtime 架构

建议独立：

```
core-agent

├── slash-runtime
│
│
├── command-registry
│
├── command-parser
│
├── command-router
│
├── command-executor
│
├── command-lifecycle
│
├── command-plugin
│
└── command-sdk
```

整体：

```
              Terminal UI
                  |
              Desktop UI
                  |
              Web UI
                  |
              API
                  |
                  v

          Slash Runtime Layer

                  |
      +-----------+------------+
      |           |            |
 Registry    Router       Lifecycle

      |
      |
 Command Plugin


      |
      |
 Agent Runtime
```

核心原则：

> Slash 是 Agent Runtime 的控制协议，而不是 CLI 命令。

---

# 3. Slash Command 生命周期设计

每个 slash command 都不是一个函数。

而是一个生命周期对象。

## Command Interface

类似：

```java
interface SlashCommand {


    CommandMetadata metadata();


    CommandPermission permission();


    CommandResult execute(
        CommandContext context
    );


    default void validate(
        CommandArguments args
    );


    default void onRegister();


    default void onDestroy();

}
```

---

# 4. Command Metadata

例如：

```json
{
"name":"compact",

"displayName":"Context Compact",

"description":"Compress current conversation context",

"category":"context",

"route":"runtime",

"version":"1.0",

"readonly":false,

"async":true,

"permission":
{
 "level":"agent"
}

}
```

---

# 5. Command 分类体系

不要只有：

```
Entry
Runtime
Agent
```

建议升级：

```
SlashCategory


SYSTEM

SESSION

CONTEXT

PROJECT

MEMORY

AGENT

TEAM

GOVERNANCE

DEVELOPER

ENTERPRISE

```

例如：

```
/exit

category:
SYSTEM


/context

category:
CONTEXT


/delegate

category:
TEAM

```

---

# 6. Command Plugin 机制

未来：

```
core-agent

内置:

context-plugin
session-plugin
memory-plugin


第三方:

java-agent-plugin
github-plugin
jira-plugin
k8s-plugin
```

加载：

```
PluginManager

        |
        |
 Scan

        |
        |
 Register Slash


        |
        |
 Runtime
```

---

# 7. Phase 0.5 四个核心 Slash

---

# Command 1

# `/context`

## 定位

Context Intelligence Runtime

不是简单查看 token。

它负责：

* 当前上下文
* Context Window
* Token Budget
* 文件上下文
* Memory
* Tool Context
* Compression 状态

---

# 交互设计

用户：

```
/context
```

输出：

```
╭────────────────────────────╮
│ Agent Context Status       │
╰────────────────────────────╯


Model

GPT-5

Window

128k tokens


Current Usage

86k


Available

42k


Sources:


Conversation

32k


Files

42k

  src/Auth.java
  src/User.java


Memory

8k


Tools

4k



Compression

Not applied

```

---

# Desktop UX

设计成：

右侧 Context Panel

```
--------------------------------

Chat


                Context


                86k /128k


                Files

                Memory

                Tools


--------------------------------
```

---

# 内部数据模型

```rust
struct AgentContext {

conversation_tokens:u32,

file_context:

Vec<FileReference>,


memory_context:

MemorySnapshot,


tool_context:

ToolSnapshot,


budget:

ContextBudget

}

```

---

# 注意点

不要让 `/context`

直接计算 token。

应该调用：

```
ContextManager


```

统一管理。

---

# Command 2

# `/compact`

这是 Phase 0.5 最重要的命令。

原因：

Agent 长会话一定遇到：

```
Context Overflow
```

---

# 不建议自己设计算法

参考：

## Claude Code compact 思路

核心：

```
Conversation
      |
Important Information Extraction
      |
Summary
      |
Replace Old Messages
```

不是简单：

```
truncate
```

---

# 推荐架构

```
Compact Engine


       |
       |

Analyzer


       |

Importance Scorer


       |

Summarizer


       |

Context Rebuilder

```

---

# Compact Pipeline

## Step 1

分析历史消息

输入：

```
User
Assistant
Tool
Files
Actions

```

---

## Step 2

分类

类似：

```
KEEP


- user goal
- constraints
- decisions
- architecture
- unfinished task


DROP


- greetings
- repeated explanation
- temporary logs


SUMMARIZE


- long code output
- tool result

```

---

## Step 3

生成 Compact Memory

例如：

原始：

```
500 messages
120k tokens

```

生成：

```
8k tokens


Project Goal:

Implement OAuth


Completed:

JWT
Login API


Current:

OAuth callback


Constraints:

No Redis

```

---

# Compact Algorithm 推荐

不要一次总结。

采用：

## Hierarchical Summarization

类似：

```
Messages


100 messages

      |
      v

Chunk Summary


      |
      v


Session Summary


      |
      v


Global Context

```

---

# 数据结构

```rust
struct CompactSnapshot {


original_range:

MessageRange,


summary:

String,


importance:

Score,


created_at:

Time

}

```

---

# UX

执行：

```
/compact
```

显示：

```
Analyzing conversation...


Before:

118k tokens


After:

18k tokens


Saved:

100k


Checkpoint:

compact-001

```

---

# 高级参数

未来：

```
/compact aggressive

/compact safe

/compact files

```

---

# Command 3

# `/resume`

## 定位

Session Recovery Runtime

---

当前：

```
/sessions

查看

```

增加：

```
/resume xxx

恢复
```

---

# 数据模型

```
AgentSession


id

workspace

messages

context_snapshot

memory_snapshot

checkpoint

created_at

```

---

# UX

```
/resume auth-task


Loading...


Session:

Auth Refactor


Last action:

Added JWT Service


Pending:

OAuth


Restore context?


Y/n

```

---

# 注意

resume 不应该恢复全部历史。

应该：

```
Session Metadata

+

Context Snapshot

+

Relevant Memory

```

否则又爆 Context。

---

# Command 4

# `/checkpoint`

## 定位

Agent Transaction Snapshot

---

区别：

undo:

```
撤销最近操作

```

checkpoint:

```
保存一个可恢复状态
```

---

# 架构

```
Checkpoint Manager


       |

File Snapshot


       |

Context Snapshot


       |

Session Snapshot


       |

Git Snapshot

```

---

# 数据模型

```rust
struct Checkpoint {


id:String,


name:String,


files:

Vec<FileChange>,


context:

ContextSnapshot,


created:

Timestamp


}

```

---

# UX

创建：

```
/checkpoint save before-refactor


✓ checkpoint created


ID:

cp-20260720-001

```

恢复：

```
/checkpoint restore cp-001


Warning:

Files will change


Continue?

```

---

# 四个命令关系

非常重要：

```
                 Session


                    |
                    |

             Context Runtime

                    |
      +-------------+-------------+

      |             |             |

 /context     /compact      /checkpoint


                    |

              /resume


```

---

# 生命周期管理

每个命令：

```
REGISTER

    |
    |
VALIDATE

    |
    |
EXECUTE

    |
    |
AUDIT

    |
    |
METRIC

    |
    |
UNREGISTER

```

---

# 必须预留接口

## Command Observer

未来：

```
Audit Plugin


监听：

command_start

command_success

command_failed

```

---

## Command Metrics

记录：

```
/compact

count

avg_time

token_saved

success_rate

```

---

# Phase 0.5 完成标准

完成后：

```
core-agent

拥有：

✓ Slash Runtime

✓ Plugin Interface

✓ Lifecycle Management

✓ Context Runtime

✓ Compact Engine

✓ Session Resume

✓ Checkpoint System


```

下一阶段 Phase 1：

建议进入：

```
/search
/trace
/architecture
/permissions
/approve
```

因为这部分会建立 **Code Intelligence Runtime + Tool Governance Runtime**，也是 Claude Code / Codex 真正拉开差距的地方。

