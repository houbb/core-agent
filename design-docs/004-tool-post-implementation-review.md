# P3 Tool Runtime Post-Implementation Review

## Metadata

- **Task / Feature:** Implement `004-tool.md`
- **Date completed:** 2026-07-17
- **Reviewer:** Codex
- **Related Unknowns Report:** `design-docs/004-tool-unknowns-report.md`
- **Related implementation notes:** `design-docs/004-tool-implementation-notes.md`
- **Related PR / commit:** None

## Behavior Changes

### Before

- P2 能返回 Tool Call，但 workspace 没有 Tool 注册、发现、校验、授权、执行、取消或审计 Runtime。
- Provider Tool、Builtin Tool 和统一结果没有可替换的公共契约。

### After

- 新增独立 `core-agent-tool`，不依赖 Session、Context 或 Model；上层可显式把 P2 Tool Call 转为 ToolRequest。
- `ToolManager` 统一执行 Interceptor → JSON Schema Validate → Policy → Permission → Execute/Timeout/Cancel → Result Mapper → Lifecycle。
- Registry 保存 live Tool，Catalog 保存 metadata；Static Provider 和 FunctionTool 提供安全 Builtin 接入方式。
- 点分 Capability Graph 支持精确/后代发现；Planner 可按能力而不是 Tool 名称查询。
- 默认权限是 Ask；Ask/Deny 均不执行，SQLite 规则按 priority/specificity 且冲突时 Deny 优先。
- 默认进程内 Lifecycle 和 SQLite Lifecycle 都拒绝重复 request ID 与非法状态跳转。
- 四张 SQLite 表保存 Provider、Tool、content-free Execution audit 与 Permission，不保存参数、输出或 Attachment 内容。

## Files and Systems Affected

| Area | Change | Why it changed |
|---|---|---|
| Workspace/root | 注册 P3 crate，显式导出 P3 API | 提供 Runtime 入口且避免新增 glob 冲突 |
| Domain | identity/schema/capability/request/result/permission/lifecycle | 冻结 Provider 无关契约 |
| Infrastructure | 10 个可替换 SPI、in-memory defaults | 支持 Builtin/MCP/Plugin 等来源 |
| Application | ToolManager 与 current-process cancel | 集中不变量和执行顺序 |
| Providers | FunctionTool、StaticToolProvider | 不提前实现 P4 文件/终端副作用 |
| Persistence | 四表、严格解析、migration、permission/lifecycle | Catalog、权限与审计闭环 |

## Assumptions Review

| Assumption | Status | Evidence | Action |
|---|---|---|---|
| session_id 只是 opaque correlation UUID | Confirmed | P3 无 Session crate dependency | Keep |
| Tool output 使用 Text/JSON + Attachment 引用 | Confirmed for P3 | ToolResult E2E | Keep, extend enum later |
| 单次 execute 可被独立并发调用 | Confirmed | in-flight map 仅按 request ID 隔离 | Keep; batch scheduling remains P6 |

## Unknowns Review

### Resolved

| Unknown | Resolution | Evidence |
|---|---|---|
| Ask 如何执行 | 返回 ApprovalRequired，绝不调用 Tool | Ask/Deny E2E |
| 参数/结果是否持久化 | content-free audit，不存在 payload 列 | schema + file DB E2E |
| 成功后审计失败 | 返回真实结果并标记 `core_agent.execution_audit=FAILED` | audit failure E2E |
| request ID 重放 | Created-only insert + 前态校验，默认/SQLite 均拒绝 | lifecycle unit + duplicate E2E |
| JSON Schema 外部资源 | HTTP/file resolver 关闭 | external `$ref` unit test |
| Permission 冲突 | priority → subject → tool → Deny/Ask/Allow → stable ID | permission unit test |

### Remaining

| Unknown | Risk | Follow-up |
|---|---|---|
| 阻塞 Tool 不协作取消 | Future 返回后底层线程/进程仍可能继续副作用 | P4/P6 Tool 必须实现 kill/rollback contract |
| 进程崩溃留下 Running | 审计准确但不会自动转 Failed/Cancelled | P6 recovery/checkpoint policy |
| 高频 Schema 编译成本 | 每次调用有额外 CPU | P10 profiling 后引入按 schema hash 的 compiled cache |
| Provider 批量 load 的跨 Catalog/Registry 原子性 | 极少数 Store/lock 故障可能留下 metadata-only Tool | Plugin/MCP 热更新阶段增加 registration transaction |

### Newly discovered

| Unknown | Impact | Recommended action |
|---|---:|---|
| 默认 jsonschema feature 可读网络/文件 | 5 | 保持 `default-features=false`，任何自定义 retriever 都需安全评审 |
| Result 阶段失败发生在副作用之后 | 5 | 保持非重试 ToolResult，不自动 retry |
| root glob export 会随 Runtime 增加冲突 | 3 | P3 已显式导出；1.0 前统一根 crate namespace policy |

## Deviations

