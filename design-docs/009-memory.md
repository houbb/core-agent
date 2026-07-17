我认为 **Memory Runtime** 是整个 Agent 平台最容易被设计错的模块。

很多项目一开始就做：

```text
Memory
↓

Vector DB
↓

Embedding
↓

RAG
```

然后认为这就是 Memory。

**这是一个误区。**

> **Embedding 不是 Memory。RAG 也不是 Memory。**

真正的 Memory 应该回答的是：

> **Agent 应该记住什么？什么时候记住？什么时候忘记？什么时候取出来？**

所以，在整个 Runtime 里面：

**Memory 是 Knowledge Management（知识管理），不是 Vector Search（向量检索）。**

---

# Phase 8：Memory Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责 Agent 的长期记忆管理，包括记忆的产生、存储、检索、演化和遗忘。**

Memory Runtime：

不知道：

* Tool
* Planner
* Workflow

它只负责：

```text
Remember

↓

Store

↓

Recall

↓

Update

↓

Forget
```

---

# 为什么放 P8？

因为现在已经拥有：

```text
Session
↓

Context
↓

Model
↓

Tool
↓

Workspace
↓

Planning
↓

Execution
↓

Agent
```

Agent：

终于：

能够：

长期工作。

所以：

现在：

才需要：

Long Memory。

---

# 第一性原理

Memory：

不是：

聊天记录。

真正：

应该：

```text
Experience

↓

Memory

↓

Knowledge

↓

Wisdom
```

例如：

Agent：

修改：

Java。

成功。

以后：

应该：

记住。

而不是：

聊天。

---

# Runtime职责

Memory：

负责：

```text
产生记忆

↓

分类

↓

存储

↓

检索

↓

更新

↓

遗忘
```

不要：

Embedding。

不要：

Vector。

以后。

---

# Runtime架构

建议：

```text
Memory Runtime

│

├── MemoryManager

├── MemoryStore

├── MemoryIndexer

├── MemoryRetriever

├── MemoryClassifier

├── MemoryPolicy

├── MemorySnapshot

├── MemoryLifecycle

└── MemoryObserver
```

---

# 一、MemoryManager

唯一：

入口。

例如：

```rust
remember()

recall()

forget()

update()
```

其它：

Runtime：

全部：

调用：

Manager。

---

# 二、MemoryStore

真正：

保存：

Memory。

第一版：

SQLite。

以后：

支持：

```text
SQLite

Postgres

Vector DB

Cloud
```

统一：

Store。

---

# 三、MemoryIndexer

不要：

Vector。

第一版：

就是：

Index。

例如：

```text
Type

Tag

Workspace

Goal

Time
```

以后：

再：

Embedding。

---

# 四、MemoryRetriever

真正：

Recall。

例如：

```text
Query

↓

Filter

↓

Rank

↓

Return
```

不要：

Context：

自己：

找。

统一：

Retriever。

---

# 五、MemoryClassifier

第一版：

就要。

因为：

不是：

所有：

东西：

都应该：

记住。

例如：

```text
Conversation

×

---------

Bug Fix

✓

---------

User Preference

✓

---------

Temporary Log

×

---------

Coding Style

✓
```

Memory：

需要：

分类。

---

建议：

MemoryType：

```text
Experience

Knowledge

Preference

Fact

Workspace

Skill

Rule

Observation
```

以后：

很好：

扩展。

---

# 六、MemoryPolicy

企业：

必须。

例如：

```text
Retention

90 Days

---------

Sensitive

Never Save

---------

Workspace

Private
```

以后：

GDPR。

企业。

全部：

这里。

---

# 七、MemorySnapshot

第一版：

预留。

例如：

```text
Memory

↓

Snapshot

↓

Restore
```

以后：

Debug。

Replay。

---

# 八、MemoryLifecycle

生命周期：

建议：

```text
Created

↓

Verified

↓

Indexed

↓

Recalled

↓

Updated

↓

Archived

↓

Forgotten
```

不要：

只有：

Insert。

---

# 九、MemoryObserver

第一版：

预留。

例如：

```text
Remember

↓

Recall

↓

Forget

↓

Archive
```

以后：

Audit。

Trace。

Analytics。

---

# Memory对象

建议：

```text
Memory

├── Identity

├── Type

├── Content

├── Metadata

├── Source

├── Importance

├── Confidence

└── State
```

不要：

String。

---

# MemorySource

建议：

```text
Conversation

Workspace

Tool

Execution

Agent

User

Plugin
```

以后：

Debug。

很好。

---

# MemoryImportance（重点）

建议：

增加：

```text
Critical

High

Medium

Low

Temporary
```

以后：

Forget。

直接：

支持。

---

# API设计

Manager：

```rust
remember()

recall()

forget()

update()
```

Retriever：

```rust
search()

filter()

rank()
```

Classifier：

```rust
classify()
```

