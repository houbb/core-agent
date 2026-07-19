# P2 Model Runtime Unknowns Report

## Metadata

- **Task / Feature:** Phase 2 Model Runtime
- **Mode:** Standard
- **Date:** 2026-07-17
- **Prepared by:** Codex
- **Scope:** `core-agent-model` 领域模型、Provider/Router/Engine 扩展点、真实兼容 Provider、SQLite Catalog/Usage、统一 API 与测试

## Intent

### User-visible problem

上层 Runtime 目前只能构建 Context，缺少一个与具体厂商无关、可路由、可流式、可统计的模型推理入口。

### Desired behavior change

调用方只面向统一 Model Manager 发起生成、流式、Embedding 和 Vision 请求；Runtime 完成模型选择、能力校验、Provider 调用、超时/有限重试、响应归一化和 Usage 采集。

### Affected users and workflows

- 框架开发者：注册 Provider 与 Model Profile 后调用统一 API。
- 后续 Tool、Planning、Execution Runtime：只依赖 Model Runtime 契约。
- 平台管理端：可从 Catalog 获取 Provider、模型画像及 Usage 数据。

### Success criteria

- Model Runtime 不依赖 Session 或 Context Runtime。
- 新 Provider 可通过实现 trait 注册，Manager 无需修改。
- generate、stream、embedding、vision 均经过路由和能力校验。
- Provider 不自行重试，重试、超时、fallback 由 Engine 统一控制。
- SQLite 只保存 Provider 非敏感配置、Model Profile 和 Usage；三张表满足审计字段及索引规范。
- 单元断言与端到端测试覆盖成功、路由、能力不足、超时、重试、fallback、流式和 Usage 持久化。

### Non-goals

