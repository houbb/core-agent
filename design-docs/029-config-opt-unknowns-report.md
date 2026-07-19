# P029 配置优化 — Unknowns Report

## Metadata

- **Task / Feature:** 全局配置、项目/session 语义、`@` 上下文与 `/` 内置命令
- **Mode:** Standard
- **Date:** 2026-07-19
- **Prepared by:** Codex
- **Scope:** `core-agent` 组合入口、Terminal、Tauri Desktop、全局用户配置与真实 DeepSeek 验证

## Intent

### User-visible problem

当前 Terminal 必须先在每个项目执行 `agent init`，模型配置和 API Key 间接配置在项目目录；Desktop 又仅从环境变量读取。用户无法像 Claude Code/OpenCode 一样配置一次模型后直接打开任意目录，也缺少 Terminal/Desktop 一致的 `@文件` 上下文和本地 `/命令` 体验。

### Desired behavior change

- 默认发现用户目录下 `~/core-agent/core-agent-config.yaml|yml|json`。
- 全局 API Key 只配置一次，项目只保存必要覆盖项。
- 未初始化 `.agent` 的目录也可直接运行。
- 每次新建 chat 得到新 session；同一 chat 的全部消息共享 session；项目数据互不串联。
- Terminal/Desktop 都通过同一核心实现支持 `@相对文件或文件夹` 上下文和核心 `/` 内置命令。
- 配置、密钥、迁移与启动路径均有真实测试证据。

### Affected users and workflows

- Terminal：`init/run/chat/config`、交互式 `/` 命令、恢复 session。
- Desktop：启动时加载配置、项目会话隔离、Console 输入框。
- Runtime：Context 构建、模型配置、权限模式与数据目录选择。

### Success criteria

- 仅有全局配置、项目没有 `.agent/config.yaml` 时 Terminal 与 Desktop 均能调用真实 DeepSeek。
- YAML/JSON 均可加载，冲突、畸形、超大、符号链接配置 fail-closed。
- CLI 显示配置和 Debug 日志不泄露 API Key。
- `@marker.txt` 与 `@src/` 能把有界 UTF-8 内容注入模型上下文；越界、敏感、符号链接和超限资源被拒绝。
- `/new` 不调用模型并切换 session；同一 chat 后续消息继续使用新 session。
- Desktop 不同项目使用不同持久化目录。

### Non-goals

- 不实现云端配置同步、账户系统或跨设备密钥托管。
- 不实现模糊文件搜索/IDE 级 `@` 自动完成。
- 不把 `@` 文件当成可信指令，不绕过既有权限和工作区边界。
- 不声明 plaintext 用户配置等同于系统凭据保险库。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Design | `design-docs/000-roadMap.md` | Context/Workspace/Permission/Session 必须保持独立 Runtime 边界 | High |
| Design | `design-docs/029-config-opt.md` | 全局配置、全局 API Key、session、`@`、`/`、双入口真实验收 | High |
| Code | `agent-cli/src/config.rs` | 当前只读项目 `.agent/config.yaml`，文件缺失即失败 | High |
| Code | `agent-cli/src/app.rs` | chat 通过 `.agent/sessions.json` 复用 current session | High |
| Code | `agent-cli/src/professional.rs` | 已有 slash registry，但缺少 help/new/clear/sessions，部分本地命令会进入模型 | High |
| Code | `agent-desktop/src-tauri/src/lib.rs` | Desktop 只从环境变量读模型，所有项目共用 app-data/runtime | High |
| Code | `src/enterprise.rs` | Context 可通过 `BuildContextRequest.user_input`安全补充显式上下文 | High |
| Code | `core-agent-kernel/src/config.rs` | 现有 `ConfigSnapshot` 是 Runtime reload 的非敏感值快照，不是配置来源/合并模块 | High |
| Tests | CLI/Desktop/Enterprise E2E | 现有单进程入口和 session/tool 真实链路可作为回归基础 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| CLI 每项目必须配置 | `CliConfig::load` 无 fallback | 必须引入全局发现与无项目配置启动 |
| CLI chat 当前可能沿用之前 `run` 的 session | `LocalSessionState.current_session_id` | 需要定义“新 chat”边界 |
| Desktop session 数据会跨项目混放 | 固定 `app_data/runtime` | 必须按 canonical workspace 派生数据目录 |
| API Key 当前不会持久化进 Runtime DB | Model config/API 仅在内存 | 全局配置加载后仍应保持此不变量 |
| `/` 命令已有注册/解析基础 | `CommandRegistry` | 应扩展而不是新增第二套 parser |
| Context 支持当前输入补充槽 | `BuildContextRequest.user_input` | `@` 文件正文可不写入 Session 消息正文 |
| 仓库没有统一配置 provider 契约 | 全仓 `ConfigProvider/ConfigSource/ConfigResolver` 检索为空 | 必须新增稳定接口，不能把 YAML 读取写进核心入口 |

