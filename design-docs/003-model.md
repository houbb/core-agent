这一阶段开始，我们终于进入 **AI Runtime**。

但是请注意：

> **P2 不是 AI Runtime，而是 Model Runtime。**

很多项目把：

```
AI = LLM
```

这是最大的设计错误。

真正应该是：

```
Agent Runtime
    │
    ├── Session Runtime   ✅ P0
    ├── Context Runtime   ✅ P1
    ├── Model Runtime     ✅ P2
    ├── Tool Runtime
    ├── Planning Runtime
    ├── ...
```

**LLM 只是整个 Agent 的一个 Provider。**

所以这一层必须做成：

> **Provider Runtime，而不是 OpenAI Runtime。**

---

# Phase 2：Model Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责统一管理所有 AI Model，并提供稳定一致的推理接口。**

注意：

Model Runtime：

**不知道什么是 Session。**

不知道：

Conversation。

不知道：

Workspace。

它只知道：

```
Input

↓

Model

↓

Output
```

所以：

Model Runtime：

应该是整个系统里面：

**最纯净的 Runtime。**

---

# 为什么放这里？

因为：

前面：

Session

Context

已经完成。

现在：

终于可以：

```
Context

↓

Model Runtime

↓

Result
```

以后：

Planner

Tool

Memory

都是：

调用：

Model Runtime。

---

# 第一性原理

Model Runtime：

其实：

只有五件事情：

```
接收请求

↓

选择模型

↓

执行推理

↓

处理流式返回

↓

返回统一结果
```

结束。

其它：

不要做。

---

# Runtime职责

只负责：

```
Inference

Streaming

Model Routing

Provider

Error Handling
```

不要：

Tool Call。

不要：

Memory。

不要：

Planner。

以后。

---

# Runtime架构

建议：

```
Model Runtime

│

├── ModelManager

├── ModelProvider

├── ModelRouter

├── InferenceEngine

├── StreamEngine

├── CapabilityRegistry

└── ModelCatalog
```

---

# 为什么不要只有 Provider？

OpenCode：

很多：

Provider：

越来越复杂。

建议：

拆。

---

# 一、ModelManager

唯一：

入口。

例如：

```rust
generate(request)

stream(request)

embedding(request)

vision(request)
```

所有：

Runtime：

只调用：

Manager。

以后：

无需：

关心：

Claude。

OpenAI。

---

# 二、ModelProvider

真正：

调用：

API。

例如：

```
OpenAI

Claude

Gemini

DeepSeek

Qwen

Ollama

LM Studio

OpenRouter
```

统一：

```rust
trait ModelProvider
```

以后：

新增 Provider：

不用：

修改：

Runtime。

---

# 三、ModelRouter

企业：

必须有。

例如：

```
Coding

↓

Claude

---------

Translation

↓

GPT

---------

Vision

↓

Gemini

---------

Cheap

↓

DeepSeek
```

Router：

决定：

哪个：

Model。

以后：

成本控制：

全部：

这里。

---

建议：

Router：

支持：

```
Manual

Rule

Capability

Cost

Latency

Fallback
```

以后：

AI Gateway：

直接：

调用。

---

# 四、InferenceEngine

真正：

执行：

推理。

例如：

```
Context

↓

Provider

↓

Retry

↓

Timeout

↓

Response
```

以后：

统一：

Tracing。

---

不要：

Provider：

自己：

处理：

Retry。

统一。

---

# 五、StreamEngine

Streaming：

单独。

不要：

if(stream)

为什么？

以后：

```
CLI

Desktop

Web

API
```

全部：

依赖。

Streaming：

必须：

统一。

---

# 六、CapabilityRegistry

这是：

企业：

最容易忽略。

例如：

不是：

所有：

Model：

都有：

```
Tool Call

Vision

Embedding

Thinking

Image

Audio
```

所以：

建议：

Capability。

例如：

```
Claude

✓ Tool

✓ Thinking

✗ Image

---------

Gemini

✓ Vision

✓ Tool

✓ Audio

---------

Ollama

✓ Chat

✗ Vision
```

以后：

Runtime：

自动：

检查。

---

# 七、ModelCatalog

不要：

Provider：

自己：

管理：

Model。

统一：

Catalog。

例如：

```
gpt-5

claude-sonnet

gemini-3

deepseek-r1

qwen3

llama4
```

统一：

Metadata。

例如：

```
Context

Price

Latency

Provider

Capability
```

以后：

UI：

直接：

展示。

---

# Request

建议：

不要：

String。

统一：

Request。

```
ModelRequest

├── Context

├── Config

├── Stream

├── Metadata

└── Capability
```

以后：

任何：

模型：

一样。

---

