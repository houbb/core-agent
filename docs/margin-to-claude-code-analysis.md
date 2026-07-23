# core-agent vs Claude Code 差距分析

> 梳理 core-agent 和 Claude Code 之间的差距，按优先级排序，整理实现建议。
>
> 对应设计文档：`design-docs/045-margin-to-claude-code.md`

---

## 评分标准

- ✅ **已对齐**：已有完整实现，差距很小
- ⚠️ **部分对齐**：有基础实现，但缺少关键功能
- ❌ **未对齐**：没有或仅有极简实现

---

## 维度一：运行时能力（Runtime Capabilities）⭐⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| Memory（持久化记忆） | CLAUDE.md + MEMORY.md 索引 | ✅ `core-agent-memory` + `memory_tools.rs` | ⚠️ 缺少 `CLAUDE.md` 自动发现和 `.claude/memory/MEMORY.md` 索引机制 | P0 |
| Context（上下文构建） | @ 语法 + 自动文件引用 | ✅ `core-agent-context` + `@` 补全 | ⚠️ 缺少 Context Chip UI（标签显示引用文件、Token 估算）和选中代码引用 | P1 |
| Plan（计划模式） | Plan Mode + read-only + 用户审批 | ✅ `core-agent-plan` + `plan_mode` | ⚠️ Plan Mode 已有，但缺少用户可见的 Task/Step 层次结构预览和审批交互 | P1 |
| LLM 调用 | 内置 Claude | ✅ `core-agent-model` | ⚠️ 只支持 OpenAI 兼容协议，缺少 Claude/Gemini 原生 Provider | P2 |
| 文件修改 | Read + Write + Edit(old→new) | ✅ `file.read/write/edit/patch` | ✅ 基本对齐 | - |
| 代码搜索 | Glob + Grep + LSP | ✅ `file.glob/grep` + LSP + AST | ✅ 基本对齐，且 AST 工具更强 | - |
| 定时任务 | CronCreate/CronDelete | ✅ `cron.create/list/delete` | ✅ 基本对齐 | - |
| 后台任务 | Background Tasks | ✅ `BackgroundCommandManager` | ✅ 基本对齐 | - |

### 关键差距：Memory 文件索引

core-agent 的 Memory 是 SQLite 持久化，但没有像 Claude Code 那样的 `CLAUDE.md` + `MEMORY.md` 文件级索引机制。Claude Code 的 memory 是文件目录 + 索引文件，更轻量、更透明。

**建议**：在 `.claude/memory/` 目录下实现 MEMORY.md 索引机制，与 SQLite memory 并存。

---