- 不执行 Tool Call，不实现 Agent Loop、AI Workflow、复杂自动重试策略、Gateway、多租户或模型辩论。
- 不将 Context 转换为 Prompt；调用方显式构造 `ModelRequest`。
- 不在数据库中持久化 API Key。
- 不在本 P 内实现 Claude/Gemini 专有 wire protocol。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Documentation | `design-docs/000-roadMap.md` | P2 是独立 Model Runtime，位于 Context 与 Tool 之间 | High |
| Documentation | `design-docs/003-model.md` | Manager/Provider/Router/Engine/Stream/Capability/Catalog、请求响应、Usage、SQLite 与扩展点要求 | High |
| Workspace | `Cargo.toml`、`src/lib.rs` | 当前 workspace 只有 Session/Context，尚无 Model crate | High |
| Code | `core-agent-context/src/domain/context.rs` | Context 是完整结构化产物，但 P2 设计要求 Model Runtime 不知道 Session/Context | High |
| Code | `core-agent-context/src/infrastructure/*` | 现有 Runtime 使用 async trait、builder、observer panic 隔离等可复用风格 | High |
| Schema | `core-agent-session/src/persistence/schema.rs` | 项目表审计列、无外键和索引约定 | High |
| Tests | Session/Context 单元和 E2E | 项目采用 crate 内单元测试 + `tests/` 端到端测试 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| P2 必须是 Provider Runtime，不是 OpenAI Runtime | `003-model.md` | 核心接口不能泄漏单一厂商协议 |
| Model Runtime 不认识 Session、Conversation、Workspace | `003-model.md` | 新 crate 不依赖 P0/P1 crate |
| Streaming 应有独立 Engine | `003-model.md` | 不以 `if stream` 混入普通推理路径 |
| Usage 第一版就要包含 Token、延迟、成本 | `003-model.md` | 响应和持久化从第一版固定字段 |
| Provider 不应自行重试 | `003-model.md` | 重试、超时和 fallback 归 Inference/Stream Engine |
| SQLite 只保存 Provider、Model、Usage | `003-model.md` | 明确持久化边界 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| P2 首版是否必须同时实现所有厂商专有 HTTP 协议 | Known unknown | 文档列出多个示例 Provider，但企业路线把多 Provider 作为 P2.2，且没有凭据/协议版本约束 | 4 | 4 | 2 | 3 | 96 | Decision | 先实现 Provider SPI + OpenAI-compatible 真实适配器；专有协议后续独立适配 |
| Profile、Provider、Model hint 冲突时的优先级 | Known unknown | 文档同时提供 Profile 抽象和手工/自动路由 | 5 | 4 | 2 | 4 | 160 | Decision | Profile 是最高优先级；其他 hint 必须与其一致，否则返回参数错误，不静默改写 |
| fallback 是否允许流中断后切换模型 | Unknown unknown candidate | 已输出的增量无法安全撤回，切换会导致重复或语义拼接 | 5 | 3 | 3 | 4 | 180 | Decision | 只允许首个流事件前 fallback；流已开始后的错误原样返回 |
| API Key 如何保存 | Unknown known | UX 示例包含 API Key，但企业安全要求未定义密钥系统 | 5 | 4 | 4 | 5 | 400 | Blocker（仅对持久化密钥） | 本 P 明确不持久化密钥；由构造 Provider 时注入，日志/错误不得包含密钥 |
| 成本币种与定价单位 | Known unknown | 文档只有 Cost，没有币种和单位 | 3 | 4 | 2 | 3 | 72 | Accept | Profile 首版按每百万 Token 的数值计算，币种作为 metadata；`cost` 保持可选 |
| Provider 原始响应是否默认暴露敏感信息 | Unknown unknown candidate | RawResponse 对 Debug 有用，但可能含服务端扩展信息 | 4 | 3 | 2 | 3 | 72 | Monitor | 仅存于返回对象，不写 `model_usage`；调用方可选择丢弃 |
| Usage 持久化失败是否应让成功推理失败 | Known unknown | 推理结果已产生，审计与可用性有冲突 | 4 | 3 | 2 | 4 | 96 | Decision | 默认 collector 失败会返回错误，保证企业审计不静默丢失；可注入 Noop collector 显式关闭 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 所有扩展点都可替换且不污染核心领域模型 | Roadmap 强调十年演进和零改动新增 Provider | trait 契约测试与 fake Provider E2E |
| 路由结果必须确定且可解释 | 企业成本/能力选择需要可审计 | 响应携带 provider/model/profile，Observer 记录阶段 |
| 失败不能泄露密钥或静默降级到不满足能力的模型 | 企业运行时和 Capability Registry 定位 | 错误路径测试、能力过滤测试 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| SSE 数据可能跨网络 chunk 分割 | 简单按 chunk 分行会丢数据 | 使用符合 SSE 边界语义的解析器并以本地 HTTP E2E 验证 |
| 重试可能造成重复计费 | Provider 可能已处理请求但连接失败 | Request ID 固定贯穿重试；只对明确 retryable 错误重试并记录 attempt |
| `f64` NaN/Infinity 会破坏 JSON 或路由排序 | temperature/pricing/performance 来自配置 | 构造/保存时严格校验有限数值 |
| Stream 被调用方提前 drop 时 Usage 不完整 | 流式调用天然可取消 | 不伪造完整 Usage；Provider 有最终 Usage 时才采集，Observer 保留失败/完成边界 |
| SQLite 损坏行被默认值掩盖 | P0/P1 已发现此风险 | 严格解析 UUID、时间、枚举和 JSON，损坏即返回错误 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| 专有 Provider 的实现时机 | 本次全部实现 / SPI + 兼容 Provider / 仅 SPI | 兼容 Provider 可最快形成真实闭环，同时避免无凭据猜测协议 | Architecture | P2 实现前，已采用推荐项 |
| Usage 失败语义 | fail-open / fail-closed / 配置化 | 默认 fail-closed 保证审计，Noop 明确 opt-out | Architecture | P2 实现前，已采用推荐项 |

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| 真实 HTTP 适配和 SSE 是否可用 | 本地 mock HTTP Server E2E | generate、embedding、stream 都能归一化响应 | Medium | Implementation |
| fallback/retry 是否严格受 Engine 管理 | 可编程 fake Provider 测试 | attempt 次数、路由顺序与错误分类符合契约 | Low | Implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| OpenAI-compatible 是首个内置 HTTP 适配器 | 一个实现可覆盖多个常见端点，SPI 不绑定它 | 后续新增独立 Provider crate/模块，无需修改 Manager |
| Profile 名称是业务稳定键，数据库 `id` 是实体键 | 兼顾可读配置和审计实体 | 增加 alias/version 字段，不改变请求主体 |
| Auto 路由以显式优先级、成本、延迟作稳定排序 | 可解释、可测试，无需 AI 决策 | 替换 `ModelRouter` 实现 |
| Stream 不做中途透明重试 | 避免重复输出，行为可预测 | 将来增加带 offset/dedup 的协议后扩展 StreamEngine |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| Claude/Gemini 专有请求和流协议 | 不影响 Provider Runtime 契约，且缺少凭据与协议版本约束 | 作为独立 Provider 适配任务 |
| 企业白名单/黑名单与租户边界 | 明确属于 P2.7/P2.8 | Permission/Enterprise 阶段引入 Policy Provider |
| 动态实时价格和 Provider model discovery | 属于 Gateway/Catalog 同步演进 | 后续 Catalog 同步器，不写入 Manager |