## Critical Unknowns

| Unknown | Category | Evidence / Reasoning | Impact | Probability | Irreversibility | Late discovery cost | Priority | Disposition | Resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 全局/项目/环境变量谁覆盖谁 | Known unknown | 多入口已有项目配置和 Desktop env | 5 | 5 | 2 | 5 | 250 | Decision | 固定为 env > project > global > builtin，并做合并测试 |
| 用户目录同时存在 YAML/JSON | Unknown unknown candidate | 文档允许两种格式但未定义优先级 | 4 | 3 | 2 | 4 | 96 | Decision | 拒绝歧义；显式 `CORE_AGENT_CONFIG` 可选定单文件 |
| 全局 API Key 的落盘风险 | Known unknown | 用户明确要求配置内全局可用 | 5 | 4 | 3 | 5 | 300 | Decision | 允许 `apiKey`，限制文件/大小/符号链接，Debug/输出脱敏，文档提示 ACL 与轮换 |
| 新 chat 是否恢复旧 session | Unknown known | 同一会话共享与“每次打开新会话”同时存在 | 5 | 4 | 2 | 4 | 160 | Decision | 默认新 session；`session.resumeLast` 可恢复；同一进程内始终复用 |
| `@` 是否可能读取仓库外/密钥 | Unknown unknown candidate | prompt parser 位于权限层上方，且文件夹会递归扩展 | 5 | 4 | 2 | 5 | 200 | Decision | canonical workspace、敏感名、UTF-8、目录深度/文件数/单文件/总量上限，失败不落 Session |
| `@` 正文是否持久化 | Known unknown | Session history 与 Context 的职责不同 | 4 | 4 | 3 | 4 | 192 | Decision | Session 只存原始 prompt，解析正文只进入本轮 User Context |
| Desktop `/` 命令是否调用模型 | Known unknown | 当前所有输入都会 `sendMessage` | 3 | 5 | 2 | 3 | 90 | Decision | new/clear/help/sessions/tools/status 本地处理，其余保持 Agent 命令 |
| Desktop 项目数据隔离方式 | Unknown unknown candidate | 全局 app data 不能直接使用项目 `.agent` | 5 | 4 | 3 | 5 | 300 | Decision | canonical workspace SHA-256 派生 app-data/projects/<hash>，稳定且不泄露完整路径 |
| 配置实现策略未来变化是否侵入核心 | Known unknown | 当前不存在配置抽象，Kernel snapshot 职责不同 | 5 | 5 | 4 | 5 | 500 | Decision | 新增 `core-agent-config`；核心只接收强类型 snapshot，来源与密钥均为 trait 实现 |

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 打开目录即可工作，不要求 init | 对标产品的第一启动体验 | 无 `.agent` 的真实 CLI/Desktop 测试 |
| `/` 是零模型成本的即时操作 | Claude Code/OpenCode 交互惯例 | 断言本地命令不产生模型调用 |
| `@` 文件可审计但不污染历史 | Context 与 Session 应职责分离 | 检查 Session 原文和 Context snapshot |
| 配置错误应指出具体文件 | 用户只配置一次，排错必须直接 | 错误契约和畸形配置测试 |

## Blind-Spot Candidates

| Candidate | Why it may matter | Validation method |
|---|---|---|
| HOME/USERPROFILE 不存在 | service/CI 环境可能无用户目录 | `CORE_AGENT_HOME` 覆盖与缺失错误测试 |
| 配置是符号链接或超过限制 | 可能绕过预期信任边界/耗尽内存 | symlink/size 单测 |
| Windows 路径大小写/短路径 | 项目 hash 和 containment 可能不稳定 | canonical path 派生测试 |
| 文件在解析后、模型调用前变化 | Context 是时间点快照 | 附带 SHA-256，并明确本轮快照语义 |
| `user@example.com` 被误识别为 mention | 常见 prompt 内容 | parser 单测 |
| Desktop IPC 决定期间 `/new` | 当前 operation lock 串行 | UI sending 状态禁止重复操作，保留监控 |

## Decisions Required

