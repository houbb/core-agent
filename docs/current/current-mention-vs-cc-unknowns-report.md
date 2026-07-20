# Unknowns Report

## Metadata

- **Task / Feature:** `@` 上下文模块对标 Claude Code 能力分析
- **Mode:** Standard
- **Date:** 2026-07-20
- **Prepared by:** Core Agent
- **Scope:** `@` 上下文引用（选中文件、选中文件内容、选中历史对话、历史对话内容）的现状 vs Claude Code 对标分析

---

## Intent

### User-visible problem

当前 `@` 上下文引用已有基础能力，但与 Claude Code 的 `@` 引用体验存在体验差距：Claude Code 中 `@` 引用的文件支持**点击跳转**到对应位置、输出中**可视化渲染**引用上下文、对话历史可**引用选中消息**。我们的 `@` 引用目前仅用于输入补全，输出侧无任何交互能力。

### Desired behavior change

对标 Claude Code 的 `@` 上下文体验，让 `@` 引用在**输入侧**和**输出侧**都具备直观、准确、美观、可交互的能力：
- 输入侧：`@` 补全支持文件/目录/历史消息，选择后显示为 Context Chip
- 输出侧：Agent 回复中引用的文件路径支持点击跳转（IDE / 终端文件打开）
- 引用侧：支持选中文件范围、历史消息作为上下文引用

### Affected users and workflows

- **所有用户**：每次使用 `@` 引用文件或上下文时
- Desktop 用户：对话中看到文件引用可点击跳转
- CLI TUI 用户：`@` 引用在输出中显示为可交互的文件链接

### Success criteria

1. `@` 引用在输入时能搜索并补全文件/目录/行范围
2. `@` 引用在输出中显示为可点击的文件链接（Desktop 支持打开文件，CLI 支持打开编辑器）
3. 选中的文件内容片段支持作为上下文引用
4. 历史消息支持选中引用

### Non-goals

- 不涉及 IDE 插件（VS Code / JetBrains）的集成
- 不涉及 AST/Symbol 级别的引用
- 不涉及 Terminal Context 引用

---

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|--------|----------|-----------------|------------|
| Code | `src/interaction.rs` | `@` 解析（`parse_mentions`）、模糊搜索（`ContextCandidateIndex`）、`ContextMentionResolver` 文件内容解析 | High |
| Code | `core-agent-context/src/domain/context_reference.rs` | `ContextReference` 三种类型（File/Selection/Message）、`ContextPackage` 聚合 | High |
| Code | `core-agent-context/src/application/service.rs` | Reference 的 CRUD 和 Context 构建流程 | High |
| Code | `core-agent-context/src/application/composer.rs` | `DefaultComposer` 处理 `Reference` Slot 并入 Context | High |
| Code | `core-agent-context/src/persistence/providers/conversation_provider.rs` | `ConversationProvider` 支持 Message 引用解析 | High |
| Code | `agent-desktop/src/App.vue` | Desktop 前端 `@` 补全实现、消息渲染（纯文本，无可点击链接） | High |
| Code | `agent-desktop/src/prompt-completion.ts` | `mentionQueryAtCursor` / `contextCompletions` / `commandCompletions` | High |
| Code | `agent-cli/src/tui.rs` | CLI TUI 的 `@` 补全、消息渲染（纯文本，无文件链接） | High |
| Design doc | `design-docs/037-context-comment.md` | Context Annotation 设计蓝图（含 Context Chip 等 UX 设计） | High |
| Code | `agent-desktop/src/types.ts` | 前端类型定义（无 `ContextReference` 相关类型） | High |

---

## Confirmed Facts