## 维度二：CLI/TUI 体验 ⭐⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| TUI 交互 | 全屏 TUI + 状态栏 + 输入框 | ✅ `agent-cli` + TUI | ⚠️ TUI 已实现，但缺少状态栏（模型/权限/模式指示） | P1 |
| 文件路径点击 | 支持 VS Code 跳转 | ✅ 支持 | ⚠️ 缺少代码块来源标注（\`\`\`lang:path） | P2 |
| @ 上下文引用 | @file、@file:42、@directory/ | ✅ `@` 补全 + 行号 | ⚠️ 缺少 Context Chip 引用标签和选中代码引用 | P1 |
| / 命令面板 | 补全列表 + 分组 | ✅ `SlashCommandRegistry` | ⚠️ 缺少 `/tool:` 和 `/skill:` 快捷入口的 TUI 展示 | P2 |
| 键盘快捷键 | 自定义 keybindings.json | ❌ 无 | ❌ 缺少快捷键系统 | P3 |
| 撤销/重做 | Undo/Redo | ✅ 已有 Checkpoint | ✅ 基本对齐 | - |
| 流式输出 | Streaming | ✅ 支持 | ✅ 基本对齐（但需确认 TUI 中使用） | - |

### 关键差距：TUI 状态栏

Claude Code 的 TUI 底部有状态栏显示当前模型、权限模式、运行状态。core-agent 的 TUI（`agent-cli/src/tui/`）目前没有实现类似的状态指示。

---

## 维度三：Tool 和 MCP 体系 ⭐⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| MCP 客户端 | 原生支持 | ✅ `core-agent-mcp` + `McpRuntime` | ⚠️ 已实现 stdio JSON-RPC 传输和工具发现，但缺少 MCP Resources 和 Prompts 支持 | P1 |
| MCP 配置 | settings.json 中声明 | ✅ `core-agent-config` + `mcp.json` | ⚠️ 缺少 `settings.local.json` 的本地配置覆盖机制 | P2 |
| 工具权限 | Allow/Ask/Deny + 覆盖 | ✅ `PermissionDecision` + `ManagedAgentPolicy` | ✅ 基本对齐 | - |
| 工具注册 | 插件式 | ✅ `ToolProvider` + `BuiltinToolProvider` | ✅ 基本对齐 | - |
| Notebook 编辑 | NotebookEdit 工具 | ❌ 无 | ❌ 缺少 Jupyter Notebook 支持 | P3 |
| Web Search | WebSearch + WebFetch | ✅ `web_runtime.rs` + `OpenAiWebSearchProvider` | ✅ 基本对齐 | - |

### 关键差距：MCP Resources 和 Prompts

Claude Code 的 MCP 支持 3 种能力：Tools（可调用）、Resources（数据资源）、Prompts（模板）。core-agent 目前只实现了 MCP Tools 的发现和调用，缺少 Resources 和 Prompts 的支持。

---

## 维度四：Agent 编排能力（SubAgent / Workflow）⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| SubAgent | spawn/send/list | ✅ `core-agent-subagent` + 9 种 Profile | ✅ 基本对齐，且多 3 种 Profile | - |
| 多 Agent 编排 | Workflow 工具 | ✅ `core-agent-orchestrator`（4 种策略） | ✅ 基本对齐，且编排策略更丰富 | - |
| Agent 消息通信 | SendMessage | ✅ `core-agent-message`（Mailbox 模式） | ✅ 基本对齐 | - |
| 并行执行 | Agent 并行 | ✅ `OrchestratorManager` | ✅ 基本对齐 | - |
| Worktree 隔离 | Git 工作树隔离 | ❌ 无 | ❌ 缺少 Git 工作树隔离机制 | P2 |
| Agent SDK | 自定义 Agent 构建 | ✅ `core-agent-sdk`（AgentBuilder） | ✅ 基本对齐 | - |
| 角色分工 | 多个 Agent 类型 | ✅ 6 种 Agent 角色 + 9 种 SubAgent Profile | ✅ 更丰富 | - |

### 关键差距：Worktree 隔离

Claude Code 的 Worktree 可以在独立 Git 分支上修改，不污染主分支。适合多个 Agent 并行工作。core-agent 目前没有此能力。

---

## 维度五：配置和 Hook 体系 ⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 项目配置 | `settings.json` + `settings.local.json` | ✅ `core-agent-config` + `core-agent-config.yaml` | ⚠️ 缺少 `settings.local.json` 本地覆盖机制 | P2 |
| 钩子系统 | before/after/on_error 钩子 | ✅ `HookRuntime`（5 种事件） | ⚠️ 钩子事件种类较少，且需环境变量 `CORE_AGENT_ENABLE_HOOKS=1` 开启 | P1 |
| CLAUDE.md | 项目级指令 | ✅ `InstructionChain` | ⚠️ 缺少 `CLAUDE.md` 自动发现和加载 | P1 |
| Keybindings | keyboard shortcuts | ❌ 无 | ❌ 缺少快捷键系统 | P3 |
| 环境变量 | 覆盖配置 | ✅ 支持 | ✅ 基本对齐 | - |

### 关键差距：CLAUDE.md 自动发现

Claude Code 的 CLAUDE.md 是项目根目录下的指令文件，Agent 自动读取并遵循。core-agent 有 `InstructionChain` 和 `GuidanceScope` 系统，但缺少对 `CLAUDE.md` 文件的自动发现和加载。

---

## 维度六：外部集成 ⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| VS Code 扩展 | 原生扩展 | ❌ 无 | ❌ 缺少 VS Code 扩展 | P2 |
| JetBrains 插件 | 原生插件 | ❌ 无 | ❌ 缺少 JetBrains 插件 | P3 |
| Slack 集成 | Claude Tag | ❌ 无 | ❌ 缺少 Slack 集成 | P3 |
| MCP 生态 | 标准协议 | ✅ 已实现 | ⚠️ 缺少 Resources 和 Prompts 支持 | P1 |
| Web 搜索 | 内置 | ✅ 已实现 | ✅ 基本对齐 | - |
| 远程执行 | 远程模式 | ✅ `HttpAgentClient` | ✅ 基本对齐 | - |
| 桌面应用 | 无 | ✅ `agent-desktop`（Tauri） | ✅ core-agent 更强 | - |

### 关键差距：IDE 扩展

Claude Code 有 VS Code 和 JetBrains 的原生扩展，支持文件路径点击跳转、行号跳转、代码块来源标注。core-agent 目前没有 IDE 扩展，仅在 Desktop 模式中支持文件跳转。

---

## 维度七：企业级能力 ⭐⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 权限系统 | Allow/Ask/Deny | ✅ 3 种模式 + ManagedAgentPolicy | ✅ 基本对齐，且更丰富 | - |
| 审计日志 | 无 | ✅ `core-agent-audit` | ✅ core-agent 更强 | - |
| 审批流程 | 无 | ✅ `core-agent-approval` + RiskEngine | ✅ core-agent 更强 | - |
| 成本控制 | 无 | ✅ `core-agent-cost` + Budget | ✅ core-agent 更强 | - |
| 多租户 | 无 | ✅ `core-agent-platform` + Tenant | ✅ core-agent 更强 | - |
| 治理策略 | 无 | ✅ `core-agent-governance` | ✅ core-agent 更强 | - |
| 可观测性 | 无 | ✅ `core-agent-evaluation` + Trace | ✅ core-agent 更强 | - |
| 合规仪表盘 | 无 | ✅ ComplianceDashboard | ✅ core-agent 更强 | - |

### 核心发现

**企业级能力是 core-agent 对 Claude Code 的最大优势**。Claude Code 基本没有企业级能力，而 core-agent 已经实现了完整的 P9-P13 企业级平台，包括审计、审批、成本、多租户、治理、合规等。

---

## 维度八：可扩展性 ⭐⭐⭐⭐

| 能力 | Claude Code | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| Skills | 用户可调用指令包 | ✅ `core-agent-skill` + `SkillCatalog` | ✅ 基本对齐 | - |
| Slash 命令 | 内置命令 | ✅ 14 分类 + 插件式 `SlashCommand` | ✅ 基本对齐，且分类更完善 | - |
| 自定义 Agent | Agent SDK | ✅ `core-agent-sdk`（AgentBuilder） | ✅ 基本对齐 | - |
| 插件系统 | 无 | ✅ `core-agent-plugin` + Extension | ✅ core-agent 更强 | - |
| 市场 | 无 | ✅ `core-agent-marketplace` | ✅ core-agent 更强 | - |
| 开发者平台 | 无 | ✅ `core-agent-developer` + `core-agent-openapi` | ✅ core-agent 更强 | - |

---

## 汇总优先级排序

### P0：必须补齐（严重缺失，直接影响核心体验）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 1 | **CLAUDE.md 自动发现** | 项目级指令无法自动加载 | 实现 `InstructionChain` 对 `CLAUDE.md` 的自动发现 |
| 2 | **Memory 文件索引机制** | 缺少轻量级文件级记忆 | 实现 `.claude/memory/MEMORY.md` 索引 |
| 3 | **TUI 状态栏** | 缺少模型/模式/状态指示 | 在 TUI 底部增加状态栏 |

### P1：重要补齐（影响用户体验，有变通方案）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 4 | **Context Chip UI** | 引用标签显示不直观 | 在输入框上方显示引用标签 |
| 5 | **MCP Resources + Prompts** | MCP 协议支持不完整 | 扩展 McpClient 支持 Resources 和 Prompts |
| 6 | **Plan Mode 用户审批交互** | 缺少计划预览和审批 | 增强 plan create/approve/reject 交互 |
| 7 | **Hook 事件完善** | 钩子事件数量少 | 增加 BeforeTool/AfterTool/ToolFailure 事件的工具级过滤 |
| 8 | **选中代码引用** | 无法快速引用代码段 | 实现选中代码 → 添加到上下文 |

### P2：值得补齐（提升体验，可替代）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 9 | **Worktree 隔离** | 无法并行隔离开发 | 实现 Git 工作树隔离机制 |
| 10 | **VS Code 扩展** | 缺少 IDE 集成 | 开发 VS Code 扩展 |
| 11 | **settings.local.json** | 本地配置覆盖不便 | 增加本地配置覆盖机制 |
| 12 | **代码块来源标注** | 代码块来源不清晰 | 在 TUI 和 Desktop 中标注来源 |

### P3：可暂缓（锦上添花）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 13 | **键盘快捷键** | 提升操作效率 | 实现 keybindings.json 支持 |
| 14 | **Notebook 编辑** | 支持 Jupyter 场景 | 实现 NotebookEdit 工具 |
| 15 | **JetBrains 插件** | 拓展 IDE 覆盖 | 开发 JetBrains 插件 |
| 16 | **Slack 集成** | 外部通知 | 实现 Slack 集成 |

---

## 关键结论

### core-agent 的独特优势（Claude Code 没有的）

1. **企业级治理**：审计、审批、成本、多租户、治理策略 → 绝不能砍
2. **多 Agent 编排**：4 种策略（Sequential/Parallel/Supervisor/Debate）→ 比 Claude Code 更强
3. **9 种 SubAgent Profile**：比 Claude Code 的 Agent 类型更丰富
4. **Desktop 应用**：Tauri 桌面端，Claude Code 没有
5. **AST 工具**：语言感知的代码搜索和替换，比 Claude Code 更强
6. **认知命令**：reason、think、hypothesis、critic、reflect、decision → Claude Code 没有

### core-agent 需补齐的关键差距（Claude Code 有但 core-agent 没有的）

1. **CLAUDE.md 自动发现** → 用户需求最强烈
2. **Memory 文件索引** → 轻量级记忆机制
3. **TUI 状态栏** → 用户体验提升
4. **MCP 协议完整支持** → 生态兼容
5. **Worktree 隔离** → 并行开发安全
6. **IDE 扩展** → 生态拓展

### 核心策略建议

**不要试图完全复制 Claude Code。** core-agent 的企业级能力是 Claude Code 没有的差异化优势。差距补齐应聚焦"让用户能丝滑使用"的体验层面，而不是在 Agent 能力上全面对标。

建议优先级策略：

1. **P0 补齐**（CLAUDE.md + Memory 索引 + TUI 状态栏）→ 1-2 天
2. **P1 补齐**（Context Chip + MCP 扩展 + Plan 交互）→ 3-5 天
3. **P2 补齐**（Worktree + VS Code + 本地配置）→ 1-2 周
4. **P3 补齐**（快捷键 + Notebook + 更多集成）→ 长期