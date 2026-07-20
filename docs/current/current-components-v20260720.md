# 现状 vs 目标：核心组件对标分析

> 对照 `docs/core-components.md` 的 Agent OS 分层模型，逐层梳理实现现状与差异。

---

## 总览

```
预期模型 (core-components.md)                现状
─────────────────────────────────────  ─────────────────────────────────────
Agent Runtime          + Agent Runtime    ✅ 完整 (core-agent-agent + kernel)
SubAgent Runtime       + SubAgent         ✅ 完整 (core-agent-multi + subagent_runtime)
Planner Runtime        + Plan             ✅ 完整 (core-agent-plan)
Context Runtime        + Context          ✅ 完整 (core-agent-context)
Memory Runtime         + Memory           ✅ 完整 (core-agent-memory)
Tool Runtime           + Tools            ✅ 完整 (core-agent-tool)
Skill Runtime          + Skills           ✅ 基础 (src/guidance)
MCP Runtime            + MCP              ✅ 完整 (src/mcp_runtime)
Command Runtime        + Slash(/)         ✅ 完整 (src/interaction)
Mention Runtime        + @                ✅ 完整 (src/interaction)
                                            + Session Runtime     ✅
                                            + Execution Runtime   ✅
                                            + Workflow Runtime    ✅
                                            + Event Runtime       ✅
                                            + Extension Runtime   ✅
                                            + Platform Runtime    ✅
                                            + Config Runtime      ✅
                                            + Workspace Runtime   ✅
                                            + Protocol Contract   ✅
                                            + Collaboration       ✅
                                            + Ecosystem           ✅
                                            + Governance          ✅
                                            + Visual/UI           ✅
                                            + CLI                 ✅
                                            + Desktop             ✅
```

---

## 逐层对标

### 1. LLM — 模型层

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-ai | `core-agent-model` |
| 核心能力 | 推理、生成 | ✅ 推理、流式、重试、超时、fallback、用量统计 |
| 路由 | — | ✅ `DefaultModelRouter`、`ModelManager` |
| 提供者 | — | ✅ `OpenAiCompatibleProvider`（通用 OpenAI 兼容协议） |
| 持久化 | — | ✅ `SqliteModelStore` |
| 差距 | 预期就叫 `core-ai` | 命名不同，但功能完备 |

**结论：✅ 已实现，层名差异不影响功能。**

---

### 2. Agent — 主智能体

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-agent-runtime | `core-agent-agent` + `core-agent-kernel` |
| 核心 | Goal + Reasoning + Memory + Tools + Loop | ✅ Agent 定义完整：Profile/Policy/Snapshot/State |
| 生命周期 | Create → Start → Run → Stop → Destroy | ✅ 9 种操作 + 8 种状态 + 状态机 |
| 持久化 | — | ✅ `SqliteAgentStore` |
| 内核 | — | ✅ `RuntimeKernel`：注册/依赖解析/拓扑启动/健康检查/热重载 |

**结论：✅ 已实现，还额外有 Kernel 做 Runtime 编排。**

---

### 3. SubAgent — 子智能体

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-agent-runtime | `core-agent-multi` + `src/subagent_runtime.rs` |
| 分工 | 专业子智能体 | ✅ 团队/组织/角色/成员/路由 |
| 生命周期 | 短期/动态 | ✅ Team/Organization/MemberState |
| 路由 | — | ✅ `DeterministicAgentRouter`、`AgentDirectory`、`AgentDispatcher` |
| 协作 | — | ✅ `Collaboration`/`CollaborationBinding`/`CollaborationOutcome` |
| 持久化 | — | ✅ `SqliteMultiAgentStore` |

**结论：✅ 已实现，比预期更强（含团队协作）。**

---

### 4. Context — 上下文

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-context | `core-agent-context` |
| 结构 | System + User + Conversation + Tool + Memory | ✅ 8 种 Context：System/User/Conversation/Tool/Memory/Workspace/Environment/Plugin |
| 组装 | Composer | ✅ `DefaultComposer`、`ContextPipeline` |
| 压缩 | — | ✅ `SummaryReducer`、`ReducerConfig` |
| 缓存 | — | ✅ `ContextCache` |
| 快照 | — | ✅ `SqliteContextSnapshotStore`、`ContextSerializer` |
| 观察者 | — | ✅ `ContextObserver` |
| Provider | — | ✅ 4 个内置 Provider + 扩展点 |
| 引用 | — | ✅ `ContextReference`/`ContextPackage`（新增 `context_reference.rs`） |

**结论：✅ 已实现，比预期丰富（8 种 Context 类型 + 全生命周期工具链）。**

---

