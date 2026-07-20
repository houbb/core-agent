# Core-Agent P6 设计

# P6：Agent Knowledge Intelligence Layer（知识智能层）

模块：

```text
core-agent-knowledge
core-agent-rag
core-agent-vector
core-agent-document
core-agent-semantic
```

---

# 一、P6 目标

前面阶段：

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

```

但是 Agent 还有一个核心问题：

> 它不知道企业世界里的知识。

---

传统 LLM：

```text
模型知识

↓

固定

```

企业 Agent：

需要：

```text
企业文档

代码

数据库

日志

CMDB

工单

历史案例

业务规则

专家经验

```

形成：

```text
Enterprise Knowledge Brain
```

---

# 二、整体架构

```text
                         core-agent


                              |


              Knowledge Intelligence Layer


 ----------------------------------------------------------------


 Document        Knowledge        RAG


    |               |               |


 文件解析          知识管理          检索增强



 Vector          Semantic


    |               |


 向量搜索          语义理解



 ----------------------------------------------------------------


                              |

                       Context Builder


                              |

                          Agent Runtime

```

---

# 三、core-agent-document ⭐⭐⭐⭐⭐

## 定位

企业文档理解基础设施。

负责：

> 把各种非结构化内容变成 Agent 可以理解的数据。

---

# 支持类型

P0：

```text
PDF

Markdown

TXT

HTML

DOCX

Code

```

未来：

```text
PPT

Excel

Image

Audio

Video

```

---

# Document Model

```java
class Document {


id;


name;


type;


source;


content;


metadata;


status;


}
```

---

例如：

```json
{
"name":

"支付系统架构设计.md",


"type":

"markdown",


"source":

"github"

}
```

---

# Document Pipeline

```text
Upload


 |

Parse


 |

Clean


 |

Split


 |

Embed


 |

Store


```

---

# Parser Runtime

架构：

```text
Document


 |

Parser


 |

Document AST


 |

Knowledge

```

---

例如：

Markdown：

```markdown
# Payment


## Architecture


## API


## Database

```

解析：

```json
{
"title":"Payment",

"sections":[

"Architecture",

"API"

]

}
```

---

# UX

文档中心：

```text
Knowledge Documents


支付系统设计


状态:

Indexed


Chunks:

325


Embedding:

Completed

```

---

# 注意点

不要只保存纯文本。

需要：

```text
Document Structure


标题

章节

表格

代码块

链接

```

---

---

# 四、core-agent-vector ⭐⭐⭐⭐⭐

## 定位

向量检索基础能力。

---

# 为什么需要？

关键词：

```text
"数据库连接失败"

```

可能对应：

```text
"Connection Pool Exhausted"

```

需要语义搜索。

---

# Vector Model

```java
class VectorRecord {


id;


content;


embedding;


metadata;


source;


}
```

---

# Embedding Pipeline

```text
Document


 |

Chunk


 |

Embedding Model


 |

Vector Database

```

---

# Vector Storage

P0：

SQLite + Vector Extension

未来：

```text
Milvus

Qdrant

Weaviate

pgvector

```

---

# Search

输入：

```text
订单接口超时

```

转换：

```text
embedding

```

查询：

```text
Top K

```

返回：

```text
相关知识片段

```

---

# UX

搜索：

```text
Ask Knowledge


为什么订单服务超时？


相关知识:

1.

数据库慢查询规范


2.

订单架构文档


```

---

# 注意点

向量不是万能。

必须：

```text
Vector

+

Keyword

+

Metadata Filter

```

混合搜索。

---

---

# 五、core-agent-rag ⭐⭐⭐⭐⭐

## 定位

Retrieval Augmented Generation Runtime。

这是 Agent 获取知识的核心。

---

# RAG Pipeline

```text
User Question


       |

Query Rewrite


       |

Retriever


       |

Reranker


       |

Context Builder


       |

LLM


       |

Answer

```

---

# RAG Components

## 1. Retriever

寻找：

```text
相关内容

```

---

## 2. Reranker

重新排序。

例如：

Top 100

↓

Top 5

---

## 3. Context Compressor

压缩：

避免：

```text
100页文档

全部塞给模型

```

---

# RAG Model

```java
class RetrievalResult {


content;


score;


source;


metadata;


}
```

---

# Agent 调用

```text
Agent


 |

Knowledge Tool


 |