# Response

统一：

```
ModelResponse

├── Content

├── Usage

├── FinishReason

├── Metadata

└── RawResponse
```

Raw：

以后：

Debug。

---

# Usage

第一版：

就要。

例如：

```
Prompt Token

Completion Token

Cache Token

Latency

Cost
```

以后：

Billing。

Analytics。

全部：

依赖。

---

# API设计

Manager：

```rust
generate()

stream()

embedding()

vision()
```

Provider：

```rust
invoke()
```

Router：

```rust
select()
```

Catalog：

```rust
list()

find()
```

Capability：

```rust
supports()
```

---

# 生命周期

建议：

```
Request

↓

Route

↓

Capability Check

↓

Provider

↓

Stream

↓

Response

↓

Usage

↓

Complete
```

---

# SQLite

建议：

不要：

保存：

聊天。

只保存：

模型。

例如：

```
model_provider

model

model_usage
```

后面：

Billing。

直接：

升级。

---

# UX设计

建议：

整个：

Model：

独立。

例如：

Settings：

```
Models

────────────────────

OpenAI

Claude

Gemini

Ollama

DeepSeek
```

点击：

Claude：

```
API Key

Endpoint

Timeout

Retry

Rate Limit
```

以后：

不用：

改。

---

Agent：

顶部：

```
Claude Sonnet

▼
```

点击：

```
Claude

GPT

Gemini

DeepSeek

Auto
```

Auto：

其实：

Router。

---

增加：

Capability：

例如：

```
Claude

✓ Tool

✓ Thinking

✗ Vision

---------

Gemini

✓ Vision

✓ Audio

✓ Tool
```

用户：

一眼：

知道。

---

再增加：

Usage：

实时：

```
Model

Claude

Latency

0.9s

Prompt

8k

Completion

500

Cost

$0.02
```

企业：

非常喜欢。

---

# MVP 不做什么

不要：

* ❌ AI Gateway
* ❌ Prompt Engineering
* ❌ Tool Call 执行（这里只返回 Tool Call 请求）
* ❌ Function Dispatcher
* ❌ Multi-Model Debate
* ❌ Self Reflection
* ❌ Auto Retry Strategy（复杂策略）
* ❌ AI Workflow
* ❌ Agent Loop

全部：

以后。

---

# 扩展点（第一版就预留）

```
Model Runtime
│
├── ModelProvider          // OpenAI、Claude、Ollama...
├── ModelRouter            // 路由策略
├── CapabilityRegistry     // 能力声明
├── ModelCatalog           // 模型目录
├── RequestInterceptor     // 请求拦截
├── ResponseInterceptor    // 响应拦截
├── UsageCollector         // Token、Cost
├── RetryPolicy            // 重试策略
├── RateLimiter            // 限流
└── ModelObserver          // Trace、Audit、Metrics
```

---

# 企业版演进路线

| Phase    | 能力                    | 为什么                         |
| -------- | --------------------- | --------------------------- |
| **P2.0** | Chat Completion       | MVP 推理能力                    |
| **P2.1** | Streaming             | CLI / Desktop 实时输出          |
| **P2.2** | 多 Provider            | OpenAI、Claude、Gemini、Ollama |
| **P2.3** | Capability Registry   | 自动识别模型能力                    |
| **P2.4** | Router                | 自动选择最优模型                    |
| **P2.5** | Fallback              | Provider 故障自动切换             |
| **P2.6** | Cost & Usage          | Token、成本统计                  |
| **P2.7** | Model Policy          | 企业模型白名单、黑名单                 |
| **P2.8** | AI Gateway            | 多租户统一模型网关                   |
| **P2.9** | Distributed Inference | 多节点、本地+云端统一调度               |

---

# 我建议增加一个比 OpenCode、Grok Build 更适合作为平台的抽象

新增一个 **Model Profile（模型画像）**。

不要只维护一个模型名称：

```
claude-sonnet-4
```

而是维护：

```
Model Profile
│
├── Identity
├── Capability
├── Pricing
├── Performance
├── Limits
├── Provider
├── Policies
└── Metadata
```

例如：

```yaml
profile: coding-fast

provider: anthropic

model: claude-sonnet-4

capabilities:
  tool: true
  vision: false
  thinking: true

limits:
  context: 200000

pricing:
  input: ...
  output: ...

policies:
  allow_workspace: true
  allow_network: false
```

以后，Planner、Tool Runtime、企业策略都**不直接依赖具体模型名称**，而是依赖 `Model Profile`。这样即使未来把 `claude-sonnet-4` 替换成其他模型，也只需要更新 Profile，而不需要修改业务逻辑。这种抽象对于长期维护和企业级平台尤为重要。