### 5. Memory — 记忆

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-memory | `core-agent-memory` |
| 短期记忆 | Working Memory | ✅ `MemoryKind::Working`、`MemoryType` |
| 长期记忆 | Long Memory | ✅ `MemoryKind::LongTerm`、`MemoryImportance` |
| 知识库 | Knowledge Base | ✅ `MemorySource::Knowledge`、`MemorySourceKind` |
| 检索 | — | ✅ `StructuredMemoryRetriever`、`MemoryQuery`、`MemoryRecallHit` |
| 分类 | — | ✅ `DefaultMemoryClassifier` |
| 索引 | — | ✅ `DefaultMemoryIndexer` |
| 生命周期 | — | ✅ `DefaultMemoryLifecycle`、`MemoryLifecycle` |
| 持久化 | — | ✅ `SqliteMemoryStore` |
| Event | — | ✅ `MemoryEvent`/`MemoryEventKind`（与 Event Runtime 集成） |

**结论：✅ 已实现，比预期完整（有分类/索引/检索/Event 集成）。**

---

### 6. Plan — 计划

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-planner | `core-agent-plan` |
| 结构 | Goal → Step → Action | ✅ Goal/Task/Step/Action 四层结构 |
| 依赖 | 任务依赖 | ✅ `PlanningGraph`/`PlanningNode`/`PlanningEdge`/`PlanningRelation` |
| 审查 | — | ✅ `PlanReview`/`ReviewDecision`/`StructuralPlanReviewer` |
| 策略 | — | ✅ `DefaultPlanningStrategy`、`RulePlanBuilder` |
| 调度 | 非执行 | ✅ `TaskScheduler`（仅调度，不执行） |
| 持久化 | — | ✅ `SqlitePlanningStore`、`PlanSnapshotStore` |
| 快照 | — | ✅ `PlanSnapshot` |

**结论：✅ 已实现，比预期丰富（图依赖 + 审查 + 策略 + 快照）。**

---

### 7. Tools — 工具

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-tool | `core-agent-tool` |
| 内置工具 | 41 种 | ✅ 覆盖文件/搜索/Shell/Git/Web/代码分析/LSP/CRON/Agent/Ask/Plan 等 |
| 注册 | name + description + input/output + handler | ✅ `ToolDefinition`/`ToolProvider`/`ToolRegistration` |
| 权限 | — | ✅ `ToolPermission`/`PermissionDecision`/`ToolPolicy` |
| 执行 | — | ✅ `ToolExecutor`/`ToolManager`/`ToolLifecycle` |
| 结果映射 | — | ✅ `ToolResultMapper`/`RawToolOutput` |
| 持久化 | — | ✅ `SqliteToolStore` |
| 观察者 | — | ✅ `ToolObserver` |

**结论：✅ 已实现，41+ 内置工具，远超预期。**

---

### 8. Skills — 技能

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-skill | `src/guidance.rs`（在根 crate 中） |
| 定义 | Prompt + Tools + Workflow + Rules | ✅ `SkillDescriptor`/`SkillCatalog`/`SkillRoot` |
| 加载 | — | ✅ `LoadedSkill`/`InstructionChain`/`InstructionDocument` |
| 范围 | — | ✅ `GuidanceScope` |
| 预算 | — | ✅ `DEFAULT_INSTRUCTION_BUDGET_BYTES`/`DEFAULT_SKILL_FILE_LIMIT_BYTES` |
| 隔离 | 独立 Runtime | ❌ **未独立成 crate**，当前在根 crate 的 `guidance.rs` 中 |

**结论：⚠️ 基础实现，缺少独立 `core-agent-skill` crate。Skill 加载/解析/指令链已就绪，但未作为独立 Runtime 集成到 Kernel。**

---

### 9. MCP — 模型上下文协议

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-mcp | `src/mcp_runtime.rs`（在根 crate 中） |
| 客户端 | MCP Client | ✅ `McpClient`（stdio JSON-RPC 2.0） |
| 服务端发现 | — | ✅ `discover_mcp_servers()`（全局 + 项目配置） |
| 工具注册 | — | ✅ `McpToolProvider`：自动发现工具列表并注册 |
| 协议版本 | — | ✅ `2025-06-18` |
| 超时/取消 | — | ✅ 支持 CancellationToken + timeout |
| 隔离 | 独立 Runtime | ❌ **未独立成 crate**，当前在根 crate 中 |

**结论：⚠️ 功能完整，但未独立成 `core-agent-mcp` crate。**

---

### 10. Slash (/) — 命令入口

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-command | `src/interaction.rs`（根 crate） |
| 命令注册 | — | ✅ `InteractionCommandRegistry` |
| 命令解析 | — | ✅ `InteractionCommandInvocation` 解析器（支持转义引号） |
| 命令路由 | Entry/Runtime/Agent | ✅ 三种路由 + 只读标记 |
| 内置命令 | 20+ | ✅ `/help` `/new` `/clear` `/exit` `/profile` `/project` `/tasks` `/sessions` `/history` `/review` `/plan` `/explain` `/test` `/fix` `/refactor` `/commit` `/pr` `/config` `/status` `/tools` `/memory` `/undo` `/redo` |
| Tab 补全 | — | ✅ `complete()` 方法 |
| 隔离 | 独立 Runtime | ❌ **未独立成 crate**，当前在根 crate 中 |