RAG


 |

Answer

```

---

# UX

回答：

```text
根据支付架构文档：


订单服务调用:

Payment Gateway


参考:

payment-design.md

第3章


```

---

# 注意点

必须返回：

来源。

否则企业不敢用。

---

---

# 六、core-agent-semantic ⭐⭐⭐⭐⭐

## 定位

语义理解层。

不仅搜索。

让 Agent 理解：

* 概念
* 关系
* 意图

---

# Semantic Model

```text
Knowledge


 |

Entity


 |

Relation


 |

Concept

```

---

例如：

文档：

```text
订单服务依赖支付服务

```

抽取：

```text
Entity:

Order Service


Relation:

depends_on


Entity:

Payment Service

```

---

# Semantic Graph

类似：

Knowledge Graph。

```text
Order Service

       |

 depends_on

       |

Payment Service


       |

 uses

       |

MySQL

```

---

# Semantic Entity

```java
class Entity {


id;


name;


type;


attributes;


}
```

---

# Relation

```java
class Relation {


source;


target;


type;


confidence;


}
```

---

# 应用

Agent 问：

```text
支付服务异常影响什么？

```

Semantic：

找到：

```text
Payment

 |

Order

 |

Checkout

```

---

# UX

知识地图：

```text
Payment System


  |

----------------


Order Service


Database


Risk Service


```

---

# 注意点

P6 不建议一开始做完整知识图谱。

MVP：

```text
Entity Extraction

Relation Storage

Simple Graph Query

```

---

---

# 七、core-agent-knowledge ⭐⭐⭐⭐⭐

## 定位

统一知识管理层。

上层入口。

---

# Knowledge 类型

```text
Knowledge


├── Document Knowledge


├── Code Knowledge


├── Runtime Knowledge


├── Business Knowledge


├── Experience Knowledge


```

---

# Knowledge Item

```java
class KnowledgeItem {


id;


type;


content;


source;


confidence;


owner;


}
```

---

# Knowledge Lifecycle

```text
Create


 |

Review


 |

Publish


 |

Update


 |

Archive

```

---

# Knowledge Source

来源：

```text
Manual

Document

Agent Learning

System Data

External API

```

---

# UX

知识中心：

```text
Knowledge Base


支付系统


├── 架构文档

├── API说明

├── 故障案例

├── 最佳实践


```

---

# 注意点

知识必须：

有来源

有版本

有权限

---

# 八、P6 和 Agent Runtime 集成

核心：

```text
                 Agent


                   |


             Context Builder


                   |


 ------------------------------------------------


 Memory          Knowledge          Context


                   |


                  RAG


                   |


              Vector/Semantic


                   |


              Enterprise Data

```

---

# 九、P6 数据关系

```text
Document


 |

Chunk


 |

Embedding


 |

Vector


 |

Retrieval


 |

Context


 |

Agent Answer


 |

Feedback


 |

Knowledge Update

```

---

# 十、Repo 设计

继续：

```text
core-agent


├── core-agent-knowledge

├── core-agent-document

├── core-agent-vector

├── core-agent-rag

├── core-agent-semantic

```

完整：

```text
core-agent

├── runtime

├── intelligence

├── multi-agent

├── extension

├── governance

├── evolution

├── knowledge

```

---

# 十一、P6 MVP 推荐顺序

## Phase 1

先做：

```text
core-agent-document

core-agent-vector

```

完成：

文档 → Embedding → 搜索

---

## Phase 2

```text
core-agent-rag

```

实现：

Agent 查询企业知识。

---

## Phase 3

```text
core-agent-knowledge

```

统一管理。

---

## Phase 4

```text
core-agent-semantic

```

增加：

知识关系。

---

# 十二、P6 完成后的能力

整个 Agent 演进：

```text
P0

能执行


↓

P1

会规划


↓

P2

会协作


↓

P3

会扩展


↓

P4

企业可控


↓

P5

会学习


↓

P6

拥有知识大脑

```

最终：

```text
Agent

+

Enterprise Knowledge

=

企业 AI Operating System

```

---

P7 下一阶段建议：

```text
core-agent-ui
core-agent-desktop
core-agent-terminal
core-agent-ide
core-agent-web
core-agent-mobile
```

进入：

**Agent Experience Layer（多端体验层）**

也就是把前面的 Runtime 能力真正产品化。
