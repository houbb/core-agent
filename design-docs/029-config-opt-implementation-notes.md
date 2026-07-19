# P029 配置优化 — Implementation Notes

## Metadata

- **Task / Feature:** 可扩展配置 Runtime、全局配置、session、`@` 与 `/`
- **Date started:** 2026-07-19
- **Implementation owner:** Codex
- **Related Unknowns Report:** `design-docs/029-config-opt-unknowns-report.md`
- **Related plan / issue / PR:** P029

## Confirmed Discoveries

### Discovery D-001

- **What was discovered:** 仓库没有统一配置来源/合并抽象。
- **Evidence:** 全仓不存在 `ConfigProvider`/`ConfigResolver`；Kernel `ConfigSnapshot` 只用于 Runtime reload，且主动拒绝密钥。
- **Why it matters:** 直接把用户 YAML 读取塞入 CLI/Desktop 会固化实现策略并造成入口漂移。
- **Affected scope:** 新 `core-agent-config`、根组合入口、CLI、Desktop。
- **Action taken:** 新建独立 crate，稳定接口与文件/env 策略分离。

## Decisions

### Decision DEC-001

- **Decision:** 核心依赖强类型 `AgentConfig`，不依赖 YAML/JSON/环境变量。
- **Alternatives considered:** CLI/Desktop 各自读取；根 crate helper；独立配置 crate。
- **Reason:** 独立 crate 能维持依赖方向，并允许未来远程配置、数据库和 vault provider 替换。
- **Evidence:** Roadmap 明确 `core-config` 是底层依赖；现有 Runtime 均采用接口 + 实现模式。
- **Owner / approver:** Architecture；用户明确要求。
- **Reversibility:** Provider 可替换；强类型 schema 版本化。
- **Follow-up:** 真实实现与测试完成后更新。

### Decision DEC-002

- **Decision:** precedence 固定为 builtin(0) < user file(100) < project file(200) < environment(300)。
- **Alternatives considered:** global 覆盖项目；按加载顺序隐式覆盖。
- **Reason:** 符合通用配置习惯，显式 priority 可测试、可扩展。
- **Evidence:** 当前既有项目配置和 Desktop env 都需要兼容。
- **Owner / approver:** Architecture。
- **Reversibility:** priority 属于 provider metadata，可版本化调整。
- **Follow-up:** 合并契约测试。

## Assumptions

### Discovery D-002

- **What was discovered:** 用户明确要求 `@` 同时支持文件和文件夹，且 `/` 与 `@` 不能由 Terminal/Desktop 各实现一套。
- **Evidence:** P029 实施期间的用户补充要求。
- **Why it matters:** 仅在 CLI parser 或 Desktop controller 处理会形成入口漂移，文件夹递归也必须复用工作区安全策略。
- **Affected scope:** 根组合层 `interaction`、EnterpriseAgent、CLI Professional、Desktop IPC。
- **Action taken:** 下沉可注册命令目录、统一解析/路由/Prompt 展开与有界 mention resolver；入口只适配输入和展示。

## Deviations

暂无。

## Unresolved Risks

| Risk | Impact | Current mitigation | Owner | Review trigger |
|---|---:|---|---|---|
| plaintext `apiKey` 受用户目录权限保护而非 OS vault | 4 | 输出/Debug 脱敏、拒绝 symlink、限制文件、文档建议 ACL/轮换 | Security | 引入 credential provider |

## Tests Added or Updated

| Test | Purpose | Result |
|---|---|---|
| Pending | Provider/merge/redaction/session/mention/slash/双入口/live DeepSeek | Pending |

## Rollback Notes

- Code rollback: 移除 `core-agent-config` 依赖，CLI 恢复项目 YAML，Desktop 恢复 env。
- Data rollback: 配置与 session schema 不做破坏迁移。
- Configuration rollback: 删除/移动用户全局文件即可回到内置/项目策略。
- External-system rollback: 轮换 DeepSeek Key。
- Recovery validation: 旧 `.agent/config.yaml` E2E。

## Knowledge Capture

- [x] Tests
- [x] Documentation
- [ ] Architecture decision record
- [ ] Schema constraint
- [ ] Static analysis rule
- [x] Reusable component
- [ ] AGENTS.md rule
- [ ] Another Skill
