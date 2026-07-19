# P3 Tool Runtime Unknowns Report

## Metadata

- **Task / Feature:** Phase 3 Tool Runtime
- **Mode:** Standard
- **Date:** 2026-07-17
- **Prepared by:** Codex
- **Scope:** `000-roadMap.md`、`004-tool.md`、P2 Tool Call 输出边界及现有 workspace/schema 约束

## Intent

### User-visible problem

模型已经能返回 Tool Call，但系统没有独立、可发现、可校验、可授权、可取消和可审计的工具执行边界。

### Desired behavior change

调用方可通过统一 `ToolManager` 注册/发现并执行来自 Builtin、MCP、Plugin、HTTP 等来源的 Tool；每次调用都经过参数校验、策略、权限、超时、取消、结果映射和生命周期记录。

### Affected users and workflows

- Runtime 开发者：注册 Tool/Provider、替换 Validator/Permission/Executor/Mapper。
- 上层 Agent：把 P2 返回的 Tool Call 显式转换为 ToolRequest 并执行。
- 平台维护者：查询 Catalog、Permission 与不含正文的 Execution audit。

### Success criteria

- 独立 `core-agent-tool` crate，不依赖 Session、Context、Model。
- Tool Registry 与 Catalog 分离；运行实例不进入 SQLite。
- JSON Schema 参数校验、默认 Ask 权限、显式 Allow/Deny、总超时和主动取消真实生效。
- 生命周期按 Created → Ready → Running → Success/Failed/Cancelled 演进，非法转换被拒绝。
- 四张 SQLite 表满足审计字段、注释、索引、无外键和 legacy additive migration。
- 单元断言与端到端覆盖成功、校验、权限、超时、取消、失败、Provider 注册和持久化恢复。

### Non-goals

- Planner、Agent Loop、Workflow、多 Tool 调度、批量并行、重试、缓存、Marketplace、远程 Agent。
- P4 Workspace 的 File/Git/Terminal 实现、P6 Approval 流程、P8 企业权限引擎、P10 完整 Observation。
- Tool Runtime 自动消费/执行 P2 Tool Call。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | P3 是独立 Tool Runtime | High |
| Documentation | `design-docs/004-tool.md` | 组件、生命周期、四表、扩展点与非目标 | High |
| Code | `core-agent-model/src/domain/response.rs` | P2 只返回 Tool Call，不执行 | High |
| Code | `core-agent-model/src/infrastructure` | live Registry 与 durable Catalog 分离是现有模式 | High |
| Schema | `core-agent-model/src/persistence/schema.rs` | 审计字段、索引、无外键约定 | High |
| Tests | P0-P2 workspace tests | async trait、SQLite migration、Observer 隔离的现有验证方式 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| Tool Runtime 不应知道 Session、Planner、LLM | `004-tool.md` Runtime 职责 | 新 crate 保持零横向依赖 |
| P2 Tool Call 已有 id/name/JSON arguments | P2 response type | 上层可无损构造 ToolRequest，但不在 P3 建依赖 |
| Permission 第一版必须存在 | `004-tool.md` ToolPermission | 执行不能绕过权限阶段 |
| MVP 明确不做 retry/cache/scheduling | `004-tool.md` 非目标 | 避免把 P6/P8/P9/P10 提前写进 P3 |
| SQLite 必须有四张表 | `004-tool.md` SQLite | P3 需要 Catalog、Provider、Execution、Permission 持久化 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| ToolRequest 包含 Session，但 Runtime 又“不知道 Session” | Known unknown | 设计文字存在表面冲突 | 4 | 5 | 3 | 4 | 240 | Decision | 只保留可选 opaque `session_id: Option<Uuid>` 关联值，不依赖 Session crate、不查询生命周期 |
| Ask 权限如何执行 | Known unknown | P3 没有 Human Approval Runtime | 5 | 5 | 4 | 5 | 500 | Decision | Ask 返回 `ApprovalRequired`，不执行；外部批准后更新/替换 Permission 实现 |
| Tool 参数与结果是否持久化 | Unknown unknown candidate | 文件、Shell、HTTP 可能含凭证/隐私 | 5 | 4 | 4 | 5 | 400 | Decision | Execution audit 默认只存身份、状态、耗时、错误种类和 allowlist metadata，不存参数/正文 |
| 成功 Tool 后审计写失败的语义 | Unknown unknown candidate | Tool 可能有不可逆副作用，返回失败会诱导重试 | 5 | 3 | 4 | 5 | 300 | Decision | 执行前 Created audit fail-closed；执行后审计 fail-open，并在结果 metadata + Observer 标记失败 |
| Cancel 的持久化/跨进程语义 | Known unknown | P3 没有分布式 Execution Runtime | 4 | 4 | 2 | 3 | 96 | Monitor | 只取消当前进程中的在途调用；重启后旧 Running 记录不自动恢复 |
| Capability Graph 的匹配语义 | Known unknown | 设计给出层级，但无继承规则 | 3 | 4 | 3 | 3 | 108 | Decision | 使用大小写不敏感的点分层级键；Catalog 支持精确和后代发现，不把父能力自动当作执行授权 |
| MVP Builtin Tool 的具体清单 | Unknown known | 示例包含 Filesystem/Git/Terminal，但这些属于 P4 Workspace | 4 | 4 | 2 | 3 | 96 | Accept | 提供通用 `FunctionTool` 与 `StaticToolProvider`，测试用安全 Builtin；不越界实现文件/终端副作用 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| Tool 替换不影响 Planner | Capability Graph 明确要求解耦 | Catalog 按 capability 查询测试 |
| Registry 热更新不破坏 Catalog 身份 | register/unregister 是公开 API | stable key 与 upsert 测试 |
| Tool 失败仍能被上层统一消费 | ToolResult 包含 Status/Error | 失败映射端到端测试 |
| Observer 不能打断真实 Tool | P0-P2 已采用 panic 隔离 | panic Observer 测试 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| Interceptor 修改参数后绕过校验 | 会形成校验旁路 | 拦截后重新验证的测试 |
| Registry 实例与 Catalog metadata 不一致 | 可能执行未启用或错误版本 Tool | Manager 每次回查双边一致性 |
| timeout/drop future 不等于阻止阻塞线程副作用 | Rust future 取消是协作式 | Tool contract 文档 + cancellation token 传入 ToolContext |
| metadata 泄露敏感信息 | Execution 表是持久化审计 | 敏感键拒绝 + allowlist/长度限制测试 |
| Result Mapper/Interceptor 失败发生在副作用之后 | 不能安全重试 | 记录 Failed，不自动 retry，并保留明确错误 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| 默认 Permission | Allow / Ask / Deny | Allow 不安全；Deny 不可发现；Ask 可安全接入未来审批 | Security | P3 实现前，采用 Ask |
| 审计正文 | 全量 / 脱敏 / content-free | 全量可 replay 但泄露风险高 | Security | P3 实现前，采用 content-free |
| JSON Schema 支持 | 自制子集 / 标准库 | 自制容易产生校验绕过 | Architecture | P3 实现前，采用成熟 JSON Schema validator |
| Tool failure API | 外层 Err / 统一 ToolResult | 统一结果更便于 LLM 消费，基础设施错误仍需 Err | Architecture | P3 实现前，工具失败映射为 `ToolResult::Failed` |

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| JSON Schema crate 是否支持当前 Rust/toolchain | Compile + validation tests | 严格 Clippy 与 schema edge tests 通过 | Low | Implementation |
| Cancel/timeout 是否进入唯一终态 | Async E2E | observer/store 都只见一个 Cancelled/Failed 终态 | Medium | QA |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| Tool key 由 Provider/Name/Version 组成的稳定字符串 | pre-1.0 且 Catalog 保留独立 UUID | 未来增加结构化 ToolIdentity 并提供解析兼容层 |
| Tool output 首版统一为 Text/JSON + Attachment 引用 | 覆盖设计示例且不内嵌二进制 | 后续新增枚举 variant |
| 同一 Manager 可接受并发独立调用 | 不等于 Runtime 批量调度 | 后续 Execution Runtime 加调度器，不改单次 execute |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| MCP/Plugin/HTTP wire protocol | P3 只冻结 Provider SPI | 对应 Provider 实现阶段做协议契约测试 |
| 分布式取消和恢复 | 属于 Execution/Enterprise Runtime | 发现 stale Running 时由后续 recovery policy 处理 |
| Tool 结果回灌 Context 的格式 | P3 不知道 Model/Context | 由 Agent Loop/Execution Runtime 定义桥接 |
| 细粒度主体/RBAC/额度 | P8 Permission Runtime | `subject_id`/metadata 保持 opaque |

