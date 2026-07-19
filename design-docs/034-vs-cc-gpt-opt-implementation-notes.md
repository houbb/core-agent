# P034 Enterprise ChatGPT / Claude Code Parity — Implementation Notes

## Metadata

- **Task / Feature:** 非 P033 的企业级 Agent 核心、扩展与治理能力
- **Date started:** 2026-07-19
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/034-vs-cc-gpt-opt-unknowns-report.md`
- **Related plan / issue / PR:** P034

## Confirmed Discoveries

### Discovery D-001

- **What was discovered:** `EnterpriseAgent` 只注册 `list_files/read_file/write_file/run_command`，Memory 使用默认内存 store 且未进入 Context。
- **Evidence:** `src/enterprise.rs` 的 `register_workspace_tools`、`EnterpriseRuntimes` 和 `run_with_approval`。
- **Why it matters:** 034 的主要问题是缺少产品接线，而不是底层 Memory 类型缺失。
- **Affected scope:** 新核心模块、根组合层、Enterprise E2E。
- **Action taken:** 复用现有 Tool/Memory/Context 合同，先隔离实现后接线。

### Discovery D-002

- **What was discovered:** P033 计划同时修改 Config/Model/Context/Enterprise/CLI/Desktop/README/CHANGELOG。
- **Evidence:** `design-docs/033-desktop-opt-unknowns-report.md`。
- **Why it matters:** 直接并发重写共享文件会覆盖另一个 AGENT 的语义。
- **Affected scope:** `src/enterprise.rs`、`src/lib.rs`、README、CHANGELOG 和最终验证。
- **Action taken:** P034 独立模块优先；共享文件只做手术式接线，并在每次编辑前复查工作区状态。

### Discovery D-003

- **What was discovered:** OpenAI 与 Claude Code 都把联网检索和本地代码搜索分成独立工具；OpenAI 当前 Shell 指南也明确把命令运行时、审批和 sandbox 视为互补能力。
- **Evidence:** OpenAI Web Search/Shell/Apply Patch 官方指南；Claude Code Tools/Sandbox 官方指南。
- **Why it matters:** `search_files` 不能冒充联网搜索，路径边界也不能冒充 OS sandbox。
- **Affected scope:** `web_runtime`、`command_runtime`、Tool 注册和 README 安全声明。
- **Action taken:** 新增真实 `web_search/web_fetch` provider、来源 URL、域名策略与 SSRF 防护；命令运行时单独提供结构化 stdout/stderr/exit、取消、后台任务和 sandbox capability。

### Discovery D-004

- **What was discovered:** MCP stdio server 和 Hooks 都会扩大本地代码执行面，且第三方 server 常需显式继承某几个凭据环境变量。
- **Evidence:** MCP 初始化/tools/list/tools/call 生命周期；Hook 命令配置与现有 Tool Permission 合同。
- **Why it matters:** 默认继承全部环境或仓库打开即执行 Hook 都不可接受。
- **Affected scope:** `mcp_runtime`、`hook_runtime`、managed policy。
- **Action taken:** 两者均需显式环境开关；Hook 复用统一 CommandRunner；MCP 默认脱敏环境，只允许配置列出的环境变量名重新继承，并由 managed server allowlist 再约束。

### Discovery D-005

- **What was discovered:** P033 的首轮新增遥测查询在 `core-agent-model/src/persistence/store.rs` 出现 Rust 临时值生命周期编译错误。
- **Evidence:** `cargo test -p core-agent --lib --tests` 的 E0597，位置为 P033 新增 `list_request_metrics/usage_buckets`。
- **Why it matters:** workspace 在进入 P034 编译前已被并行改动阻断。
- **Affected scope:** 最终统一验证，不属于 P034 语义实现。
- **Action taken:** 暂不覆盖 P033 文件，继续本侧静态 review；待 P033 稳定后复跑并仅在仍有必要时做最小集成修正。

## Decisions

### Decision DEC-001

- **Decision:** 按 P0 核心闭环 → P1 Hooks/MCP → P2 sandbox/background/managed policy 三阶段实施。
- **Alternatives considered:** 只做 P0；P0+P1；一次性耦合全部能力。
- **Reason:** 用户同时列出三档并要求开始实现；分阶段能保留每层真实验收和回退边界。
- **Evidence:** 用户 2026-07-19 回复；Unknowns Report competing models。
- **Owner / approver:** 用户 / Architecture。
- **Reversibility:** 每层独立模块和注册入口，可单独禁用。
- **Follow-up:** 每阶段完成后记录验证结果。

### Decision DEC-002

- **Decision:** Memory 采用自动召回 + governed tool 写入/list/forget，不做原始对话后台总结。
- **Alternatives considered:** 后台额外模型总结；仅用户显式记忆。
- **Reason:** 在跨 Session 体验与隐私/成本之间保持可审计边界。
- **Evidence:** Unknowns Report 推荐项；OpenAI/Anthropic 都将强制规则与 Memory 分离。
- **Owner / approver:** Architecture；用户未覆盖推荐默认值。
- **Reversibility:** Memory tools 和 provider 可注销；Session transcript 不依赖 Memory。
- **Follow-up:** 用 secret redaction、provenance、forget 与重开 E2E 验证。

## Assumptions

### Assumption A-001

- **Assumption:** 版本使用 `0.34.0`。
- **Why it is currently acceptable:** 与 P034 编号一致，且避免占用 P033 版本语义。
- **Risk:** 用户可能希望其他发布策略。
- **How it will be validated:** 最终 CHANGELOG 明确单独 P034 条目；用户可在完成前覆盖。
- **Reversal plan:** 文档版本字符串可独立修改，不影响数据 schema。

### Assumption A-002

- **Assumption:** P033 拥有 `agent-desktop/**` 的 UI 与配置交互实现。
- **Why it is currently acceptable:** 用户明确要求避免与另一 AGENT 冲突。
- **Risk:** 共享组合根仍需小范围接线。
- **How it will be validated:** 编辑前后复查 `git status/diff`，不覆盖未知变更。
- **Reversal plan:** P034 adapter 可从共享文件中单独移除。

## Deviations

暂无。

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| P033 在 P034 接线期间修改共享文件 | 5 | 独立模块优先、每次编辑前复查 diff | Codex | 任何共享文件出现外部变更 |
| OS 原生 sandbox 在当前平台无可用 backend | 5 | capability 检测与 fail-closed，不以 path guard 冒充 sandbox | Security | P2 实施 |
| MCP/Hook 配置扩大代码执行面 | 5 | allowlist、显式 trust、超时、脱敏环境、统一 Permission | Security | P1 实施 |

## Tests Added or Updated

| Test | Purpose | Result |
|---|---|---|
| Pending | AGENTS/Skills/Tools/Memory/Hooks/MCP/Sandbox/Background/Policy 单元与 E2E | Pending |

## Rollback Notes

- Code rollback: 按模块取消注册，保留原四工具和现有 Session 主链。
- Data rollback: Memory/后台任务使用独立存储；不删除或重写 Session 数据。
- Configuration rollback: 新配置缺失时使用安全默认；未知配置 fail-closed。
- External-system rollback: MCP/Hook 断开或禁用不影响内置工具。
- Recovery validation: 重开 Runtime、损坏存储、拒绝权限和取消后台任务 E2E。

## Knowledge Capture

- [ ] Tests
- [ ] Documentation
- [ ] Architecture decision record
- [ ] Schema constraint
- [ ] Static analysis rule
- [ ] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
