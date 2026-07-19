# P3 Tool Runtime Implementation Notes

## Metadata

- **Task / Feature:** Phase 3 Tool Runtime
- **Date started:** 2026-07-17
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/004-tool-unknowns-report.md`
- **Related plan / issue / PR:** `design-docs/004-tool.md`

## Confirmed Discoveries

### Discovery D-001

- **What was discovered:** P2 Tool Call 已经是 P3 可消费的 id/name/JSON arguments，但 P3 不应依赖 Model crate。
- **Evidence:** P2 response contract 与 P3 “不知道 LLM”边界。
- **Why it matters:** 避免 Tool Runtime 被某一模型协议或 Agent Loop 绑定。
- **Affected scope:** crate dependencies、ToolRequest constructors、root integration。
- **Action taken:** 桥接由上层显式完成；P3 只接受自己的 ToolRequest。

### Discovery D-002

- **What was discovered:** Tool 执行可能有不可逆副作用，retry/审计失败语义不能照搬纯读取调用。
- **Evidence:** Write/Delete/Shell/HTTP 都可能已经产生外部效果。
- **Why it matters:** 把成功误报为失败会诱导重复副作用。
- **Affected scope:** Lifecycle、ExecutionStore、ToolResult metadata、Observer。
- **Action taken:** 执行前审计 fail-closed；执行后审计失败不隐藏成功结果，并明确标记。

### Discovery D-003

- **What was discovered:** JSON Schema validator 的默认 feature 会解析 HTTP/file `$ref`，默认正则引擎允许回溯。
- **Evidence:** 本地依赖源码的 default feature 与 PatternOptions 文档。
- **Why it matters:** 外部 Tool Schema 可形成 SSRF/本地文件读取或正则拒绝服务入口。
- **Affected scope:** `JsonSchemaToolValidator` 与 Cargo feature。
- **Action taken:** 禁用 external resolver feature，并固定线性时间 regex engine；加入外部引用拒绝测试。

### Discovery D-004

- **What was discovered:** `request_id` 不只是 correlation 字段，也是副作用执行的幂等审计身份。
- **Evidence:** SQLite 原 upsert 会覆盖旧 execution，重复请求可能再次执行且抹掉旧审计。
- **Why it matters:** Write/Delete/Shell 等 Tool 不能因审计覆盖而静默重放。
- **Affected scope:** 默认 Lifecycle、SQLite transition、Manager initial audit。
- **Action taken:** 默认使用进程内 Lifecycle；Created 只允许插入，后续状态必须验证前态和身份后更新。

## Decisions

### Decision DEC-001

- **Decision:** Runtime 默认 Permission 为 Ask；Ask/Deny 均不执行 Tool。
- **Alternatives considered:** 默认 Allow；默认 Deny。
- **Reason:** Ask 既安全又保留未来 Approval Runtime 的接入语义。
- **Evidence:** P3 明确要求 Permission 第一版存在，P6/P8 才拥有完整审批/权限编排。
- **Owner / approver:** Security/Architecture（用户授权继续 P3）
- **Reversibility:** Permission trait 可替换；默认值变更需版本化。
- **Follow-up:** P8 提供主体、资源、动作级策略实现。

### Decision DEC-002

- **Decision:** SQLite Execution audit 不保存参数、结果正文或 Attachment 内容。
- **Alternatives considered:** 全量保存；字段级脱敏保存。
- **Reason:** P3 没有 Secret/Data Classification Runtime，无法可靠脱敏任意 Tool 数据。
- **Evidence:** 工具示例天然可能处理源代码、凭证、SQL 和网络响应。
- **Owner / approver:** Security
- **Reversibility:** 后续可新增显式 opt-in payload store，不改变现有表语义。
- **Follow-up:** P10 Observation/Replay 定义加密与保留策略。

### Decision DEC-003

- **Decision:** Tool failure 映射为统一 `ToolResult` 终态；解析、注册、权限、执行前审计等 Runtime 失败使用外层 typed error。
- **Alternatives considered:** 所有失败都外层 Err；所有失败都 ToolResult。
- **Reason:** 上层可以统一消费真实 Tool 失败，同时不会把框架配置错误误当成工具输出。
- **Evidence:** `004-tool.md` 的 ToolResult 同时包含 Status 与 Error。
- **Owner / approver:** Architecture
- **Reversibility:** 可增加 helper，不改变核心 wire shape。
- **Follow-up:** E2E 覆盖两类失败边界。

## Assumptions

### Assumption A-001

- **Assumption:** `session_id` 只是 opaque correlation UUID。
- **Why it is currently acceptable:** 满足 ToolRequest 可关联 Session，同时没有 Session crate 依赖或状态查询。
- **Risk:** 将来可能需要 tenant/subject/conversation 等结构化上下文。
- **How it will be validated:** Cargo 依赖图与 request serialization 测试。
- **Reversal plan:** 增加独立 InvocationContext，而不是依赖 Session domain。

## Deviations

| Planned behavior | Implemented behavior | Reason | Evidence |
|---|---|---|---|
| Result Mapper/Result Interceptor 失败使用外层 error | Tool 已执行后发生的 Mapper/Interceptor 失败映射为非重试 ToolResult::Failed | 避免调用方重试已产生副作用的 Tool | mapper/interceptor 终态逻辑 + E2E failure contract |
| 默认 Lifecycle 可 Noop | 默认使用 `InMemoryToolLifecycle`，Noop 仅显式 opt-out | 默认阻止当前进程重复 request ID 和非法状态跳转 | lifecycle 单元测试 + duplicate request E2E |

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| 阻塞 Tool 忽略 cancellation token | 4 | ToolContext 暴露协作取消；timeout 只保证调用 Future 返回 | Tool author | 接入首个阻塞/子进程 Tool |
| 重启后 Running audit 无自动恢复 | 3 | 状态可查询且不伪造终态 | Execution Runtime | P6 checkpoint/recovery |
| 专有 Provider 协议未内置 | 2 | 稳定 ToolProvider SPI | Provider owner | MCP/Plugin 接入 |
| 每次执行重新编译 JSON Schema | 2 | Schema/参数有大小上限，首版保持 Validator 无状态 | Runtime maintainer | 高频 Tool profiling |

## Tests Added or Updated

- P3 单元断言：18/18，覆盖 capability hierarchy、schema/外部引用、metadata、lifecycle、registry/catalog、permission 冲突、SQLite schema/migration/strict parsing。
- P3 端到端：10/10，覆盖 Provider discovery、成功执行、Ask/Deny、拦截后重校验、timeout、Tool failure、cancel、Observer panic、审计失败、重复 request、文件数据库恢复。
- 全工作区回归：P0 36+4、P1 52+4、P2 30+11、P3 18+10，全部通过。
- 静态验证：`cargo clippy -p core-agent-tool --all-targets -- -D warnings` 通过；format/diff check 见 Post-Implementation Review。

## Rollback Notes

- Code rollback: 移除 workspace/root 对 `core-agent-tool` 的依赖并删除独立 crate。
- Data rollback: 四张表与 P0-P2 无依赖，可保留数据。
- Configuration rollback: 删除 Tool/Provider/Permission 注册。
- External-system rollback: P3 不创建外部资源；具体 Tool 自己声明回滚语义。
- Recovery validation: P0-P2 workspace regression 继续通过。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
