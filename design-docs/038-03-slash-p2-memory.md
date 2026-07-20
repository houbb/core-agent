# Phase 2：Core-Agent Memory & Knowledge Runtime

## 目标

Phase 0.5：

```text
Context Runtime
Session Runtime
Checkpoint Runtime
Compact Engine
```

解决：

> Agent 当前这一次对话如何工作。

Phase 1：

```text
Code Intelligence Runtime
Tool Governance Runtime
```

解决：

> Agent 如何理解代码并安全执行。

Phase 2 开始解决更核心的问题：

> Agent 如何长期积累知识、形成工程经验、持续进化。

新增 Slash：

```text
/memory
/memory-show
/memory-save
/memory-clear
/knowledge
/learn
```

---

# 1. 总体架构设计

新增：

```text
core-agent

├── slash-runtime
│
├── context-runtime
│
├── code-intelligence-runtime
│
├── execution-runtime
│
├── memory-runtime              ⭐
│
├── knowledge-runtime           ⭐
│
├── learning-runtime            ⭐
│
└── retrieval-runtime           ⭐
```

整体：

```text
                Terminal
                    |
                Desktop
                    |
                 API
                    |
                    v

              Slash Runtime


                    |
        +-----------+------------+
        |                        |
        v                        v

    Memory Runtime        Knowledge Runtime


        |                        |

   Personal Memory        Engineering Knowledge

   Project Memory         Team Knowledge

   Session Memory         Organization Knowledge

```

---

# 2. 核心设计原则

## 不要把 Memory 等同于 Chat History

很多 Agent 产品的问题：

```text
历史消息
     |
     |
全部塞给模型
```

结果：

* token 浪费
* 噪声增加
* 长期不可控

正确：

```text
Memory

=
Structured Knowledge

+
Semantic Retrieval

+
Lifecycle Management

```

---

# 3. Memory Runtime 设计

## Memory 分层

参考：

* Claude Code memory
* Cursor Rules
* OpenAI Agents Memory
* MemGPT / Letta

建议：

```text
Memory


├── Working Memory
│
│   当前上下文
│
│
├── Session Memory
│
│   当前任务
│
│
├── Project Memory
│
│   项目规则
│
│
├── User Memory
│
│   用户偏好
│
│
└── Organization Memory
│
    企业知识

```

---

# 4. Memory 数据模型

```rust
struct AgentMemory {


id:String,


scope:MemoryScope,


type:MemoryType,


content:String,


importance:f32,


embedding:Option<Vector>,


created_at:Timestamp,


expire_at:Option<Timestamp>


}
```

---

# Memory Scope

```rust
enum MemoryScope {


Session,


Project,


User,


Organization


}
```

---

# Memory Type

```rust
enum MemoryType {


Fact,

Preference,

Decision,

Rule,

Architecture,

Experience


}
```

---

# 5. Command 1

# `/memory`

## 定位

Memory Runtime 总入口。

---

使用：

```text
/memory
```

输出：

```text
╭──────────────────────╮
│ Agent Memory Status  │
╰──────────────────────╯


Working Memory

12 items


Project Memory

56 items


User Memory

18 items


Organization Memory

203 items


Storage:

core-storage


Retrieval:

enabled

```

---

Desktop：

增加 Memory Panel：

```text
--------------------------------

Chat


Memory


Project

  - Java17
  - SpringBoot
  - SQLite


Rules

  - No Redis


Decisions

  - Use Event Driven


--------------------------------

```

---

# 6. Command 2

# `/memory-show`

## 查看具体 Memory

用法：

```text
/memory-show
```

或者：

```text
/memory-show project
```

---

输出：

```text
Project Memory


1.

Database Choice


SQLite first

Importance:

0.92


Created:

2026-07-20



2.

Architecture Rule


No Redis before scaling


Importance:

0.87

```

---

# 7. Command 3

# `/memory-save`

⭐⭐⭐⭐⭐

这是 Agent 主动学习入口。

---

使用：

```text
/memory-save
```

交互：

```text
What should Agent remember?


>
Use SQLite before MySQL migration


Scope:


( ) Session

(x) Project

( ) User

```

---

保存：

```json
{
"type":"Rule",

"scope":"Project",

"content":
"Use SQLite before MySQL migration",

"importance":0.9
}
```

---

# 自动 Memory Extraction

未来：

Agent 自动发现：

用户：

> 后续所有 core 服务先不要 Redis

Agent：

```text
Detected Rule:

"No Redis initially"

Save memory?

Yes / No

```

类似：

Claude Code CLAUDE.md。

---

