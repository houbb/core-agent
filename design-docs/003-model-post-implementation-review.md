# P2 Model Runtime Post-Implementation Review

## Metadata

- **Task / Feature:** Review and implement `003-model.md`
- **Date completed:** 2026-07-17
- **Reviewer:** Codex
- **Related Unknowns Report:** `design-docs/003-model-unknowns-report.md`
- **Related implementation notes:** `design-docs/003-model-implementation-notes.md`
- **Related PR / commit:** None

## Behavior Changes

### Before

- Workspace 只有 Session 与 Context Runtime，没有统一模型请求、路由、流式输出或 Usage 审计能力。
- Provider、Model Profile、能力声明、重试与 fallback 没有稳定边界。

### After

- 新增独立 `core-agent-model` crate，提供 Generate、Stream、Embedding、Vision 四类统一 API。
- Model Profile、Catalog、Capability Registry 和确定性 Router 支持手动、自动、最低成本、最低延迟及 fallback。
- 中央执行引擎统一处理总超时、有限重试、限流和首输出前 fallback；流开始后禁止透明切换。
- OpenAI-compatible Provider 通过真实 HTTP/SSE 支持文本、多模态、Embedding 和 Tool Call 返回，但不执行 Tool。
- SQLite 保存 Provider、Model 与 Usage，严格校验数据、迁移审计列、禁止持久化 API Key。
- Interceptor、Usage Collector、Retry Policy、Rate Limiter 与 Observer 均可替换，Observer panic 不改变推理结果。

## Files and Systems Affected

| Area | Change | Why it changed |
|---|---|---|
| Workspace/root crate | 注册并导出 `core-agent-model` | 提供 P2 公共入口 |
| Model domain | 请求、响应、Profile、Capability、Usage | 建立 Provider 无关契约 |
| Application | Manager、Engine、Router、Stream | 集中执行策略和不变量 |
| Infrastructure | Catalog、Registry、扩展 Traits、默认策略 | 支持可插拔实现 |
| Provider | OpenAI-compatible HTTP/SSE adapter | 建立可真实调用的首个 Provider |
| Persistence | 三张 SQLite 表、迁移、严格解析 | Catalog 与 Usage 审计闭环 |

## Assumptions Review

| Assumption | Status | Evidence | Action |
|---|---|---|---|
| 定价按每百万 Token 配置 | Confirmed | 成本计算和最低成本路由测试 | Keep |
| Cached Token 属于 Prompt Token | Confirmed for compatible provider | Usage 归一化和真实 HTTP 测试 | Monitor other providers |
| P2 不依赖 Session/Context | Confirmed | crate 依赖图与 workspace build | Keep |

## Unknowns Review

### Resolved

| Unknown | Resolution | Evidence |
|---|---|---|
| Profile 与 Provider/Model hint 冲突 | 明确返回路由错误 | Router 单元测试 |
| 流式 fallback 边界 | 只允许首输出前 fallback | Stream E2E |
| Usage Collector 失败语义 | 成功推理保留结果并显式标记审计失败 | Usage failure E2E |
| 密钥与 metadata 边界 | 密钥仅存在 Provider 实例；持久化 metadata 使用 allowlist | 单元 + SQLite E2E |
| 失败请求的 Provider 归属 | 记录实际最终 Provider | fallback audit E2E |

### Remaining

| Unknown | Risk | Follow-up |
|---|---|---|
| Claude/Gemini 专有协议 | 兼容 Provider 无法覆盖专有 wire format | 后续通过 Provider SPI 单独实现 |
| Consumer 主动丢弃 Stream | 无法同步写入 Cancelled/Aborted Usage | Observation/Billing 强一致阶段设计取消协议 |
| 动态定价和汇率 | Catalog 价格可能过期，混合币种不可比较 | 引入动态 Catalog 前定义价格版本与 Currency |

### Newly discovered

| Unknown | Impact | Recommended action |
|---|---:|---|
| SSE 正常 EOF 与 `[DONE]` 缺失易混淆 | 4 | 保持严格完成标记；专有 Provider 单独适配 |
| 自定义 Router 可能返回 Catalog 外 Profile | 4 | Manager 始终回查并规范化为合格 Catalog 候选 |
| Fail-closed Usage 会触发重复计费 | 5 | 默认 fail-open + 显式审计失败信号 |

## Deviations

