这里我会做一个**比较大的调整**。

很多 Agent 框架最后才开始支持：

* MCP
* Plugin
* Extension
* Provider

但如果你的目标是做一个**类似 OpenCode，但能够长期演进的开源平台**，那么 **P12 不应该叫 Plugin Runtime**。

我建议叫：

> **Extension Runtime（扩展运行时）**

因为 **Plugin** 太狭义了。

未来接入的不只是插件，而是：

* MCP Server
* Tool Provider
* Model Provider
* Storage Provider
* Git Provider
* Browser Provider
* 企业插件
* 第三方 SaaS
* 本地脚本
* WASM 扩展

所以应该统一抽象成：

> **Extension（扩展）**

---

# Phase 12：Extension Runtime ⭐⭐⭐⭐⭐

## 一句话定位

> **负责平台所有外部能力的接入、管理、隔离和生命周期。**

Extension Runtime：

不知道：

* Agent
* Workflow
* Planner

它只负责：

```text
Discover

↓

Install

↓

Load

↓

Execute

↓

Unload
```

它更像：

VS Code Extension Host。

而不是：

插件管理器。

---

# 为什么放 P12？

因为：

现在：

已经拥有：

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

↓

Memory

↓

Event

↓

Workflow

↓

Multi-Agent
```

现在：

平台：

已经稳定。

终于：

允许：

第三方：

进入。

---

# 第一性原理

很多项目：

Extension：

就是：

```text
Plugin

↓

Call
```

真正：

应该：

```text
Extension

↓

Capability

↓

Provider

↓

Runtime
```

Extension：

不是：

代码。

而是：

能力。

---

# Runtime职责

Extension：

负责：

```text
安装

↓

加载

↓

注册能力

↓

运行

↓

升级

↓

卸载
```

不要：

业务。

---

# Runtime架构

建议：

```text
Extension Runtime

│

├── ExtensionManager

├── ExtensionRegistry

├── ExtensionLoader

├── ExtensionHost

├── CapabilityRegistry

├── ProviderManager

├── ExtensionPolicy

├── ExtensionLifecycle

├── ExtensionMarketplace

└── ExtensionObserver
```

---

# 一、ExtensionManager

唯一：

入口。

例如：

```rust
install()

uninstall()

enable()

disable()

upgrade()
```

以后：

CLI。

Desktop。

Web。

统一。

---

# 二、ExtensionRegistry

维护：

全部：

Extension。

例如：

```text
Git

Terminal

Docker

Browser

MCP

Slack
```

统一。

---

# 三、ExtensionLoader

真正：

加载。

例如：

以后：

支持：

```text
Native

WASM

Process

Remote

HTTP
```

不要：

Host：

自己：

加载。

---

# 四、ExtensionHost（重点）

我认为：

整个平台：

一定：

需要：

Host。

类似：

VSCode。

例如：

```text
Platform

↓

Extension Host

↓

Extension
```

以后：

Crash。

不会：

影响：

主程序。

---

# 五、CapabilityRegistry

真正：

注册：

能力。

例如：

```text
Search

Git

Filesystem

SQL

Browser

Embedding
```

不是：

插件。

而是：

Capability。

Agent：

以后：

找：

Capability。

不是：

Plugin。

---

# 六、ProviderManager

以后：

越来越重要。

例如：

```text
Model Provider

↓

OpenAI

Claude

Qwen

Gemini

---------

Storage Provider

↓

Local

S3

OSS
```

统一：

Provider。

---

# 七、ExtensionPolicy

企业：

必须。

例如：

```text
Can Access Network

No

---------

Can Access File

Readonly

---------

Signed

Yes
```

以后：

企业：

放心。

---

# 八、ExtensionLifecycle

生命周期：

建议：

```text
Installed

↓

Loaded

↓

Enabled

↓

Running

↓

Disabled

↓

Uninstalled
```

---

# 九、ExtensionObserver

第一版：

预留。

例如：

```text
Loaded

↓

Capability Registered

↓

Error

↓

Unload
```

以后：

Debug。

---

# Extension对象

建议：

```text
Extension

├── Manifest

├── Version

├── Capability

├── Provider

├── Policy

├── State

└── Metadata
```

---

# Manifest（重点）

建议：

第一版：

就有：

```yaml
id: git

name: Git Extension

version: 1.0

capabilities:

- git.clone

