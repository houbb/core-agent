# P2 Model Runtime Implementation Notes

## Metadata

- **Task / Feature:** Phase 2 Model Runtime
- **Date started:** 2026-07-17
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/003-model-unknowns-report.md`
- **Related plan / issue / PR:** `design-docs/003-model.md`

## Confirmed Discoveries

### Discovery D-001

- **What was discovered:** Model Runtime 必须保持对 Session/Context 的零依赖。
- **Evidence:** `003-model.md` 明确 Model Runtime 只知道 Input、Model、Output。
- **Why it matters:** 避免模型接口绑定 Session 生命周期或 Context 内部结构。
- **Affected scope:** 新 crate 依赖、领域 DTO、根 crate 集成。
- **Action taken:** `core-agent-model` 不依赖 `core-agent-session` 或 `core-agent-context`。

### Discovery D-002

- **What was discovered:** 一个 OpenAI-compatible Provider 可覆盖多个首批目标端点，但不能正确覆盖 Claude/Gemini 专有协议。
- **Evidence:** P2 Provider 清单与各端点的协议边界。
- **Why it matters:** 既需要真实闭环，也不能伪装成错误的全厂商兼容。
- **Affected scope:** Provider SPI 与首个内置 Provider。
- **Action taken:** 实现真实兼容 Provider，专有协议通过相同 SPI 后续加入。

## Decisions

### Decision DEC-001

- **Decision:** Profile 是路由最高优先级，Provider/Model hint 与其冲突时返回错误。
- **Alternatives considered:** 静默覆盖 hint；忽略 Profile；最后写入者优先。
- **Reason:** 明确且可审计，避免调用了意外模型。
- **Evidence:** Model Profile 是文档建议的长期稳定抽象。
- **Owner / approver:** Architecture（用户已授权继续 P2）
- **Reversibility:** Router 可替换；默认契约需版本化后变更。
- **Follow-up:** 添加冲突和确定性路由测试。

### Decision DEC-002

- **Decision:** API Key 仅在构造 Provider 时注入，不进入 Catalog、Usage、Observer 或错误正文。
- **Alternatives considered:** SQLite 明文；加密列；环境变量名持久化。
- **Reason:** 项目尚无 Secret Runtime，明文不可接受，加密密钥生命周期也未定义。
- **Evidence:** UX 需要 Key，但 P2 数据边界只要求 Provider/Model/Usage。
- **Owner / approver:** Architecture/Security
- **Reversibility:** 后续可注入 Secret Provider，不改变模型请求契约。
- **Follow-up:** Provider 测试验证持久化数据不含密钥。

### Decision DEC-003

- **Decision:** 流开始后不透明重试或 fallback。
- **Alternatives considered:** 重放整流；从中断点切换；拼接另一模型输出。
- **Reason:** 首版没有 offset/dedup 协议，透明切换会重复或破坏语义。
- **Evidence:** Streaming 数据一旦交付调用方就不可撤回。
- **Owner / approver:** Architecture
- **Reversibility:** 将来增加可恢复流协议后扩展。
- **Follow-up:** 测试首事件前可 fallback、首事件后错误直达。

### Decision DEC-004

- **Decision:** 已成功的推理不因 Usage Collector 失败而改写为失败；响应写入 `core_agent.usage_collection=FAILED`，并通知 `ModelStage::UsageFailed`。
- **Alternatives considered:** 推理与 Usage 持久化整体 fail-closed；完全忽略 Usage 失败。
- **Reason:** fail-closed 会诱导调用方重试已经计费的推理，造成重复输出和重复计费；完全忽略又会隐藏审计缺口。
- **Evidence:** 安全评审发现重复计费风险，端到端测试覆盖成功响应、失败标记和 Observer 通知。
- **Owner / approver:** Architecture/Security
- **Reversibility:** 可在未来增加显式的强审计模式；默认行为变更需要版本化。
- **Follow-up:** Observation Runtime 接入后为 `UsageFailed` 增加告警指标。

## Assumptions

### Assumption A-001

- **Assumption:** 定价按每百万 Token 配置，币种由 metadata 描述，成本允许未知。
- **Why it is currently acceptable:** 不阻塞 Usage 数据契约，也不虚构币种。
- **Risk:** 不同 Catalog 来源可能使用不同币种。
- **How it will be validated:** 成本计算单元测试；未知价格返回 `None`。
- **Reversal plan:** 后续增加结构化 Currency，不修改原 Token 计数。

### Assumption A-002

- **Assumption:** `cached_tokens` 是 `prompt_tokens` 的子集，`total_tokens = prompt_tokens + completion_tokens`。
- **Why it is currently acceptable:** 符合首个 OpenAI-compatible Provider 的计量语义，并避免缓存 Token 被重复计数。
- **Risk:** 个别 Provider 可能上报不同定义。
- **How it will be validated:** Usage 领域测试与真实 HTTP Provider 端到端测试。
- **Reversal plan:** 在 Provider 适配层归一化，不修改统一 Usage 契约。

## Deviations

| Planned behavior | Implemented behavior | Reason | Evidence |
|---|---|---|---|
| Usage 持久化失败默认 fail-closed | 成功推理 fail-open，并携带失败标记与 Observer 事件 | 防止调用方重试已计费请求 | 安全评审 + `usage_collector_failure_does_not_hide_successful_inference` |
| Provider metadata 仅做敏感键拒绝 | Provider 拒绝敏感键；持久化 Usage metadata 额外采用 allowlist、控制字符清理和长度限制 | 降低未知元数据泄露凭证或大对象的风险 | metadata 单元测试 + SQLite E2E |

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| 专有 Provider 协议尚未内置 | 3 | 稳定 Provider SPI + 兼容 Provider | Provider maintainer | 首个 Claude/Gemini 接入需求 |
| Stream consumer 主动丢弃可能没有 Cancelled/Aborted Usage | 3 | 不伪造 Usage；超时、Provider 错误和正常完成均有审计 | Runtime maintainer | Billing 要求强一致时 |
| 动态价格与多币种换算未实现 | 2 | 混合币种最低成本路由明确拒绝 | Catalog maintainer | 接入动态 Catalog 时 |

## Tests Added or Updated

- P2 单元断言：30/30，通过领域约束、路由、重试、Provider 解析、Schema/迁移和严格持久化解析。
- P2 端到端：11/11，通过四类推理、SQLite Usage、retry/fallback、限流、流式超时/截断、真实 HTTP/SSE、审计归属和观察器隔离。
- 全工作区回归：P0 36+4、P1 52+4、P2 30+11，全部通过。
- 静态验证：`cargo clippy -p core-agent-model --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check` 通过；后者仅报告既有 LF/CRLF 提示。

## Rollback Notes

- Code rollback: 移除 workspace/root 对 `core-agent-model` 的依赖并删除独立 crate。
- Data rollback: 新 crate 使用独立三张表；停止使用后可保留数据，不影响 P0/P1。
- Configuration rollback: 移除 Provider/Profile 注册配置。
- External-system rollback: 撤销 API Key 或 endpoint 配置；数据库不保存 Key。
- Recovery validation: P0/P1 regression 仍通过。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [x] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