| Decision | Options | Trade-offs | Recommended owner | Deadline / Trigger |
|---|---|---|---|---|
| 配置优先级 | global-first / project-first / env-first | 兼容性与可预测性 | Architecture | 实现前；采用 env > project > global |
| 配置抽象边界 | 直接文件读取 / 共享 helper / 独立 Runtime crate | 扩展性与依赖稳定性 | Architecture | 采用独立 `core-agent-config` crate |
| API Key 存储 | plaintext config / env ref / OS vault | 开箱体验、跨平台、安全 | Security/User | 用户已要求 config；支持 plaintext + env，输出脱敏 |
| session 默认 | resume / new | 连续性与会话隔离 | Product | 采用 new，配置可切换 |

## Experiments or Prototypes Required

| Question | Method | Success signal | Cost | Owner |
|---|---|---|---|---|
| DeepSeek 是否使用 `@` 注入正文正确回答 | 真实 opt-in E2E | 返回随机 marker，且无 read_file tool call 也可成功 | Low | Implementation |
| 无项目配置是否可启动 | 临时 HOME + 临时 workspace binary E2E | `agent run` 成功且只创建运行态项目数据 | Low | Implementation |
| Desktop local slash 是否零模型 | Controller fake API 测试 | sendMessage 调用次数为 0 | Low | Implementation |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| YAML 为推荐格式 | 可读性更好，JSON 同时支持 | 文档切换即可，不影响 schema |
| `CORE_AGENT_CONFIG`/`CORE_AGENT_HOME` 作为测试和高级覆盖 | 不改变默认发现路径 | 移除环境覆盖即可 |
| `@` 文件夹只展开普通 UTF-8 文件 | 可复用文件上下文合同并保持确定性 | 后续注册其他 Context Provider，不改变入口 |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| OS vault 自动写入 | 跨平台产品决策，不是 P029 验收必要条件 | 后续 credential provider Runtime |
| `@` fuzzy picker/自动完成 | 需要 Terminal TUI 与 Desktop picker | 后续交互 P |
| 云同步全局配置 | 需要身份与冲突解决 | Platform/IAM 阶段 |

## Recommended Implementation Boundary

### Implement now

- 共享全局配置 schema/discovery/redaction。
- `ConfigProvider`、`SecretResolver`、`ConfigManager` 与不可泄密的 `ResolvedConfig` 稳定契约；File/Environment/Defaults 作为策略实现。
- CLI 项目覆盖合并和无 `.agent` 启动。
- Desktop 全局配置加载、项目数据 hash 隔离、有效配置展示。
- Runtime 有界 `@file`/`@folder` Context 注入。
- Terminal/Desktop 复用同一可注册命令目录和执行路由，不维护两套 `/` 实现。
- 全局实际 DeepSeek 配置、双入口与真实 Provider 验证。

### Do not implement now

- 云同步、OS vault、模糊检索、网络上下文 mention。

### Interfaces or data contracts to freeze

- `version/model/permissions/memory/session/context` 全局 schema。
- 核心只依赖 `AgentConfig`；Provider 只产生分层 patch，Resolver 按 priority 合并。
- env > project > global > builtin 优先级。
- Session 存原始 prompt，mention 正文仅在 Context snapshot。

### Areas that must remain reversible

- 默认 provider/model、session.resumeLast、mention 上限。
- 全局配置 YAML/JSON 文件名和显式环境覆盖。

## Verification Plan

### Automated

- Unit tests：发现/合并/脱敏、mention parser、slash registry、session reset、项目 hash。
- Integration tests：无项目配置 CLI、全局覆盖、同 chat session、Desktop local slash。
- Migration tests：旧完整 `.agent/config.yaml` 仍可加载并覆盖全局。
- Contract tests：敏感路径、symlink、文件/总量上限、歧义格式。
- Static analysis：fmt、Clippy `-D warnings`、vue-tsc、Vite、npm audit。

### Manual

- Happy path：真实用户全局配置启动 Terminal/Desktop。
- Empty state：无项目 `.agent`。
- Failure path：空/错误 Key、畸形/冲突配置、未知 mention。
- Recovery path：修正配置后重启。
- Permission boundaries：`@.env`、`@../file`、命令审批。
- Accessibility：Desktop 本地命令结果仍进入可读 conversation。

### Observability

- 事件记录 mention 数量、路径和 hash，不记录 API Key。
- `/config` 只输出来源与脱敏有效配置。
- 启动错误包含配置路径，不包含密钥正文。

## Handoff

- [x] Acceptance criteria
- [x] Explicit invariants
- [x] Data and interface contracts
- [x] Test cases
- [x] Rollback requirements
- [x] Observability requirements
- [x] Non-goals
- [ ] Implementation notes file
