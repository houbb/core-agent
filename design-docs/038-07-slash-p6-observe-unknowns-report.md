# Unknowns Report

## Metadata

- **Task / Feature:** Phase 6 — Agent Observability & Evaluation Runtime
- **Mode:** Standard
- **Date:** 2026-07-20
- **Prepared by:** Claude
- **Scope:** 新增 6 个 Slash 命令（/trace-agent, /evaluate, /benchmark, /debug, /replay, /score）+ SQLite 存储 + CLI 集成

## Intent

### User-visible problem

Agent 执行过程不可见，出现问题时无法追溯原因，无法评估 Agent 质量，无法对比不同配置/模型的表现。

### Desired behavior change

Agent 从 Black Box 变为 Observable Intelligent System：
1. 每次执行生成完整 Trace（思考链、决策、工具调用）
2. 可对任务进行多维度质量评分
3. 可进行基准测试对比 Agent 能力
4. 可调试失败点、回放历史执行
5. 可查看 Agent 健康度仪表盘

### Affected users and workflows

- 终端用户：通过 `/trace-agent`, `/evaluate`, `/score` 查看执行详情
- 开发者：通过 `/debug`, `/replay` 调试 Agent 行为
- 平台管理员：通过 `/benchmark` 评估 Agent 能力基线
- 所有 Agent 执行路径（run_with_approval_inner → 自动采集 Trace）

### Success criteria

1. `/trace-agent` 展示 Agent 执行链（时间线 + 步骤）
2. `/evaluate` 输出多维度评分（Correctness/Safety/Efficiency/Maintainability）
3. `/benchmark` 运行内置任务集并输出统计
4. `/debug` 定位失败根因
5. `/replay` 基于事件溯源回放执行
6. `/score` 展示 Agent 健康度
7. 以上所有命令通过 CLI 和 TUI 均可访问
8. SQLite 持久化存储 Trace 数据

### Non-goals

- 不接入 OpenTelemetry / ClickHouse 等外部系统
- 不做 LLM Judge 打分（MVP 用规则打分）
- 不做可视化 Trace View（Desktop 端后续支持）
- 不与 core-audit 集成（只记录，不审计）

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Code | src/slash/mod.rs | 已有 SlashCommand trait 和 SlashCommandRegistry 插件架构 | High |
| Code | src/slash/commands/mod.rs | 现有 13 个内置命令，可直接新增 | High |
| Code | src/enterprise.rs | EnterpriseAgent::execute_command 处理零模型命令 | High |
| Code | src/enterprise.rs | run_with_approval_inner 是 Agent 执行核心，可注入 Trace 采集 | High |
| Code | agent-cli/src/command.rs | CliCommand enum 定义 CLI 子命令 | High |
| Code | agent-cli/src/main.rs | 已有 /trace 命令的 CLI 处理模式 | High |
| Code | src/slash/commands/trace.rs | 现有 /trace 是代码调用链追踪，与 /trace-agent 不冲突 | High |
| Design doc | design-docs/038-07-slash-p6-observe.md | 完整架构设计和数据模型 | Medium |
| Dependency | Cargo.toml | 已有 rusqlite (通过 core-agent-session 等)，无需新增依赖 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| 已有 SlashCategory 枚举，含 System/Session/Context/Project/Memory/Agent/Checkpoint/Governance/Developer 9 种 | src/slash/mod.rs:38-57 | 需要新增 Observability 分类 |
| SlashCommand trait 已支持 async execute + validate + metadata | src/slash/mod.rs:168-199 | 新命令直接实现此 trait |
| SlashCommandRegistry 已支持插件式注册 | src/slash/mod.rs:257-281 | 新命令注册方式已就绪 |
| EnterpriseAgent 已有 execute_command 处理零模型命令 | src/enterprise.rs:854-1220 | 新命令可在此处处理 |
| 已存在 /trace 命令（代码调用链追踪），与 /trace-agent 不同 | src/slash/commands/trace.rs | 不会冲突 |
| SQLite 是项目默认存储方式（session.db, context.db, model.db 等） | src/enterprise.rs:526-532 | Trace 存储沿用 SQLite |
| 无 core-audit crate 存在 | glob 搜索无结果 | 不集成 core-audit |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Priority | Disposition | Resolution |
|---|---|---|---|---|---|---|---|---|
| Trace 数据模型字段完整度 | Known unknown | 设计文档有 AgentTrace/TraceStep 原型，但未定义错误捕获、耗时统计等字段 | 4 | 3 | 2 | 3 | Decision | MVP 用最小字段，后续扩展（已确认） |
| Evaluation 规则打分具体算法 | Known unknown | 设计文档未定义 Correctness/Safety 等维度的具体打分规则 | 4 | 4 | 2 | 4 | Experiment | MVP 用简单的存在性/耗时/工具调用数等指标计算（已确认：规则打分） |
| Benchmark 内置任务集具体内容 | Known unknown | 设计文档未列出具体任务，需定义 5-10 个测试任务 | 3 | 4 | 1 | 3 | Experiment | 预置 5 个任务：代码修复、文档生成、架构分析、安全审查、测试生成（已确认） |
| Replay 事件溯源事件粒度 | Known unknown | 需要决定记录哪些事件（模型输入/输出、工具调用、决策点） | 4 | 3 | 2 | 3 | Decision | 记录所有模型请求+工具调用+决策，按顺序重放（已确认） |
| Trace 采集与 EnterpriseAgent 执行流程的 hook 点 | Known unknown | 需要在 run_with_approval_inner 的哪些位置插入 trace 采集 | 4 | 3 | 1 | 4 | Decision | 在 context_built/model_completed/tool_completed/execution_finished 事件处插入（已确认） |
| CLI 子命令命名与现有 /trace 的区分 | Known unknown | 现有 /trace 是代码调用链分析，新的是 /trace-agent Agent 追踪 | 2 | 2 | 1 | 1 | Accept | 命名不同，不冲突（已确认） |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Trace 应自动采集，无需用户手动启用 | 所有 PaaS 可观测性产品都是零配置接入 | 默认开启，EnterpriseAgent 启动时自动初始化 TraceCollector |
| Evaluation 应有可解释性（Why this score） | 设计文档提到 "Score + Why + How Improve" | 每个维度附带简短说明文字 |
| Replay 应能跨进程/跨重启 | 事件溯源存储在 SQLite，天然持久化 | SQLite 存储确保跨重启可用 |
| Agent 健康度应有历史趋势 | 设计文档 /score 只显示当前值 | MVP 显示当前值+最近 N 次平均值 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| 大量 Trace 数据可能导致 SQLite 写入瓶颈 | 高频 Agent 执行可能产生大量 Trace 行 | MVP 不做优化，观察实际写入量 |
| 多个 Agent 并发执行时 Trace 数据一致性 | 当前 EnterpriseAgent 是串行执行（operation_lock），无并发问题 | 确认当前架构无并发路径 |
| Trace 数据可能包含敏感信息（用户输入、文件内容） | 设计文档未提及数据脱敏 | MVP 不处理，记录为已知风险 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Resolution |
|---|---|---|---|---|
| Trace 存储方式 | SQLite 三表 / 内存 / JSON 文件 | 持久化 vs 简单 | 已确认 | SQLite 三表 |
| Evaluation 评分方式 | 内置固定维度 / 自定义维度 / 延后 | 灵活 vs 简单 | 已确认 | 内置固定维度 |
| Evaluation 打分方式 | 规则打分 / LLM 打分 | 成本 vs 准确性 | 已确认 | 规则打分 |
| Replay 机制 | 事件溯源 / 简单重放 / 仅日志 | 精确 vs 简单 | 已确认 | 事件溯源 |
| 实现范围 | 全部 / 最小 / 仅 trace | 完整 vs 快速 | 已确认 | 全部实现 |
| 架构集成 | 新分类+插件+CLI+EnterpriseAgent | 全集成 | 已确认 | 全量集成 |
| Benchmark 数据集 | 内置 / 文件加载 / 双模式 | 开箱即用 vs 灵活 | 已确认 | 内置任务集 |
| SlashCategory 命名 | Observability / Observe / Governance | 一致性 vs 简洁 | 已确认 | Observability |
| 新命令与现有 /trace 不冲突 | 命令名不同 | 无 | 已确认 | 无冲突 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 新命令使用现有 SlashCommand 插件架构 | 已有完善接口，注册模式已验证 | 改用 EnterpriseAgent 直接处理 |
| Trace 存储在单 SQLite 文件（trace.db） | 项目多处使用 SQLite，模式成熟 | 切换到 ClickHouse 等外部存储 |
| 新命令分类为 Observability | 仅影响 help 分组，不影响执行 | 修改 SlashCategory 枚举值 |
| 不引入新外部依赖 | 已有 all required deps (serde, chrono, uuid, sqlite) | 如有需要，后续添加 |