| Deviation | Reason | User-visible effect | Risk | Approved |
|---|---|---|---|---|
| 成功推理遇 Usage Collector 失败时 fail-open | 避免重试已计费推理 | 返回结果并在 metadata 标记失败 | Usage 记录可能短暂缺失 | Yes, review-driven |
| Usage metadata 从黑名单收紧为 allowlist | 防止未知敏感信息持久化 | 非允许键不会进入 Usage | 部分自定义标签被丢弃 | Yes, security review |
| 缺少 `[DONE]` 的 SSE 视为失败 | 防止截断响应被误记完成 | 不完整流返回错误 | 少数非标准端点需专有适配 | Yes |

## Verification Evidence

### Automated checks

- [x] Unit tests — P2 30/30
- [x] Integration/end-to-end tests — P2 11/11
- [x] Migration and schema tests
- [x] Real HTTP/SSE contract tests
- [x] Strict P2 Clippy
- [x] Workspace build/test
- [x] Format and diff checks

全工作区同轮回归：P0 36 个单元断言 + 4 个端到端、P1 52 + 4、P2 30 + 11，全部通过。根 crate 仍有既有 ambiguous glob re-export 警告，P2 在 `-D warnings` 下无警告。

### Manual checks

- [x] Happy path represented by four-operation E2E
- [x] Empty/invalid request represented by assertions
- [x] Failure/retry/fallback paths represented by E2E
- [x] Persistence recovery and migration represented by tests
- [x] Credential and metadata boundaries represented by tests
- [ ] UI/responsive/accessibility — P2 无 UI
- [ ] Production latency/cost accuracy — 需要真实供应商运行数据

## Three Review Passes

### Pass 1 — Architecture and API

- 收紧 Manager 边界：自定义 Router 结果必须回到启用的 Catalog 候选，并重新应用 operation capability、输出上限和策略约束。
- 将 timeout、retry、fallback 与 rate limit 收敛到中央 Engine，明确 Stream 首输出边界。

### Pass 2 — Invariants and audit

- 修复 Response Interceptor 失败时丢失真实 Usage、fallback 失败归属错误和缓存 Token 重复计数。
- Usage Collector 失败不再隐藏成功推理，避免重复计费；失败通过 metadata 与 Observer 明确暴露。

### Pass 3 — Security and maintainability

- 收紧 endpoint、错误正文、Provider metadata 和 Usage metadata，密钥不进入 Catalog、Usage 或 Observer。
- 严格处理 SSE 完成标记、Observer panic、SQLite 损坏行与 legacy audit migration。

### Code Review Verdict

- **Architecture:** CLEAN
- **Safety:** APPROVED after fixes
- **Testability:** TESTABLE
- **Verdict:** APPROVED WITH NON-BLOCKING FOLLOW-UPS

## Rollback and Recovery

- **Rollback trigger:** 路由选择、流式边界、Provider 协议或 Usage 归属出现回归。
- **Code rollback steps:** 移除 workspace/root 依赖与 `core-agent-model` crate。
- **Data rollback steps:** 保留三张独立表；P0/P1 不读取它们，无需破坏性迁移。
- **Configuration rollback steps:** 撤销 endpoint/API Key/Profile 注册。
- **Recovery verification:** 运行 P0/P1 workspace regression 并确认原数据库可重开。

## Maintainer Notes

- `ModelManager` 是不变量边界；自定义 Router、Interceptor 和 Registry 不能绕过它。
- Stream 只有首输出前能 fallback；任何已交付事件后的错误必须原样结束该流。
- 成功响应上的 `core_agent.usage_collection=FAILED` 不是模型失败，调用方不应自动重试。
- API Key 只能注入 Provider 实例。新增可持久化 metadata 键前必须评估敏感性和大小。

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

1. **What changed?** 系统获得独立、可路由、可流式、可审计的模型运行时。
2. **Which old paths enter it?** 当前无 P0/P1 旧路径被强制改写；根 crate 仅新增导出，后续 Runtime 可显式调用。
3. **Most likely failures?** Provider 协议差异、Stream 中途网络失败、Usage Collector 不可用。
4. **Evidence versus assumptions?** 路由/超时/持久化/HTTP-SSE 均有测试；专有 Provider 和生产定价仍待真实接入。
5. **Safe rollback?** 删除独立 crate 集成并保留无依赖的 P2 表。
6. **First place to inspect later?** `application/manager.rs`、`application/engine.rs`、Provider adapter、`persistence/store.rs`。
7. **Future difficult contracts?** Profile identity、Capability 名称、Usage 计量语义、Stream 完成协议和 Provider wire format。