**结论：⚠️ 功能完整，但未独立成 `core-agent-command` crate。**

---

### 11. @ (AT Mention) — 上下文指定

| 项目 | 预期 | 现状 |
|------|------|------|
| 模块 | core-mention | `src/interaction.rs`（根 crate） |
| 文件引用 | @file | ✅ `ContextMentionResolver` |
| 目录引用 | @directory/ | ✅ 递归展开目录 |
| 引号支持 | @"path with spaces" | ✅ |
| 限制 | — | ✅ 16 次提及 / 128 文件 / 256 KiB 单文件 / 1 MiB 总计 |
| 索引 | — | ✅ `ContextCandidateIndex`：ripgrep 优先 + 模糊搜索 |
| 隔离 | 独立 Runtime | ❌ **未独立成 crate**，当前在根 crate 中 |

**结论：⚠️ 功能完整，但未独立成 `core-agent-mention` crate。**

---

## 额外已实现模块

| 概念 | 模块 | 说明 |
|------|------|------|
| **Session** | `core-agent-session` | 会话生命周期、Conversation/Message/Attachment/Manifest、EventBus |
| **Execution** | `core-agent-execution` | Plan 执行引擎、步骤执行、重试/回滚/检查点/状态机 |
| **Workflow** | `core-agent-workflow` | 工作流定义、调度、进度、变量 |
| **Event** | `core-agent-event` | 进程内事件总线、路由、死信、重放 |
| **Extension** | `core-agent-extension` | 扩展清单、能力注册、Provider 管理、宿主隔离 |
| **Platform** | `core-agent-platform` | 多租户治理、策略引擎、配额、审计、健康中心 |
| **Config** | `core-agent-config` | 分层配置（默认→用户→项目→环境）、密钥解析 |
| **Workspace** | `core-agent-workspace` | 工作区/项目/资源/环境/图索引 |
| **Protocol** | `core-agent-protocol` | AgentOS 内部契约 0.1、协议注册/发现/兼容性检查 |
| **Collaboration** | `core-agent-collaboration` | 团队项目/任务/审查/知识资产/活动流 |
| **Ecosystem** | `core-agent-ecosystem` | 市场/发布者/包管理/评分/安装解析 |
| **Governance** | `core-agent-governance` | 企业治理（资产/成本/数据分类/身份） |
| **Visual** | `core-agent-visual` | UI 面板描述/注册表/Studio 面板 |
| **CLI** | `agent-cli` | 终端客户端（嵌入式/HTTP/专业模式/TUI） |
| **Desktop** | `agent-desktop` | Tauri 桌面端（Vue3 + Rust） |
| **App Contracts** | `core-agent-app` | 产品阶段定义（7 个阶段）、能力映射、阶段就绪评估 |

---

## 差距总结

### 1. 独立 crate 化未完成（3 项）

| 概念 | 当前位置 | 建议目标 |
|------|----------|----------|
| Skills | `src/guidance.rs` | `core-agent-skill` |
| MCP | `src/mcp_runtime.rs` | `core-agent-mcp` |
| Slash/Command | `src/interaction.rs` | `core-agent-command` |
| @ Mention | `src/interaction.rs` | `core-agent-mention` |

> 当前 `interaction.rs` 同时包含了 Slash 命令和 @ Mention 两个逻辑，应拆分为独立的 `core-agent-command` 和 `core-agent-mention`。

### 2. 命名的细粒度差异

| 预期名 | 实际名 | 影响 |
|--------|--------|------|
| `core-ai` | `core-agent-model` | 无，功能一致 |
| `core-agent-runtime` | `core-agent-agent` + `core-agent-kernel` | 无，拆分更合理 |
| `core-planner` | `core-agent-plan` | 无 |
| `core-memory` | `core-agent-memory` | 无 |
| `core-tool` | `core-agent-tool` | 无 |
| `core-context` | `core-agent-context` | 无 |
| `core-command` | 未独立 | 需拆分 |
| `core-mention` | 未独立 | 需拆分 |
| `core-mcp` | 未独立 | 需拆分 |
| `core-skill` | 未独立 | 需拆分 |

### 3. 实测覆盖

- ✅ 单元测试：各 crate 均含 `#[cfg(test)] mod tests`
- ✅ 端到端测试：`tests/` 目录下多个集成测试
- ✅ 桌面端：E2E 测试覆盖 Desktop 配置/工作区
- ✅ CLI：E2E 测试覆盖 CLI 运行时

---

## 一句话总结

**核心 11 个概念全部实现，其中 8 个已独立成 crate，3 个（Skills/MCP/Command+Mention）暂在根 crate 中。** 额外实现了 10+ 个模块（Session/Execution/Workflow/Event/Extension/Platform/Config/Workspace/Collaboration/Ecosystem），远超文档预期。当前主要差距在于将交互层（Command + Mention）和能力层（Skills + MCP）抽离为独立 crate。