Snapshot：

```rust
save()

restore()
```

Policy：

```rust
check()
```

---

# 生命周期

建议：

```text
Event

↓

Classify

↓

Store

↓

Index

↓

Recall

↓

Update

↓

Forget
```

---

# SQLite

建议：

第一版：

```text
memory

memory_index

memory_snapshot

memory_policy

memory_tag
```

五张。

---

# UX设计

左边：

增加：

```text
Memory

────────────

Experience

Knowledge

Preference

Skill

Workspace
```

点击：

Experience：

例如：

```text
Memory

────────────

修复 NullPointer

Importance

High

Workspace

Monolith
```

下面：

```text
Tags

────────────

Java

Spring

Bug
```

Agent：

知道：

自己：

记住：

什么。

---

增加：

Memory Timeline：

```text
Yesterday

Bug Fix

↓

Today

Coding Style

↓

Today

User Preference
```

以后：

很好：

Review。

---

增加：

Memory Inspector：

例如：

```text
Reason

────────────

Remembered

Because

Successful Task
```

让用户：

知道：

为什么：

Agent：

记住：

它。

---

增加：

Recall：

例如：

```text
Query

↓

Matched

12 Memories

↓

Used

3 Memories
```

透明。

---

# MVP 不做什么

不要：

* ❌ Embedding
* ❌ Vector Database
* ❌ Semantic Search
* ❌ Graph Memory
* ❌ AI Summary
* ❌ Auto Reflection
* ❌ Memory Compression
* ❌ Long Context Optimization
* ❌ Memory Sharing

以后。

---

# 扩展点（第一版就预留）

```text
Memory Runtime
│
├── MemoryStore          // SQLite、PG、Vector...
├── MemoryIndexer        // 索引
├── MemoryRetriever      // 检索
├── MemoryClassifier     // 分类
├── MemoryPolicy         // 企业策略
├── MemorySnapshotStore  // 快照
├── MemoryObserver       // Trace、Audit
├── MemoryInterceptor    // Hook
└── MemoryCompressor     // 后续压缩
```

---

# 企业版演进路线

| Phase    | 能力                        | 为什么           |
| -------- | ------------------------- | ------------- |
| **P8.0** | Structured Memory         | MVP，结构化长期记忆   |
| **P8.1** | Importance & Tags         | 重要性与标签        |
| **P8.2** | Memory Policy             | 保存、遗忘策略       |
| **P8.3** | Embedding Index           | 向量索引          |
| **P8.4** | Hybrid Retrieval          | 标签 + 向量混合检索   |
| **P8.5** | Memory Reflection         | 自动总结经验        |
| **P8.6** | Graph Memory              | 实体关系图谱        |
| **P8.7** | Shared Memory             | 多 Agent 共享知识  |
| **P8.8** | Enterprise Knowledge Base | 企业知识沉淀        |
| **P8.9** | Cognitive Memory Engine   | 具备持续学习能力的记忆系统 |

---

# 我建议增加一个比 OpenCode、Claude Code 更重要的抽象

## 引入 Memory Event（记忆事件）

目前很多系统都是：

```text
Conversation

↓

Save Memory
```

这是错误的。

应该：

所有 Runtime：

都产生：

Memory Event。

例如：

```text
Execution Success
        │
        ▼
Memory Event
        │
        ▼
Classifier
        │
        ▼
Remember
```

例如：

```text
Tool Failed
↓

Memory Event

↓

"Terminal timeout on Windows"

↓

Experience
```

或者：

```text
User 修改了 Agent 配置

↓

Memory Event

↓

Preference
```

甚至：

```text
RCA 找到根因

↓

Memory Event

↓

Knowledge
```

也就是说：

**Memory Runtime 不应该主动扫描世界，而应该消费统一的 Memory Event。**

这样：

* Execution Runtime 可以产生日志经验。
* Tool Runtime 可以产生工具经验。
* Workspace Runtime 可以产生项目知识。
* Agent Runtime 可以产生用户偏好。
* Workflow Runtime 可以产生业务知识。

整个系统最终会形成一个**事件驱动的长期记忆体系**。

---

## 我还建议提前预留两种 Memory

不要只有一种 Memory，而是从第一版就区分：

```text
Memory
│
├── Episodic Memory（情景记忆）
│      今天修复了什么？
│      执行过哪些任务？
│
├── Semantic Memory（语义知识）
│      Spring Boot 的最佳实践
│      公司开发规范
│
└── （企业版再扩展）
       Procedural Memory（技能）
       Working Memory（工作记忆，由 Context Runtime 管理）
```

这会让后续的 Reflection、RAG、企业知识库、多 Agent 学习都建立在稳定的认知模型之上，而不是把所有内容都塞进一个向量库。对于一个希望从 **MVP 演进到企业级 Agent 平台** 的架构来说，这种抽象会更加稳定、可扩展。
