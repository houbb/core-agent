# core-agent vs OpenAI Codex CLI 差距分析

> 梳理 core-agent 和 OpenAI Codex CLI（[github.com/openai/codex](https://github.com/openai/codex)）之间的差距。
>
> 对应设计文档：`design-docs/046-margin-to-codex.md`

---

## 评分标准

- ✅ **已对齐**：已有完整实现，差距很小
- ⚠️ **部分对齐**：有基础实现，但缺少关键功能
- ❌ **未对齐**：没有或仅有极简实现

---

## 维度一：运行时能力（Runtime Capabilities）⭐⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 文件编辑（Read/Write/Edit） | ✅ FileEdit 工具 | ✅ `file.read/write/edit/patch` | ✅ 基本对齐 | - |
| Shell 执行 | ✅ Bash 工具 + 沙箱隔离 | ✅ `run_command` + `start/poll/cancel` | ⚠️ Codex 沙箱更完善（Docker 容器级隔离） | P1 |
| Git 操作 | ✅ 自动 commit + 分支管理 | ✅ `git.*` 7 个工具 | ⚠️ 缺少自动 commit 流程和 AI 驱动的分支管理 | P0 |
| 代码搜索 | ✅ Glob + Grep | ✅ `file.glob/grep` + LSP + AST | ✅ 更丰富（AST 工具） | - |
| 多文件协调编辑 | ✅ Plan → Multi-file Edit | ✅ `plan.*` + `apply_patch` | ⚠️ 缺少 Plan 驱动的多文件编辑流程 | P1 |
| AI 驱动 Git 工作流 | ✅ 自动 commit + PR | ❌ 无自动提交/PR 流程 | ❌ 缺少 AI 驱动的 Git 工作流 | P0 |
| 会话管理 | ✅ session 持久化 | ✅ `core-agent-session` | ✅ 基本对齐 | - |

### 关键差距：AI 驱动的 Git 工作流

Codex CLI 能自动完成 `git add → commit → push → PR` 全流程，且支持 AI 分支命名和 commit message 生成。core-agent 有完整的 git 工具但缺少自动编排。

**建议**：实现 `AutoGitWorkflow` 模块，自动编排 commit/branch/PR 流程。

---

## 维度二：安全沙箱（Sandbox）⭐⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 容器级沙箱 | ✅ Docker 容器隔离 | ⚠️ `bubblewrap`（Linux only） | ❌ 缺少 Docker/容器级沙箱 | P1 |
| 网络隔离 | ✅ 默认隔离 | ⚠️ `SandboxNetworkPolicy` | ⚠️ 已有策略但默认不启用 | P2 |
| 文件系统隔离 | ✅ 只读宿主机 | ⚠️ 路径验证 + 敏感目录拒绝 | ⚠️ 缺少完整文件系统隔离 | P2 |
| 沙箱超时/资源限制 | ✅ 有 | ⚠️ 超时 + 输出上限 | ⚠️ 缺少 CPU/内存资源限制 | P2 |
| macOS 沙箱支持 | ❌（Linux 优先） | ❌ 无 | ❌ macOS 无沙箱支持 | P3 |

### 关键差距：沙箱实现

Codex 使用 Docker 容器作为沙箱，提供完整的文件系统、网络、进程隔离。core-agent 的 bubblewrap 仅支持 Linux，且功能有限。

**建议**：增加 Docker 沙箱后端支持，作为 Linux 上 bubblewrap 的替代和增强。

---

## 维度三：CLI/TUI 体验 ⭐⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| REPL 终端体验 | ✅ 简洁 REPL | ✅ `agent chat` + TUI | ⚠️ TUI 已实现但缺少 diff 展示 | P1 |
| 文件 diff 展示 | ✅ 终端内 diff | ✅ 有限 | ⚠️ 缺少终端内文件 diff 可视化 | P1 |
| 流式输出 | ✅ Streaming | ✅ 支持 | ✅ 基本对齐 | - |
| 进度指示 | ✅ spinner | ✅ spinner + 状态栏 | ✅ 基本对齐 | - |
| 键盘快捷键 | ✅ 基本 | ✅ Ctrl+C/D/Shift+C | ⚠️ 缺少快捷键自定义 | P3 |
| 命令历史 | ✅ 有 | ✅ 有 | ✅ 基本对齐 | - |

### 关键差距：文件 diff 展示

Codex CLI 在终端内展示文件修改的 diff（绿色/红色高亮），让用户直观看到变更。core-agent 的 TUI 缺少 diff 渲染。

**建议**：在 TUI 中增加 diff 渲染，支持 `+`/`-` 行高亮。

---

## 维度四：Agent 编排能力 ⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 单 Agent 循环 | ✅ 模型 → 工具 → 回填 | ✅ 8 轮循环 | ✅ 基本对齐 | - |
| 多文件计划 | ✅ Plan → Edit | ✅ `plan.*` 工具 | ⚠️ 缺少 Plan → 多文件编辑的自动流程 | P1 |
| 工具调用 | ✅ function calling | ✅ ToolManager + 8 轮 | ✅ 基本对齐 | - |
| 错误恢复 | ✅ 重试机制 | ✅ Retry Policy | ✅ 基本对齐 | - |
| 子 Agent | ❌ 无 | ✅ 9 种 SubAgent Profile | ✅ core-agent 更强 | - |
| 多 Agent 编排 | ❌ 无 | ✅ 4 种策略 | ✅ core-agent 更强 | - |

### 关键差距：Plan → Multi-file Edit 流程

Codex 的 Plan 模式会自动生成多文件修改计划并逐个执行。core-agent 的 `plan.*` 工具定义了 Plan/Task/Step 但缺少自动执行流程。

---

## 维度五：IDE 集成 ⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| VS Code 扩展 | ✅ 原生扩展 | ❌ 无 | ❌ 缺少 VS Code 扩展 | P2 |
| Cursor 集成 | ✅ 支持 | ❌ 无 | ❌ 缺少 | P3 |
| Windsurf 集成 | ✅ 支持 | ❌ 无 | ❌ 缺少 | P3 |
| Desktop 应用 | ✅ `codex app` | ✅ `agent-desktop`（Tauri） | ✅ core-agent 更强 | - |

### 关键差距：IDE 扩展

Codex 有 VS Code 原生扩展，支持文件路径点击跳转、行号跳转。core-agent 目前没有 IDE 扩展。

---

## 维度六：模型和 API ⭐⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 默认模型 | ✅ o3 / o4-mini | ✅ OpenAI 兼容 | ⚠️ core-agent 支持任意 OpenAI 兼容模型 | - |
| 计费系统 | ✅ ChatGPT 计划计费 | ✅ `core-agent-cost` | ✅ core-agent 更灵活（企业级成本控制） | - |
| API Key 认证 | ✅ ChatGPT / API Key | ✅ `core-agent-config` | ✅ 基本对齐 | - |
| 多 Provider | ❌ 仅 OpenAI | ✅ 多 Provider 支持 | ✅ core-agent 更强 | - |
| 流式 | ✅ 支持 | ✅ 支持 | ✅ 基本对齐 | - |

---

## 维度七：企业级能力 ⭐⭐⭐⭐⭐

| 能力 | Codex CLI | core-agent | 差距 | 优先级 |
|------|-----------|------------|------|--------|
| 权限系统 | ⚠️ 基本 | ✅ 3 种模式 + 企业策略 | ✅ core-agent 更强 | - |
| 审计日志 | ❌ 无 | ✅ `core-agent-audit` | ✅ core-agent 更强 | - |
| 审批流程 | ❌ 无 | ✅ `core-agent-approval` | ✅ core-agent 更强 | - |
| 成本控制 | ✅ ChatGPT 计划 | ✅ `core-agent-cost` | ✅ 基本对齐 | - |
| 多租户 | ❌ 无 | ✅ `core-agent-platform` | ✅ core-agent 更强 | - |
| 治理策略 | ❌ 无 | ✅ `core-agent-governance` | ✅ core-agent 更强 | - |

---

## 优先级汇总

### P0：必须补齐（严重缺失，直接影响核心体验）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 1 | **AI 驱动的 Git 工作流** | 缺少自动 commit/branch/PR 流程 | 实现 `AutoGitWorkflow`：自动分支管理、commit message 生成、PR 创建 |
| 2 | **终端内 Diff 展示** | 文件变更不可见 | 在 TUI 中渲染 diff（+/- 行高亮） |

### P1：重要补齐（影响用户体验，有变通方案）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 3 | **Docker 沙箱后端** | 缺少容器级沙箱 | 增加 Docker 沙箱后端作为 bubblewrap 替代 |
| 4 | **Plan → Multi-file Edit** | 缺少自动多文件编辑流程 | 实现 Plan 驱动的多文件修改自动编排 |
| 5 | **Sandbox 资源限制** | 缺少 CPU/内存限制 | 增加沙箱资源限制参数 |

### P2：值得补齐（提升体验，可替代）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 6 | **VS Code 扩展** | 缺少 IDE 集成 | 开发 VS Code 扩展 |
| 7 | **文件系统隔离增强** | 沙箱安全不足 | 增强文件系统隔离策略 |

### P3：可暂缓（锦上添花）

| 排名 | 差距 | 影响 | 说明 |
|------|------|------|------|
| 8 | **macOS 沙箱** | macOS 无沙箱 | 实现 macOS 沙箱支持 |
| 9 | **快捷键自定义** | 提升操作效率 | 实现 keybindings.json 支持 |
| 10 | **更多 IDE 集成** | 拓展覆盖 | Cursor/Windsurf 集成 |

---

## 核心结论

### core-agent 的独特优势（Codex 没有的）

1. **企业级治理**：审计、审批、成本、多租户、治理策略 → 绝不能砍
2. **多 Agent 编排**：4 种策略（Sequential/Parallel/Supervisor/Debate）
3. **9 种 SubAgent Profile**：比 Codex 的 Agent 类型更丰富
4. **Desktop 应用**：Tauri 桌面端，Codex 的 `codex app` 是 Web 套壳
5. **AST 工具**：语言感知的代码搜索和替换
6. **认知命令**：reason、think、hypothesis、critic、reflect、decision
7. **多 Provider 支持**：任意 OpenAI 兼容模型，不绑定单一厂商

### core-agent 需补齐的关键差距（Codex 有但 core-agent 没有的）

1. **AI 驱动的 Git 工作流** → 自动 commit + branch + PR
2. **终端内 Diff 展示** → 文件变更可视化
3. **Docker 沙箱后端** → 容器级安全隔离
4. **Plan → Multi-file Edit** → 自动多文件编辑
5. **VS Code 扩展** → IDE 生态

### 核心策略建议

**不要试图完全复制 Codex。** core-agent 的企业级能力和多 Agent 编排是 Codex 没有的差异化优势。差距补齐应聚焦"让用户能丝滑使用"的体验层面。

建议优先级策略：
1. **P0 补齐**（AI Git 工作流 + Diff 展示）→ 1-2 天
2. **P1 补齐**（Docker 沙箱 + Plan 多文件编辑 + 资源限制）→ 3-5 天
3. **P2 补齐**（VS Code 扩展 + 文件系统隔离）→ 1-2 周