- git.commit

provider:

type: local
```

以后：

Marketplace。

直接：

支持。

---

# Capability

建议：

统一：

```text
Capability

├── Name

├── Version

├── Provider

├── Permissions

└── Metadata
```

不要：

Tool。

以后：

Tool：

只是：

Capability。

---

# API设计

Manager：

```rust
install()

enable()

disable()

upgrade()
```

Loader：

```rust
load()

reload()
```

Registry：

```rust
register()

lookup()
```

Provider：

```rust
resolve()
```

---

# 生命周期

```text
Install

↓

Load

↓

Register Capability

↓

Enable

↓

Execute

↓

Disable
```

---

# SQLite

建议：

```text
extension

extension_manifest

extension_state

capability

provider
```

五张。

---

# UX设计

左边：

增加：

```text
Extensions

────────────

Git

Browser

Docker

Slack

MCP
```

点击：

Git：

例如：

```text
Version

1.0

Capability

Git

Provider

Native
```

下面：

```text
Permissions

Filesystem

✓

Network

✓
```

非常：

透明。

---

增加：

Capability：

例如：

```text
Search

↓

Filesystem

↓

Git

↓

Browser
```

Agent：

知道：

自己：

拥有：

什么。

---

增加：

Marketplace：

例如：

```text
Featured

Git

Docker

Kubernetes

OpenAPI
```

以后：

企业：

安装。

---

# MVP 不做什么

不要：

* ❌ 在线 Marketplace
* ❌ 收费插件
* ❌ 热更新（跨版本）
* ❌ 分布式 Extension
* ❌ Extension Sandbox VM
* ❌ Remote Extension Cluster
* ❌ 插件依赖解析
* ❌ 插件经济系统

以后。

---

# 扩展点（第一版就预留）

```text
Extension Runtime
│
├── ExtensionHost
├── ExtensionLoader
├── CapabilityRegistry
├── ProviderManager
├── ExtensionPolicy
├── ExtensionObserver
├── ExtensionMarketplace
├── Sandbox
└── ExtensionInterceptor
```

---

# 企业版演进路线

| Phase     | 能力                    | 为什么         |
| --------- | --------------------- | ----------- |
| **P12.0** | Local Extension       | MVP，本地扩展    |
| **P12.1** | Capability Registry   | 能力注册中心      |
| **P12.2** | Provider Runtime      | Provider 抽象 |
| **P12.3** | Sandbox               | 安全隔离        |
| **P12.4** | WASM Extension        | 高性能、跨平台     |
| **P12.5** | Remote Extension      | 远程扩展        |
| **P12.6** | Extension Marketplace | 扩展市场        |
| **P12.7** | Enterprise Policy     | 企业权限治理      |
| **P12.8** | Extension SDK         | 开发者生态       |
| **P12.9** | Extension Platform    | 完整扩展平台      |

---

# 我认为还应该再抽象一层：Capability Runtime

这是我认为目前 **OpenCode、Claude Code、Grok Build** 都没有完全做好的地方。

现在很多框架都是：

```text
Agent

↓

Tool
```

或者：

```text
Agent

↓

MCP
```

实际上：

Agent 真正需要的不是 Tool，也不是 MCP。

Agent 需要的是：

> **能力（Capability）**

例如：

```text
Capability
│
├── Read File
│
├── Search Code
│
├── Execute Command
│
├── Search Web
│
├── Commit Git
│
└── Generate Image
```

这些能力可以由不同 Provider 提供：

```text
Read File
      │
      ├── Local FS Provider
      ├── SSH Provider
      ├── S3 Provider
      └── GitHub Provider
```

因此整个调用链建议变成：

```text
Agent
      │
      ▼
Capability
      │
      ▼
Provider
      │
      ▼
Extension
      │
      ▼
Execution
```

这样设计有几个重要优势：

* **Agent 永远依赖 Capability，而不是具体插件。**
* **MCP、本地实现、云服务可以互相替换。**
* **同一种能力可以有多个 Provider，支持自动选择、故障切换、成本优化。**
* **未来接入 MCP、A2A、OpenAPI、企业 SDK 时，都不需要修改 Agent Runtime。**

这也是我认为，一个长期维护的 Agent 平台应该坚持的设计原则：

> **Everything is a Capability，Everything is Pluggable（万物皆能力，万物皆可扩展）。**
