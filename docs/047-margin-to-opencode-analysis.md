# 047 — 与 OpenCode 差距分析

> **目标**：梳理当前 core-agent 项目与 OpenCode 之间的能力差距，按照优先级排序，明确落地路径。
> **分析日期**：2026-07-24
> **版本**：v0.45.0

---

## 目录

1. [分析方法](#1-分析方法)
2. [总体架构对比](#2-总体架构对比)
3. [P0 — 关键差距（必须补齐）](#3-p0--关键差距必须补齐)
4. [P1 — 重要差距（建议补齐）](#4-p1--重要差距建议补齐)
5. [P2 — 增强差距（体验优化）](#5-p2--增强差距体验优化)
6. [用户自定义扩展点专题](#6-用户自定义扩展点专题)
7. [落地路线图](#7-落地路线图)

---

## 1. 分析方法

### 对比维度

| 维度 | 说明 |
|------|------|
| **工具体系** | bash/read/grep/glob/edit/write/patch/lsp/websearch/webfetch/todo/question 等内置工具的覆盖与能力差距 |
| **Agent 运行时** | Agent 调度、子 agent、上下文压缩、token 管理、流式输出等运行时核心差距 |
| **用户扩展点** | 能否让用户在指定目录自定义 agents/skills/tools/mcp 的加载机制 |
| **体验与治理** | TUI 交互、权限审批、slash 命令、可观测性等产品层差距 |

### 优先级定义

| 等级 | 定义 | 预期时间 |
|------|------|----------|
| **P0** | 核心能力缺失，影响 Agent 基本可用性 | 1-2 周 |
| **P1** | 重要能力缺失，影响扩展性和深度使用 | 2-4 周 |
| **P2** | 体验增强，影响用户满意度和工程效率 | 按需排期 |

---

## 2. 总体架构对比

### OpenCode 架构

```
                 LLM Brain
                    |
                    |
              Tool Calling Layer
                    |
 ------------------------------------------------
 文件世界          代码世界        系统世界
 read             lsp             bash
 grep             edit            git
 glob             write           docker
 ------------------------------------------------
                    |
              Developer Machine
```

### Core-Agent 当前架构

```
                 LLM Brain (core-agent-kernel)
                    |
                    |
              Tool Calling Layer (core-agent-tool)
                    |
 ------------------------------------------------
 文件层         代码分析层      系统层      企业层
 read           lsp            shell       enterprise
 write          ast            git         ai
 edit           code_index     web         runtime
 glob           dependency     cron
 grep           project        agent
 patch          decompiler     plan
 list                           todo
 info                           ask
 delete
 move
 copy
 ------------------------------------------------
                    |
    Kernel (RuntimeKernel) — 生命周期管理
                    |
    Extension Manager — 扩展安装/加载/启用/执行
                    |
    Skill Catalog — SKILL.md 发现与加载
                    |
    MCP Client — 外部工具发现
                    |
    CLI (agent-cli) — TUI / 命令行
```

### 核心差异总结

| 对比项 | OpenCode | Core-Agent (当前) | 差距 |
|--------|----------|--------------------|------|
| 内置工具数量 | ~12 | **41**（含企业扩展） | ✅ 领先 |
| 工具接口设计 | 简单函数 | 完整 Trait 体系（Tool/Provider/Registry/Catalog/Executor/Permission） | ✅ 领先 |
| 用户自定义工具 | 可通过插件扩展 | 仅 Builtin + MCP（需环境变量启用） | ❌ P0 |
| 用户自定义 Agent | ❌ 不支持 | 有 AgentManifest 但无运行时加载 | ❌ P0 |
| 用户自定义 Skill | 通过 SKILL.md | 有 SkillCatalog 但发现路径不完整 | ⚠️ 部分 |
| MCP 支持 | 原生支持 | 有 McpClient 但需 `CORE_AGENT_ENABLE_MCP=1` | ⚠️ 部分 |
| LSP 集成 | 实验性 | 6 个 LSP 工具（definition/references/hover/completion/diagnostics/symbols） | ✅ 领先 |
| Web Search | 内置 | 有 web.search（需配置 API key） | ⚠️ 部分 |
| 上下文管理 | 自动压缩 | 有 context compression 策略 | ✅ 领先 |
| 审计追踪 | 无 | 完整 Audit 框架 | ✅ 领先 |
| 权限审批 | 简单 | 三级审批（Risk-Based/Strict/Auto）+ SQLite 持久化 | ✅ 领先 |

---

## 3. P0 — 关键差距（必须补齐）

### P0-1：用户自定义工具目录加载

**现状**：工具目前仅支持 Builtin 注册和 MCP 发现（需环境变量）。用户无法在 `~/.core-agent/tools/` 或项目 `.core-agent/tools/` 下放置自定义工具定义文件自动加载。

**OpenCode 对标**：允许用户通过配置扩展工具集。

**影响**：用户无法扩展 Agent 能力，限制了场景适配。

**实现路径**：
1. 定义用户工具目录规范（`~/.core-agent/tools/` + `.core-agent/tools/`）
2. 支持 YAML/JSON 工具定义文件（基于 ToolDefinition 模型）
3. 实现 `UserToolProvider` 实现 `ToolProvider` trait，从目录发现并注册工具
4. 集成到 `RuntimeKernel` 启动流程

**涉及模块**：`core-agent-tool`、`core-agent-kernel`、`core-agent-config`

**工作量**：中（3-5 天）

---

### P0-2：用户自定义 Agent 加载

**现状**：`core-agent-sdk` 定义了 `AgentManifest` 和 `AgentBuilder`，但运行时没有从目录加载自定义 Agent 的机制。Agent 只能通过代码创建。

**OpenCode 对标**：无直接对标，但作为 Agent OS 方向，此能力是核心差异点。

**影响**：用户无法定义和复用自定义 Agent 角色，多 Agent 协作场景受限。

**实现路径**：
1. 定义 Agent 目录规范（`~/.core-agent/agents/` + `.core-agent/agents/`）
2. 支持 `agent.yaml`/`agent.json` 清单文件（基于 `AgentManifest` + `AgentProfile`）
3. 实现 Agent 目录扫描器，自动发现并注册到 Agent Manager
4. 支持 `agent.spawn` 通过名称引用自定义 Agent

**涉及模块**：`core-agent-agent`、`core-agent-sdk`、`core-agent-kernel`

**工作量**：中（3-5 天）

---

### P0-3：用户自定义 Skills 目录增强

**现状**：SkillCatalog 已支持从 `~/.skills/` 和 `.agents/skills/` 发现 SKILL.md，但：
- 默认 root 路径缺少 `~/.core-agent/skills/`
- 缺少 `.core-agent/skills/` 项目级路径
- 无 CLI 命令管理 skill（`skill list` / `skill install` 等）
- 无 skill 市场安装流程

**OpenCode 对标**：允许用户通过 skill 目录扩展能力。

**影响**：用户编写 skill 后无法方便地管理和发现。

**实现路径**：
1. 增加 `~/.core-agent/skills/` 和 `.core-agent/skills/` 为默认发现路径
2. SkillScope 增加 `Agent` 类型
3. 实现 CLI 子命令：`agent skill list`、`agent skill info <name>`、`agent skill install <path>`
4. 集成到 `RuntimeKernel` 启动流程

**涉及模块**：`core-agent-skill`、`agent-cli`、`core-agent-kernel`

**工作量**：小（2-3 天）

---

### P0-4：MCP 默认启用 + 目录发现

**现状**：MCP 需要设置 `CORE_AGENT_ENABLE_MCP=1` 环境变量才能启用，且配置只从 `mcp.json` 读取，不支持从目录发现多个 MCP 服务器。

**OpenCode 对标**：MCP 是 OpenCode 的原生扩展机制，无需环境变量开关。

**影响**：用户无法零配置使用 MCP 工具，上手门槛高。

**实现路径**：
1. 移除环境变量开关，默认启用 MCP
2. 增加 MCP 服务器目录发现（`~/.core-agent/mcp/` + `.core-agent/mcp/`）
3. 每个子目录下的 `server.yaml`/`server.json` 作为一个 MCP 服务器配置
4. 保持原有 `mcp.json` 全局+项目配置兼容

**涉及模块**：`core-agent-mcp`、`core-agent-tool`（McpToolProvider）

**工作量**：小（2 天）

---

## 4. P1 — 重要差距（建议补齐）

### P1-1：统一扩展目录规范

**现状**：缺少统一的扩展目录结构文档和加载约定。Skill、MCP、Agent、Tool 的发现路径分散在不同模块中，用户需要分别了解。

**影响**：用户学习成本高，不清楚"把我的扩展放在哪里"。

**建议目录结构**：

```
~/.core-agent/                    # 用户级
  ├── agents/                     # 自定义 Agent
  │   └── my-agent/
  │       └── agent.yaml
  ├── tools/                      # 自定义工具
  │   └── my-tool/
  │       └── tool.yaml
  ├── skills/                     # 自定义技能
  │   └── my-skill/
  │       └── SKILL.md
  └── mcp/                        # MCP 服务器
      └── my-server/
          └── server.yaml

.project/.core-agent/             # 项目级
  ├── agents/
  ├── tools/
  ├── skills/
  └── mcp/
```

**实现路径**：
1. 定义 `ExtensionRoot` 统一配置模型
2. 在 `KernelConfig` 中增加扩展目录配置
3. 实现 `ExtensionRootScanner` 统一扫描所有扩展类型
4. 更新文档

**工作量**：小（2 天）

---

### P1-2：Web Search / Web Fetch 工具增强

**现状**：`web.search` 和 `web.fetch` 已定义但需要外部 API key 配置，无默认实现。

**OpenCode 对标**：内置搜索引擎和网页抓取能力。

**影响**：Agent 无法自主搜索网络信息，降低了问题解决能力。

**实现路径**：
1. 为 `web.search` 集成本地可用的搜索 API（如 SearXNG 自托管）
2. 提供主流搜索 API（SerpAPI/Bing/Google）的配置模板
3. 确保 `web.fetch` 能处理常见网页格式（markdown 转换）

**工作量**：小（1-2 天）

---

### P1-3：插件热加载

**现状**：`core-agent-plugin` 定义了插件基础结构，但没有运行时热加载机制。所有扩展需要在启动时注册。

**影响**：用户安装/更新扩展需要重启 Agent 进程。

**实现路径**：
1. 扩展 `ExtensionManager` 支持 `watch` 模式监听目录变化
2. 实现 `install` → `load` → `enable` 的完整热加载流程
3. 支持 `disable` → `upgrade` → `enable` 的热更新流程

**涉及模块**：`core-agent-extension`、`core-agent-plugin`

**工作量**：中（4-5 天）

---

### P1-4：配置热重载

**现状**：Config 在启动时一次性加载，变更需要重启。

**OpenCode 对标**：配置变更即时生效。

**影响**：修改模型、权限模式等配置需要重启进程。

**实现路径**：
1. 实现 `ConfigWatcher` 监听配置文件变化
2. 通过 `RuntimeKernel.reload()` 推送配置变更给各 Runtime
3. 非关键配置（模型、权限模式）支持热更新

**工作量**：中（3-4 天）

---

## 5. P2 — 增强差距（体验优化）

### P2-1：Agent 快照与恢复

**现状**：`AgentSnapshot` 模型已定义，但没有恢复机制。Session 可以 resume 但没有 crash recovery。

**OpenCode 对标**：会话持久化，断点续传。

**影响**：进程崩溃后 Agent 状态丢失。

**实现路径**：
1. 实现 `AgentManager.snapshot()` 定期自动快照
2. 实现 `AgentManager.restore()` 从快照恢复
3. 集成到 `RuntimeKernel` 的生命周期

**工作量**：中（3 天）

---

### P2-2：用户自定义工具运行时注册

**现状**：用户无法在 Agent 运行时动态注册/注销工具。

**影响**：灵活性受限。

**实现路径**：
1. 增加 `ToolRegistry` 运行时注册 API
2. 实现 `tool.register` 和 `tool.unregister` 内置工具
3. 与 Approval 系统集成确保安全

**工作量**：小（2 天）

---

### P2-3：CLI 增强

**现状**：Agent CLI 已有丰富功能，但缺少：
- Shell 自动补全（bash/zsh/powershell）
- 进度条/流式输出（非 TUI 模式）
- 多会话管理 UI

**实现路径**：
1. 利用 clap 生成 shell completion 脚本
2. 非 TUI 模式增加流式输出
3. `agent sessions` 命令增强

**工作量**：小（2-3 天）

---

### P2-4：Slash 命令统一

**现状**：`core-agent-slash` 和 `core_agent::InteractionCommandRegistry` 两套系统并行。

**影响**：维护成本高，命令行为不一致。

**实现路径**：
1. 废弃 `InteractionCommandRegistry`，统一到 `SlashCommandRegistry`
2. 将现有 slash 命令迁移到 `core-agent-slash` 框架
3. CLI 和 TUI 统一使用同一命令注册表

**工作量**：中（3-4 天）

---

## 6. 用户自定义扩展点专题

### 6.1 当前支持情况

| 扩展类型 | 发现机制 | 用户可自定义 | 配置方式 | 优先级 |
|----------|----------|-------------|----------|--------|
| **Tools** | BuiltinProvider 硬编码 | ❌ | 代码注册 | P0 |
| **Skills** | SkillCatalog 从目录发现 | ✅ 部分 | SKILL.md 文件 | P0 |
| **Agents** | AgentManager 从代码创建 | ❌ | 无 | P0 |
| **MCP Servers** | discover_mcp_servers 从文件加载 | ⚠️ 需环境变量 | mcp.json | P0 |
| **Plugins** | ExtensionManager 从代码安装 | ❌ | 无 | P1 |
| **Slash Commands** | SlashCommandRegistry 代码注册 | ❌ | 无 | P2 |

### 6.2 统一加载架构（目标）

```
用户放置目录
~/.core-agent/ 或 ./.core-agent/
  ├── agents/
  │   └── my-agent/agent.yaml
  ├── tools/
  │   └── my-tool/tool.yaml
  ├── skills/
  │   └── my-skill/SKILL.md
  └── mcp/
      └── my-server/server.yaml

         ↓ ExtensionRootScanner

    UnifiedExtensionLoader
         ↓
    ┌──────────────────────────────┐
    │  AgentProfileLoader          │  → AgentManager
    │  ToolDefinitionLoader        │  → ToolRegistry
    │  SkillCatalogLoader          │  → SkillCatalog
    │  McpServerLoader             │  → McpToolProvider
    └──────────────────────────────┘
```

### 6.3 各类扩展的定义格式

**Agent 定义 (`agent.yaml`)**：

```yaml
name: java-reviewer
version: "1.0.0"
description: Java code review specialist
model: gpt-4
tools:
  - builtin/file.read
  - builtin/file.grep
  - builtin/lsp.*
skills:
  - java-analysis
instructions: |
  You are a Java code reviewer. Analyze code for:
  - Null safety
  - Thread safety
  - Performance issues
permissions:
  - repository.read
```

**Tool 定义 (`tool.yaml`)**：

```yaml
name: my-custom-tool
version: "1.0.0"
description: My custom tool description
input_schema:
  type: object
  required: [param1]
  properties:
    param1:
      type: string
      description: First parameter
runtime: # 执行方式
  type: command  # 或 http, mcp, wasm
  command: my-tool-executable
  args: [--input, "{param1}"]
```

**MCP Server 定义 (`server.yaml`)**：

```yaml
name: my-mcp-server
command: npx
args: [ "@my/mcp-server" ]
env:
  - MY_API_KEY
request_timeout_ms: 30000
```

---

## 7. 落地路线图

### Phase 1 — 扩展目录加载（Week 1-2）

| 任务 | 模块 | 工作量 |
|------|------|--------|
| P0-1：用户自定义工具目录加载 | `core-agent-tool` | 3-5天 |
| P0-3：Skills 目录增强 | `core-agent-skill` | 2-3天 |
| P0-4：MCP 默认启用+目录发现 | `core-agent-mcp` | 2天 |
| P1-1：统一扩展目录规范 | 新增 `core-agent-scanner` | 2天 |

### Phase 2 — Agent 扩展（Week 3-4）

| 任务 | 模块 | 工作量 |
|------|------|--------|
| P0-2：用户自定义 Agent 加载 | `core-agent-agent` + `core-agent-kernel` | 3-5天 |
| P1-3：插件热加载 | `core-agent-extension` | 4-5天 |
| P1-4：配置热重载 | `core-agent-config` | 3-4天 |

### Phase 3 — 体验增强（Week 5+）

| 任务 | 模块 | 工作量 |
|------|------|--------|
| P1-2：Web Search/Fetch 增强 | `core-agent-tool` | 1-2天 |
| P2-1：Agent 快照与恢复 | `core-agent-agent` | 3天 |
| P2-3：CLI 增强 | `agent-cli` | 2-3天 |
| P2-4：Slash 命令统一 | `core-agent-slash` + `agent-cli` | 3-4天 |

---

## 附录：工具完整清单

### Core-Agent 现有 41 个内置工具

| 分类 | 工具 | 状态 | 对应 OpenCode |
|------|------|------|---------------|
| **File (11)** | file.read, file.write, file.edit, file.patch, file.glob, file.grep, file.delete, file.move, file.copy, file.info, file.list | ✅ 全部实现 | read, write, edit, patch, glob, grep |
| **Shell (3)** | shell.exec, shell.script, shell.bg | ✅ 全部实现 | bash |
| **Git (7)** | git.diff, git.status, git.log, git.commit, git.branch, git.checkout, git.push | ✅ 全部实现 | git |
| **Web (2)** | web.fetch, web.search | ⚠️ 需配置 API key | webfetch, websearch |
| **Ask (3)** | ask.user, ask.confirm, ask.select | ✅ 全部实现 | question |
| **Todo (3)** | todo.add, todo.update, todo.list | ✅ 全部实现 | todowrite, todoread |
| **Agent (3)** | agent.spawn, agent.send, agent.list | ✅ 全部实现 | — |
| **Plan (3)** | plan.create, plan.update, plan.review | ✅ 全部实现 | — |
| **Cron (3)** | cron.create, cron.list, cron.delete | ✅ 全部实现 | — |
| **LSP (6)** | lsp.definition, lsp.references, lsp.hover, lsp.completion, lsp.diagnostics, lsp.symbols | ✅ 全部实现 | lsp（实验） |
| **AST (2)** | ast.search, ast.replace | ✅ 全部实现 | — |
| **Code Index (2)** | code_index.index, code_index.query | ✅ 全部实现 | — |
| **Dependency (1)** | dependency.inspect | ✅ 全部实现 | — |
| **Decompiler (1)** | decompiler.decompile | ✅ 全部实现 | — |
| **Project (4)** | project.analyzer, architecture.graph, callgraph.query, api.analyzer | ✅ 全部实现 | — |
| **Runtime (5)** | log.query, metric.query, trace.query, cmdb.query, k8s.query | ⚠️ 企业 stub | — |
| **Enterprise (5)** | knowledge.search, ticket.create, notification.send, browser.navigate, browser.screenshot | ⚠️ 企业 stub | — |
| **AI (5)** | code.review, test.generate, security.scan, data.analyze, vision.analyze | ⚠️ 企业 stub | — |

### 差距总结

- **工具数量**：Core-Agent 41 个 vs OpenCode ~12 个，**领先**
- **工具设计**：Core-Agent 有完整 Trait 体系（Tool/Provider/Registry/Catalog/Executor/Permission/Interceptor/Lifecycle），**领先**
- **用户自定义**：Core-Agent 有框架但缺少目录加载机制，**这是最大差距**
- **代码智能**：Core-Agent 有 LSP/AST/CodeIndex/Dependency/Project 等 15+ 个代码分析工具，**显著领先**
- **企业能力**：Core-Agent 有 Runtime/Enterprise/AI 等 15 个企业工具（部分 stub），**OpenCode 不具备**