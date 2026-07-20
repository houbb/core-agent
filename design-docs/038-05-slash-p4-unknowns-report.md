# Unknowns Report — Phase 4: Cognitive Runtime

## Metadata

- **Task / Feature:** Phase 4：Agent Cognitive Runtime（6 个 Slash 命令）
- **Mode:** Standard
- **Date:** 2026-07-20
- **Scope:** 新增 `/reason`, `/think`, `/hypothesis`, `/critic`, `/reflect`, `/decision` 命令 + Cognitive Engine + ADR 生成

## Intent

### User-visible problem
当前 Agent 只是"任务执行器"，不具备分析、反思、决策能力。Phase 4 让 Agent 具备认知能力。

### Desired behavior change
用户可以通过 6 个新 slash 命令触发 Agent 的认知能力：问题分析、复杂任务推理、假设管理、自我批判、反思学习、决策记录。

### Affected users and workflows
- CLI/TUI 用户通过 `/reason`, `/think` 等命令直接使用
- 输出格式为结构化推理摘要（非 CoT 思维链）
- `/decision` 自动生成 ADR 到 `docs/adr/`
- `/reflect` 结果可写入 Memory

### Success criteria
- 6 个新命令可注册、可路由到 Agent 模型
- 每个命令输出结构化分析结果
- `/decision` 生成 `docs/adr/NNN-title.md` 文件
- 单元测试覆盖

### Non-goals
- 不暴露 CoT（Chain of Thought）
- 不做自动触发（Phase 5 workflow 触发）
- 不做 Agent Society 深度集成（Phase 3）

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | `src/interaction.rs` | 现有 Agent 命令注册模式（/plan, /review） | High |
| Code | `src/enterprise.rs` | `run_with_approval_inner` 处理 Agent 路由命令 | High |
| Code | `src/slash/mod.rs` | `SlashCategory` 枚举 + 插件式命令注册 | High |
| Code | `src/slash/commands/*.rs` | 现有 Runtime 路由命令实现模式 | High |
| Code | `src/lib.rs` | 模块导出 + 多 Phase 共存 | High |
| Design | `design-docs/038-05-slash-p4-agent-conitive.md` | 完整设计文档 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Agent 路由命令通过 `InteractionCommandRoute::Agent` 注册 | `interaction.rs:130-408` | 6 个新命令同样注册 |
| `run_with_approval_inner` 处理所有 Agent 路由命令 | `enterprise.rs:1326-1822` | 认知命令自动进入模型循环 |
| `is_read_only()` 控制命令是否可写 | `interaction.rs:77-83` | `/reason`/`/think`/`/hypothesis`/`/critic` 应只读 |
| `model_prompt()` 生成模型提示词 | `interaction.rs:101-121` | 认知命令需要定制提示词 |
| 现有命令注册在 `with_builtins()` 中 | `interaction.rs:130-408` | 新命令追加到同一列表 |
| ADR 目录没有预定义位置 | 代码扫描 | `/decision` 需创建 `docs/adr/` |

## Critical Unknowns

| Unknown | Category | Impact | Probability | Priority | Disposition | Resolution |
|---|---|---|---|---|---|---|
| ADR 写入失败时如何处理 | Known unknown | 3 | 3 | Medium | Decision | 在 cognitive engine 中优雅处理，返回错误信息到响应 |
| `/reflect` 写入 Memory 的接口 | Known unknown | 2 | 4 | Medium | Decision | 当前 MVP 不要求写入 Memory，只输出 Reflection 文本 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 6 个命令共享 Cognitive category | 设计文档明确分类 | 后续可拆分 |
| Agent 路由处理认知命令足够 | 现有 `/plan` 已证明模式可行 | 后续可升级为多步引擎 |
| ADR 写入 `docs/adr/` 目录 | 标准 ADR 惯例 | 可配置路径 |

## Implementation Boundary

### Implement now
- 6 个 Slash 命令注册 + SlashCategory
- Cognitive Engine（prompt 模板 + 结构化输出格式）
- ADR 自动生成 (`/decision` → `docs/adr/`)
- 单元测试

### Do not implement now
- 多步推理引擎（多个模型调用串联）
- Memory 自动写入（`/reflect` → Memory）
- Agent Society 深度集成

## Verification Plan

### Automated
- 单元测试：命令注册、参数验证、ADR 生成
- 集成测试：认知命令路由到 Agent 模型

### Manual
- 在 CLI/TUI 中测试每个命令
- 验证 `/decision` 生成 ADR 文件