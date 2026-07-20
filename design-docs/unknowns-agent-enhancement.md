# Unknowns Report — Agent 全面增强

## Metadata

- **Task / Feature:** Agent 全面增强（对标 Claude Code / ChatGPT / OpenCode）
- **Mode:** Standard
- **Date:** 2026-07-20
- **Scope:** `src/subagent_runtime.rs` + 新增 Slash 命令 + 相关测试

## Intent

### User-visible problem
当前 Agent 子类型只有 General / Explore / Review 三种，缺少 Test、Debug、SecurityReview、Doc、Migration、Architecture 等高频场景的专用 Agent。硬编码的 4 轮只读工具限制也无法满足这些场景需求。

### Desired behavior change
1. `SubAgentProfile` 从 3 个扩展到 10+ 个（P0: Test/Debug/SecurityReview, P1: Doc/Migration/Architecture）
2. 每个 Profile 可配置允许的工具集和最大轮数
3. 新增 `/test`、`/debug-agent`、`/security-review` 等 Slash 命令路由到对应 Profile
4. 向后兼容，已有 General/Explore/Review 行为不变

### Affected users and workflows
- 所有使用 `delegate_task` tool 的调用方
- Slash 命令系统（需新增命令注册）
- 测试套件（需适配新 Profile）

### Success criteria
- 每个新 Profile 有独立的 prompt + tool filter + max_turns
- 3 个 P0 Profile 完整实现并测试通过
- 已有 Profile 行为不变，测试全绿

### Non-goals
- 不修改 Multi-Agent 系统（`core-agent-multi/`）
- 不修改 `core-agent-agent/` 的完整 Agent Runtime
- 不涉及 UI/Desktop 变更
- 不实现 P2 的 Deploy/Research/Init/DataViz Agent

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|--------|----------|-----------------|:----------:|
| 代码 | `src/subagent_runtime.rs` | 当前 3 个 Profile，硬编码 4 轮 + 4 类只读工具 | High |
| 代码 | `src/slash/mod.rs` | SlashCommandRegistry 支持插件式注册 | High |
| 代码 | `src/slash/commands/mod.rs` | 现有 26 个 Slash 命令模块 | High |
| 代码 | `src/slash/society_plugin.rs` | 批量注册模式（参考 SocietyCommandPlugin） | High |
| 代码 | `src/lib.rs` | 需要确认 SubAgentRuntime 的导出路径 | Medium |
| 文档 | `design-docs/039-agent-vs-cc.md` | 完整对标分析和实现计划 | High |
| 文档 | `docs/current/current-agent-v20260720.md` | 现状分析 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|------|----------|-----------|
| `SubAgentProfile` 是 `#[derive(Clone, Copy)]` 枚举，用于 `fn run()` 参数 | `subagent_runtime.rs:18-24` | 扩展需保持 `Clone + Copy` |
| 工具过滤由 `subagent_tool_allowed()` 函数硬编码 | `subagent_runtime.rs:253-259` | 需改为 Profile 级配置 |
| 最大轮数硬编码为 4 | `subagent_runtime.rs:114` | 需改为 Profile 级配置 |
| Prompt 由 `fn prompt()` 返回 &'static str | `subagent_runtime.rs:38-45` | 扩展需同步增加 |
| 工具定义 JSON Schema 中 profile 枚举为 `["general", "explore", "review"]` | `subagent_runtime.rs:215` | 需同步扩展 |
| 已有 `/debug` 命令是 Trace 分析（非 Agent），`/architecture` 是架构图查看 | `debug.rs`, `architecture.rs` | 新增 Agent 不冲突 |
| `SubAgentProfile` 实现了 `Serialize/Deserialize` | `subagent_runtime.rs:18` | 扩展保持 Serde 兼容 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---------|----------|---------------------|:-----:|:-----------:|:---------------:|:------------------:|:--------:|:-----------:|:-----------|
| 新 Profile 的工具集是否满足实际使用场景 | Known unknown | 未经验证，Test/Debug 可能需要更多工具 | 4 | 3 | 2 | 4 | 96 | Experiment | 实现后通过端到端测试验证 |
| `lib.rs` 是否公开导出 `SubAgentProfile` | Unknown known | 需确认外部 crate 是否依赖 | 3 | 2 | 3 | 3 | 54 | Accept | 保持 pub 导出，注意兼容性 |
| Slack 命令命名冲突：`/debug` 已存在（Trace 分析） | Known known | `debug.rs` 已占用 | 4 | 5 | 1 | 2 | 40 | Decision | 新 Agent 用 `/debug-agent` 或 `/agent-debug` |
| 与 `core-agent-agent` 的 AgentProfile 关系未明确 | Unknown known | 文档说"SubAgent 是轻量级隔离模型上下文" | 2 | 3 | 3 | 3 | 54 | Accept | 保持独立，不耦合 |
| 任务/输出大小限制对 Test/Debug 是否足够 | Known unknown | 16KiB/128KiB 可能不够容纳测试输出 | 3 | 3 | 1 | 4 | 36 | Experiment | 实现后观测，必要时调大 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|-------------|-----------------|-------------------|
| 新 Agent 应与现有 Slash 命令系统无缝集成 | 现有 `/plan`, `/review` 等已占位 | 遵循 SocietyCommandPlugin 的批量注册模式 |
| 用户期望通过 `/test` 直接触发 Test Agent | Claude Code 有 `/test` 命令 | 实现时注册 `/test` 到 Test Profile |
| 已有 Profile 的行为完全不变 | 现有用户依赖 General/Explore/Review | 不改动已有 Profile 的 prompt/tools/turns |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|----------|---------|------------|-------------------|-------------------|
| 新 Slash 命令命名 | ① `/debug-agent` ② `/agent-debug` ③ 复用 `/debug` 并扩展 | ③ 破坏现有 `/debug` 行为；①② 命名清晰但多一个记忆成本 | User | 实现前确认 |
| Test/Debug 的 max_turns 值 | ① 8 轮 ② 12 轮 ③ 16 轮 | 轮数多→更灵活但更贵；轮数少→省钱但可能不够 | Architecture | 实现时确认 |
| Tool filter 配置方式 | ① 硬编码 match ② 配置文件 ③ 函数指针 | ① 最简单 ② 灵活但过度设计 ③ Rust 惯用 | Architecture | 推荐 ① 先硬编码，后续可配置化 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|------------|---------------|---------------|
| 新 Profile 保持 `SubAgentProfile` 枚举的 `Clone + Copy` | 不破坏现有模式 | 去掉 Copy 仅影响少量 match |
| 新增 Profile 在 `SubAgentProfile::parse` 中增加分支 | 当前已用 match 模式 | 去掉分支即报错，编译期可发现 |
| 工具定义 JSON Schema 的 enum 同步扩展 | 否则 LLM 无法调用新 Profile | 回退 schema 即可 |