## Recommended Implementation Boundary

### Implement now

- Domain：ToolDefinition、Schema、Capability、Request、Result、Lifecycle、Permission、Execution record。
- Runtime：Manager、Registry、Catalog、Provider loader、Executor、Validator、Mapper、Lifecycle、Interceptor、Observer、Policy。
- In-memory defaults、FunctionTool/StaticProvider、current-process cancel/timeout。
- SQLite 四表、strict parsing、audit migration、无正文 Execution audit。

### Do not implement now

- 文件/终端/Git/Browser 的真实 Workspace Tool。
- 自动 Tool Call loop、并行调度、重试、缓存、审批 UI、MCP wire protocol。

### Interfaces or data contracts to freeze

- Tool identity/key、Capability dotted path、ToolRequest/ToolResult、Permission decision、Lifecycle transition、Provider/Executor/Validator/Mapper traits。

### Areas that must remain reversible

- Capability 选择策略、Permission backend、JSON Schema implementation、Execution storage、Provider discovery。

## Verification Plan

### Automated

- Unit tests: identity/capability/schema/request/result/lifecycle/permission/registry/catalog/metadata。
- Integration tests: provider registration → discovery → validate → permission → execute → map → lifecycle/audit。
- Migration tests: 四表审计列/索引、legacy additive migration、损坏行严格报错。
- Contract tests: Ask/Deny 不调用 Tool，timeout/cancel 单终态，observer panic 隔离，审计失败语义。
- Static analysis: fmt、strict Clippy、workspace tests、diff check。

### Manual

- Happy path: 安全 FunctionTool 返回统一结果。
- Empty state: missing tool/empty schema/empty params。
- Failure path: validation/permission/tool/mapper/audit failure。
- Recovery path: file SQLite reopen。
- Permission boundaries: default Ask、explicit Allow/Deny。
- Mobile / responsive: 无 UI。
- Accessibility: 无 UI。
- Performance: 记录 duration；生产基准推迟。

### Observability

- Logs: P3 不内建 logger。
- Metrics: Observer 提供阶段与耗时。
- Alerts: audit failure 事件供 P10 接入。
- Audit trail: `tool_execution` content-free 状态记录。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file