| Fact | Evidence | Relevance |
|------|----------|-----------|
| `@` 输入补全已实现，支持文件/目录模糊搜索 | `src/interaction.rs:ContextCandidateIndex` + `tui.rs:mention_at_cursor` + `prompt-completion.ts:mentionQueryAtCursor` | 核心输入能力就绪 |
| `ContextReference` 领域模型完整（File/Selection/Message） | `core-agent-context/src/domain/context_reference.rs` | 数据模型就绪 |
| `ReferenceProvider` 支持 Message 引用解析 | `core-agent-context/src/persistence/providers/conversation_provider.rs` | 消息引用能力就绪 |
| `ReferenceStore` 支持持久化 | `core-agent-context/src/persistence/reference_store.rs` | 数据持久化就绪 |
| Desktop 消息渲染为纯文本，无文件链接 | `agent-desktop/src/App.vue:391-393` (`<p>{{ item.content }}</p>`) | 输出侧体验缺失 |
| CLI TUI 消息渲染为纯文本，无文件链接 | `agent-cli/src/tui.rs:630-667` (`render_messages` 纯文本) | 输出侧体验缺失 |
| 无 Context Chip 组件 | `agent-desktop/src/` 无相关组件 | UX 缺失 |
| 无 `open_file` 工具或文件跳转机制 | `agent-cli/src/` 无相关代码 | 交互能力缺失 |

---

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---------|----------|---------------------|:------:|:-----------:|:---------------:|:-------------------:|:--------:|:------------|-----------|
| 输出侧 `@` 引用如何渲染为可点击链接？ | Known unknown | 当前 `App.vue` 消息渲染为 `<p>{{ item.content }}</p>`，无文件路径解析 | 5 | 5 | 2 | 3 | 150 | **Decision** | 需确定：Desktop 用 `open` Tauri API 打开文件 + 行号；CLI 用 `code --goto` 或自定义协议 |
| 是否需要后端返回 `ContextReference` 与消息内容关联？ | Known unknown | 当前消息内容为纯文本，Agent 回复中引用的文件路径需要解析结构 | 4 | 4 | 3 | 4 | 192 | **Decision** | 需确定是否在后端将引用结构化，还是前端后处理解析 |
| 选中的文件行范围如何在前端选择？ | Known unknown | Desktop 中用户选择代码后，如何获取行号并创建引用 | 4 | 5 | 2 | 2 | 80 | **Experiment** | 需要原型验证 Tauri 的 `selectedText` 能力 |
| 历史消息选中引用如何在前端交互？ | Known unknown | 现有 `@` 补全不支持历史消息，需新增 `@message` 或 `@session` | 4 | 4 | 3 | 3 | 144 | **Decision** | 需确定：`@message:<id>` 还是对话框选择 |
| 消息中 `@` 路径渲染后如何保证路径有效？ | Unknown known | 用户输入 `@` 引用工作区文件，但输出时文件可能已删除 | 3 | 3 | 2 | 2 | 36 | **Accept** | 合理行为：文件不存在时显示灰色不可点击 |
| Agent 回复中的文件引用是否自动转为链接？ | Unknown unknown | 当前 Agent 回复是纯文本，可能包含 `src/main.rs:42` 等格式，需要解析 | 4 | 4 | 2 | 3 | 96 | **Experiment** | 需要原型验证正则匹配路径模式转为链接 |

---

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|-------------|-----------------|-------------------|
| 点击 `@file` 链接应打开文件编辑器 | Claude Code 的 `@` 引用可以点击跳转 | 对比 Claude Code 行为 |
| 选中代码后弹出浮动菜单 | IDE 中常见的"选中→操作"模式 | 查看用户是否期待类似 VS Code 的 CodeLens |
| 消息中 `@` 引用的文件应有视觉高亮 | Claude Code 中 `@` 引用以不同颜色显示 | 对比 Claude Code 的渲染效果 |
| 当前上下文应显示为 Context Chip | Claude Code 和 ChatGPT 的 attachment 设计 | 对比已有设计文档 037 中的 Context Chip 设计 |

---

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|-----------|-------------------|-------------------|
| `@` 引用在多行代码选择时的精确行号获取 | Desktop 中选择代码段时可能跨行，需要精确的行范围 | 原型验证 |
| `@` 引用在 Agent 回复中的去重 | 多次引用同一文件时，应合并显示 | 代码审查 |
| Session 引用中涉及大量历史消息的 Token 控制 | 引用整个 Session 可能导致 Token 爆炸 | 压力测试 |
| 终端（CLI）模式下打开文件的 UI 交互 | 终端中点击文件链接需要 `code --goto` 或类似能力 | 原型验证 |