## Recommended Implementation Boundary

### Implement now
1. `SubAgentProfile` 枚举扩展：Test / Debug / SecurityReview / Doc / Migration / Architecture
2. `allowed_categories()` 方法 — 每个 Profile 独立工具集
3. `max_turns()` 方法 — 每个 Profile 独立最大轮数
4. 更新 `prompt()` 方法 — 每个 Profile 独立 System Prompt
5. 更新 `parse()` 方法 + tool JSON Schema 的 enum
6. 更新 `subagent_tool_allowed()` → 使用 `allowed_categories()`
7. 注册 `/test`、`/debug-agent`、`/security-review` 三个 Slash 命令
8. 单元测试 + 端到端测试

### Do not implement now
- P2 Agent（Deploy / Research / Init / DataViz）
- 基于配置文件的 Profile 注册机制（先硬编码，观察需求）
- 修改 `core-agent-agent/` 或 `core-agent-multi/`

### Interfaces or data contracts to freeze
- `SubAgentProfile` 枚举 serde 命名（`snake_case`）
- `delegate_task` tool 的 JSON Schema（profile enum 扩展）
- `SubAgentOutcome` 结构体字段

## Verification Plan

### Automated
- 单元测试：每个 Profile 有独立的 prompt 且不为空
- 单元测试：每个 Profile 的 `allowed_categories()` 包含 `filesystem.read`
- 单元测试：parse 拒绝未知 profile 名称
- 单元测试：Test/Debug profile 的 max_turns > 4
- 端到端测试：`delegate_task` 工具调用新 Profile 返回正确结果
- 端到端测试：`/test` 命令正确路由到 Test Agent

### Manual
- Happy path：调用已存在的 Test/Debug Agent 正常返回
- Error path：传入未知 profile 名称报错清晰