## Recommended Implementation Boundary

### Implement now

- 独立 `core-agent-model` crate 和根 crate 重导出。
- 完整领域契约：请求、响应、流事件、Usage、Capability、Profile、Route。
- Manager、Router、InferenceEngine、StreamEngine、Provider Registry。
- 拦截器、RetryPolicy、RateLimiter、UsageCollector、Observer 扩展点。
- In-memory 与 SQLite Catalog，SQLite Usage Collector。
- OpenAI-compatible generate/stream/embedding/vision 真实适配器。

### Do not implement now

- Tool Call 执行、复杂自适应重试、专有厂商协议、Gateway、Policy、多租户。

### Interfaces or data contracts to freeze

- `ModelProvider`、`ModelRouter`、`ModelCatalog`、`UsageCollector`。
- `ModelRequest`、`ModelResponse`、`ModelStreamEvent`、`ModelProfile`、`ModelUsage`。
- Profile/hint 冲突、能力检查、retryable 错误和流式 fallback 语义。

### Areas that must remain reversible

- Router 排序策略、RetryPolicy、RateLimiter、Observer、拦截器、Usage 后端和 Provider 协议实现。

## Verification Plan

### Automated

- Unit tests: 领域校验、路由、Capability、成本、重试、超时、observer 隔离、schema。
- Integration tests: fake 多 Provider 完整 generate/stream/embedding/vision + SQLite Usage。
- Migration tests: 三张表审计列及旧表补列。
- Contract tests: OpenAI-compatible 本地 HTTP generate/stream/embedding。
- Static analysis: workspace build、Clippy、rustfmt。

### Manual

- Happy path: 注册 Provider/Profile 后四类调用返回统一结果。
- Empty state: 无模型或空输入明确报错。
- Failure path: 无能力、Provider 失败、超时、Usage 失败不静默。
- Recovery path: retryable 错误有限重试并可 fallback。
- Permission boundaries: API Key 不落库、不进入观察事件。
- Mobile / responsive: 不适用（本 P 无 UI）。
- Accessibility: 不适用（本 P 无 UI）。
- Performance: 阻塞 SQLite 操作必须 `spawn_blocking`，流式不预缓冲完整响应。

### Observability

- Logs: Runtime 不直接记录请求正文或 API Key。
- Metrics: Observer 暴露阶段、attempt、provider/model、耗时，不暴露输入内容。
- Alerts: 留给 Observation Runtime。
- Audit trail: `model_usage` 保存每次完成调用及失败分类。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [x] Implementation notes file