---

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|----------|---------|------------|-------------------|-------------------|
| 输出侧文件链接渲染方式 | 1) 前端正则解析 `@path:line` 转为链接 2) 后端结构化返回引用列表+消息内容 | 1) 前端解析简单但需约定格式 2) 后端结构可靠但协议复杂 | Architecture | 实现前 |
| 文件打开方式 | 1) `code --goto path:line` 2) Tauri `open` API 3) 自定义协议 | 1) CLI 友好 2) Desktop 友好 3) 通用但复杂 | Architecture | 实现前 |
| 历史消息引用方式 | 1) `@message:<id>` 语法 2) 侧边栏选择 3) 消息 hover 菜单 | 1) 语法级简洁 2) 交互直观 3) 最符合直觉 | UX | 实现前 |

---

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|----------|--------|----------------|:----:|-------|
| Desktop 中选中代码能否获取行号？ | Tauri API 原型 | 能获取 selection 的行范围 | Low | Dev |
| 消息中 `@` 路径正则匹配的准确率？ | 代码实验 | 90%+ 准确匹配工作区文件路径 | Low | Dev |
| CLI 打开文件链接的用户体验？ | 原型验证 | 用户能接受 `code --goto` 的延迟 | Low | Dev |

---

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|------------|----------------|---------------|
| 可先用 `@file:line` 格式约定输出侧引用 | 前端格式约定易于修改，无后端侵入 | 改为后端结构化返回 |
| Desktop 使用 Tauri shell 打开文件 | Tauri 已集成，无需额外依赖 | 降级为复制路径到剪贴板 |
| CLI 使用 `code --goto` 打开文件 | 广泛支持，VS Code 用户为主 | 降级为打印路径 |

---

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---------|-------------|----------------------|
| AST/Symbol 级别引用 | 超出当前对标范围 | Phase 2 考虑 |
| Terminal Context 引用 | 超出当前对标范围 | Phase 2 考虑 |
| 企业级数据引用 | 超出当前对标范围 | Phase 3 考虑 |

---

## Recommended Implementation Boundary

### Implement now

1. **输出侧 `@` 引用渲染** — 消息中 `@path` 转为可点击链接
2. **文件跳转机制** — Desktop 用 Tauri open，CLI 用 `code --goto`
3. **Context Chip** — 当前上下文以 Chip 形式显示在输入框上方
4. **选中文件范围引用** — 支持 `@file:start-end` 语法

### Do not implement now

- AST/Symbol 级别引用
- Terminal Context 引用
- 企业数据 Context 引用

### Interfaces or data contracts to freeze

- `ContextReference` 领域模型已稳定
- `ContextPackage` 接口已稳定

### Areas that must remain reversible

- 前端 `@` 路径解析正则表达式（可调整匹配规则）
- Context Chip 渲染样式（可调整视觉风格）

---

## Verification Plan

### Automated

- 单元测试: `@` 路径正则匹配、`ContextReference` 序列化
- 集成测试: Desktop 消息渲染含文件链接
- 快照测试: CLI TUI 渲染含文件链接

### Manual

- Happy path: 输入 `@file` → 补全 → 发送 → 回复含可点击链接
- Empty state: 引用的文件已被删除 → 显示灰色不可点击
- Failure path: 文件路径格式错误 → 不渲染为链接
- 终端: 点击链接 → `code --goto` 打开文件

### Observability

- 日志: 记录用户点击文件链接的操作
- 指标: 文件链接点击率、引用使用率

---

## Handoff

Convert resolved findings into:

- [ ] 输出侧 `@` 引用渲染（正则解析 + 可点击链接）
- [ ] Context Chip 组件（输入框上方显示当前引用）
- [ ] 文件跳转机制（Desktop Tauri open / CLI `code --goto`）
- [ ] 选中文件范围引用（`@file:start-end` 语法）
- [ ] 历史消息引用（`@message:<id>` 语法支持）
- [ ] 对比文档（`docs/current/` 下）