## Recommended Implementation Boundary

### Implement now

1. **数据模型** — AgentTrace, TraceStep, ToolExecution, Evaluation, BenchmarkResult, ScoreCard 结构体
2. **SQLite 存储** — trace.db 含 agent_trace, trace_step, tool_execution, evaluation, benchmark_result 表
3. **TraceCollector** — 自动采集 Agent 执行过程中的关键事件
4. **6 个 SlashCommand 实现** — trace-agent, evaluate, benchmark, debug, replay, score
5. **SlashCategory::Observability** 新增枚举值
6. **CLI 子命令** — 新增对应 CliCommand 变体
7. **EnterpriseAgent 集成** — execute_command 处理新命令，run_with_approval_inner 注入 Trace 采集
8. **单元测试** — 每个命令至少 1 个测试
9. **文档更新** — CHANGELOG.md + README.md

### Do not implement now

- LLM Judge 打分（预留接口）
- OpenTelemetry 导出
- 可视化 Trace View（Desktop 端）
- 与 core-audit 集成
- 数据脱敏
- 分布式追踪

### Interfaces or data contracts to freeze

- TraceCollector 的 record_trace() / get_trace() 接口
- Evaluation 的 score() 接口签名
- 存储层的 save/load 方法签名

### Areas that must remain reversible

- 内置 Benchmark 任务集（可追加不可删除）
- 规则打分权重（后续可调整）
- Trace 数据字段（只能扩展不能删除）

## Verification Plan

### Automated

- 单元测试：每个命令 validate + execute 测试
- 集成测试：SQLite 存储 CRUD 测试
- 端到端测试：从 EnterpriseAgent 执行 → Trace 采集 → 查询验证

### Manual

- 发一条消息给 Agent → 查看 `/trace-agent` 输出
- 运行 `/evaluate` 查看评分
- 运行 `/benchmark` 查看基准测试
- 运行 `/debug` 查看失败分析
- 运行 `/replay` 查看回放
- 运行 `/score` 查看健康度

### Observability

- 日志：trace 数据写入 SQLite 的过程
- 指标：Trace 采集数量、SQLite 写入延迟
- Audit trail：通过 core-audit（后续 Phase）

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [ ] Test cases
- [ ] Rollback requirements
- [ ] Observability requirements
- [x] Non-goals
- [ ] Implementation notes file