# 8. Command 4

# `/memory-clear`

删除。

用法：

```text
/memory-clear project
```

确认：

```text
Warning


Delete:

56 project memories?


Confirm:

Y/N

```

---

# 注意

不能直接物理删除。

应该：

```text
Soft Delete


memory.deleted=true

```

方便：

* 审计
* 恢复
* 分析

---

# 9. Knowledge Runtime

Memory 是：

> Agent 自己知道的东西。

Knowledge 是：

> 外部可学习的东西。

---

架构：

```text
Knowledge Runtime


        |

 Knowledge Source


        |

 Parser


        |

 Chunker


        |

 Index


        |

 Retrieval


```

---

# Knowledge Source

支持：

```text
文件

Git Repository

Markdown

PDF

Wiki

API Docs

Database Schema

```

---

# 10. Command 5

# `/knowledge`

## 查看知识库

使用：

```text
/knowledge
```

输出：

```text
Knowledge Base


Sources:


core-agent-docs

120 files


Spring Docs

500 pages


Company Rules

230 documents


Index:

Ready


```

---

# 11. Command 6

# `/learn`

⭐⭐⭐⭐⭐

Agent 学习入口。

---

使用：

```text
/learn ./docs
```

流程：

```text
Scan


 ↓


Parse


 ↓


Chunk


 ↓


Embed


 ↓


Index


 ↓


Ready

```

---

UX：

```text
Learning...


Files:

235


Chunks:

8200


Vectors:

8200


Completed

```

---

# 12. 学习算法设计

不要：

```text
全文 embedding
```

推荐：

## Hierarchical Knowledge Index

类似：

RAG + Knowledge Graph。

结构：

```text
Document


 |
 v


Section


 |
 v


Chunk


 |
 v


Concept


 |
 v


Relation

```

---

# 13. Retrieval Runtime

未来：

Agent 查询：

```text
How does auth work?
```

流程：

```text
Question


 |

Memory Retrieval


 |

Knowledge Retrieval


 |

Code Retrieval


 |

LLM


 |

Answer

```

---

# 14. Memory 与 Compact 的关系

非常重要。

Phase 0.5：

```text
/compact
```

压缩：

Conversation

Phase 2：

```text
/memory-save
```

沉淀：

Knowledge

关系：

```text
Conversation

       |
       |
       v

 Compact

       |
       |
       v

 Memory Extraction

       |
       |
       v

 Knowledge Base

```

---

# 15. 插件化设计

新增插件：

```text
core-agent-plugin-memory


提供:

MemoryCommand

MemoryStore

MemoryRetriever



core-agent-plugin-knowledge


提供:

KnowledgeCommand

Indexer

Retriever

```

---

# 16. 与 Core 平台连接

## core-storage

存：

```text
memory.json

knowledge files

vector index

```

---

## core-ai

负责：

```text
summarization

embedding

memory extraction

```

---

## core-audit

记录：

```text
memory created

memory deleted

knowledge imported

```

---

## core-config

配置：

```yaml
memory:
  max_size: 10GB

retrieval:
  top_k: 10

learning:
  auto_extract: true
```

---

# 17. Phase 2 完成能力

完成后：

```text
core-agent


拥有：

✓ 短期上下文

✓ 长期记忆

✓ 项目知识

✓ 企业知识

✓ 自动学习

✓ RAG 检索

✓ Memory 生命周期


```

能力：

```text
Claude Code

+
Cursor Rules

+
MemGPT

+
Enterprise Knowledge System

```

---

# Phase 2 关键注意点

## 1. Memory 不应该全部自动保存

必须：

```text
Candidate Memory

        |

Confidence Score

        |

Human Confirm

        |

Permanent Memory

```

否则 Agent 会污染自己。

---

## 2. Vector Database 不要过早引入

符合你 Core 平台原则：

> 简洁、SQLite 起步。

建议：

Phase 2 MVP：

```text
SQLite

+
FTS5

+
Embedding Table

```

后期：

```text
SQLite

-->

pgvector

-->

Milvus

```

---

## 3. Memory 是未来 Agent Society 基础

Phase 3：

```text
Agent Society Layer
```

多个 Agent：

```text
Planner Agent

Coder Agent

Reviewer Agent

Security Agent

```

共享：

```text
Organization Memory

```

所以 Phase 2 是整个 Agent OS 的“长期大脑”。

---

下一阶段建议：

# Phase 3：Agent Society Runtime

新增：

```text
/agents
/delegate
/team
/roles
/collaborate
```

把单 Agent 升级成 **Multi-Agent Operating System**。