| Deviation | Reason | User-visible effect | Risk | Approved |
|---|---|---|---|---|
| 默认 Lifecycle 从 Noop 收紧为 InMemory | 默认阻止重放与非法跳转 | 重复 request ID 明确失败 | 进程内记录增长 | Yes, review-driven |
| Mapper/Result Interceptor 失败映射 ToolResult::Failed | Tool 可能已经产生副作用 | 上层得到非重试统一终态 | 原输出被安全丢弃 | Yes, safety review |
| Builtin 首版提供 FunctionTool adapter，不实现 Filesystem/Shell | 真实副作用属于 P4 Workspace | 可注册安全 Builtin，无越层能力 | 需 P4 才有常用 coding tools | Yes, design boundary |

## Verification Evidence

### Automated checks

- [x] Unit tests — P3 18/18
- [x] Integration/end-to-end tests — P3 10/10
- [x] Migration/schema/strict parsing tests
- [x] Permission/security/cancellation contract tests
- [x] Strict P3 Clippy (`-D warnings`)
- [x] Workspace build/test
- [x] Format and diff checks

同轮全工作区回归：P0 `36+4`、P1 `52+4`、P2 `30+11`、P3 `18+10`。根 crate 仍有 P0–P2 既有 ambiguous glob re-export 警告；P3 使用显式根导出，没有新增冲突。

### Manual checks

- [x] Happy path represented by Provider → execute → SQLite E2E
- [x] Invalid/empty parameters represented by schema tests
- [x] Failure paths represented by validation, permission, timeout, tool and audit failures
- [x] Recovery represented by file database reopen
- [x] Permission boundaries represented by Ask/Deny/Allow and conflict tests
- [ ] UI/responsive/accessibility — P3 无 UI
- [ ] Production performance — 需要 P10 metrics/profile

## Three Review Passes

### Pass 1 — Architecture and API

- 确认 P3 与 Session/Context/Model 零依赖，Registry/Catalog 与 Provider/Executor 职责分离。
- 修复 SQLite execution upsert 覆盖旧审计和重复副作用风险，状态更新改为事务内前态/身份校验。

### Pass 2 — Invariants and security

- 禁用 JSON Schema HTTP/file resolver，固定线性 regex engine，限制 Schema/参数/Catalog metadata 大小。
- Permission scope 必须在 tool/capability 中二选一；等优先级冲突按 Deny → Ask → Allow。
- Tool output 不得伪造 `core_agent.*` metadata，Observer panic 隔离。

### Pass 3 — Regression and maintainability

- 默认 Lifecycle 改为进程内存储，Noop 仅显式 opt-out；根 crate 对 P3 采用显式导出。
- 补齐初始审计 fail-closed、最终审计 fail-open、重复 request、取消单终态和 file recovery 测试。
- Production code 无 `unsafe`、panic、unwrap、TODO 或 FIXME。

### Code Review Verdict

- **Engine specialists:** N/A — no engine configuration
- **ADR compliance:** NO ADRS FOUND
- **Architecture:** CLEAN
- **SOLID:** COMPLIANT
- **Testability:** TESTABLE
- **Verdict:** APPROVED WITH NON-BLOCKING SUGGESTIONS

Non-blocking suggestions: 生产 profiling 后缓存 compiled JSON Schema；MCP/Plugin 热更新前增加批量注册事务；后续拆分较长的 Manager/Store 编排函数并补齐所有公共 API rustdoc。

## Rollback and Recovery

- **Rollback trigger:** 权限选择、重复执行、取消终态或 audit identity 回归。
- **Code rollback steps:** 移除 workspace/root P3 集成和 `core-agent-tool` crate。
- **Data rollback steps:** 保留四张独立表；P0–P2 不读取它们，无需删除。
- **Configuration rollback steps:** 移除 Provider/Tool/Permission 注册。
- **Recovery verification:** 跑 P0–P2 workspace regression，并重开原 SQLite 数据库。

## Maintainer Notes

- `ToolManager` 是唯一执行入口；直接调用 live Tool 会绕过 Validator、Permission、Lifecycle 和 Policy。
- `request_id` 是幂等审计身份。不要把 SQLite Created insert 改回 upsert。
- Permission rule 的 capability 表示子树匹配，但 capability discovery 不等于授权。
- Timeout/cancel 只能取消协作式 Future；阻塞线程/子进程必须由具体 Tool 实现终止。
- 开启 jsonschema external resolver 或 fancy regex 前必须重新做 SSRF/file-read/ReDoS 评审。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [x] Reusable component
- [ ] Static analysis rule
- [ ] AGENTS.md rule
- [ ] Another Skill

## Understanding Check

1. **What changed?** 系统获得独立、可发现、可授权、可取消、可审计的 Tool Runtime。
2. **Which old paths enter it?** P0–P2 不被自动改写；P2 Tool Call 必须由上层显式构造 P3 ToolRequest。
3. **Most likely failures?** Tool 不协作取消、Permission 配置冲突、Provider 注册一半失败。
4. **Evidence versus assumptions?** 单进程/SQLite 执行契约均有测试；分布式恢复、MCP/Plugin 协议和生产性能仍未验证。
5. **Safe rollback?** 删除独立 crate 集成，保留无依赖的四张表。
6. **First place to inspect later?** `application/manager.rs`、`infrastructure/defaults.rs`、`persistence/store.rs`。
7. **Future difficult contracts?** Tool key/version、Capability hierarchy、Permission precedence、request ID 幂等与 cancellation contract。
