# P034 Enterprise ChatGPT / Claude Code Parity — Unknowns Report

## Metadata

- **Task / Feature:** 对标企业级 ChatGPT/Codex 与 Claude Code，补齐非 P033 Desktop 的核心 Agent 能力
- **Mode:** Deep
- **Date:** 2026-07-19
- **Prepared by:** Codex
- **Scope:** 规则链、Skills、Memory、内置 Tools、Agent 主链、权限/观测、MCP/Hooks/后台能力的分期边界；明确排除 P033 Desktop UI/配置/统计实现

## Intent

### User-visible problem

当前 `core-agent` 已能从 Terminal/Desktop 进入同一个本地 Agent 工具循环，但“有 Runtime 类型”还不等于企业级产品闭环：项目规范没有自动进入上下文，Skills 没有发现/延迟加载，Memory 没有在主聊天中持久召回与写入，内置工具缺少高效搜索与增量补丁；MCP、Hooks、OS 沙箱和后台/子 Agent 也尚未形成产品能力。

### Desired behavior change

- 每轮 Agent 工作前，确定性加载全局与项目路径上的 `AGENTS.md` 指令链，并保留来源、优先级和容量边界。
- 发现系统、用户和项目 Skills；初始上下文只放 name/description/path，真正调用时才加载完整 `SKILL.md`，脚本继续走受控 Tool/Permission，而不是被 Skill 隐式执行。
- Memory 以持久化、可审计、可召回、可遗忘的方式进入主链；项目/Session/Conversation 语义明确，不把强制规范仅保存在 Memory。
- 在已有 `list_files/read_file/write_file/run_command` 上补齐代码 Agent 最常用的 Glob/Grep/Edit 类工具，并复用工作区路径、敏感文件、CAS、审批、超时和输出上限。
- `Agent → Planning → Context → Memory → Tool → Workspace → Execution → Permission → Observation → Plugin` 中的产品请求不再绕过关键治理节点；不强迫每个简单问答生成形式化 Plan。
- 用事件和测试证明规则、Memory、Tool、Permission、Session 与失败路径真实贯通。

### Affected users and workflows

- Terminal/Desktop 用户：同一项目规范、Skill、Memory 和 Tool 行为，不因入口不同而漂移。
- 团队维护者：可把强制规则提交到仓库，把可复用流程打包为 Skill，把可遗忘经验保存在 Memory。
- 企业管理员/安全人员：后续可基于同一合同接入 managed policy、MCP allowlist、Hooks 和 OS sandbox。

### Success criteria

- 全局/项目/嵌套指令按稳定顺序合并；override、空文件、越界、符号链接、过大文件和 UTF-8 错误均有断言。
- Skill 目录只注入有界 metadata；调用时完整校验并加载，未知/重复/损坏 Skill fail-closed。
- Enterprise 组合根使用 `SqliteMemoryStore`；相关 Memory 在建模前进入 Context，成功写入后跨 Runtime 重开仍可召回；用户可查询和遗忘。
- 新增文件匹配、内容搜索和增量编辑工具；均不绕过当前路径/敏感文件/权限/checkpoint 边界。
- 规则、Memory、工具调用和权限决定产生带 request/session/source 的 Observation/Event。
- JUnit5 不适用于本 Rust/Vue 仓库；使用等价的 Rust `#[test]` / `#[tokio::test]` 单元断言、真实跨 Runtime E2E，并保持 Vue 既有测试门禁。
- P033 在改的 Desktop/Config/Model/Context/Telemetry 文件不被本任务覆盖；共享组合根只做可审计的小型集成改动。

### Non-goals

- 不修改 `agent-desktop/**` 的布局、主题、i18n、模型设置、Usage、Context 圆环或耗时 UI；这些属于 P033。
- 不把路径检查和人工审批描述成 OS/容器级 sandbox。
- 不在第一阶段同时交付 MCP、Hooks、LSP、后台任务、工作树隔离、集中 IAM/RBAC/Compliance API 和多节点 HA。
- 不把聊天原文、API Key、`.env`、凭据或完整工具输出无差别写入 Memory。
- 不为了“完整链路图”让简单问答产生无价值的 Planning/Execution 记录。

### Scope clarification after user confirmation

用户确认按 P0 → P1 → P2 分阶段开始全路线实现，并额外明确联网搜索与命令行执行是核心能力。因此原“Do not implement now”仅代表 Unknowns Discovery 时的推荐切片，不再是最终交付边界。本次采用：

- P0：AGENTS、Skills、SQLite Memory、find/search/apply_patch、受控前台命令和主链接线。
- P1：Hooks、MCP stdio client、真实 `web_search/web_fetch` provider 与引用来源。
- P2：bubblewrap 能力探测/fail-closed、后台命令、只读隔离子 Agent、managed policy；Windows 无可用 OS sandbox backend 时如实报告并允许策略强制拒绝。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Requested design | `design-docs/034-vs-cc-gpt-opt.md` | 明确要求 Tools、Memory、AGENTS/Skills 和完整核心链路对标 | High |
| Concurrent design | `design-docs/033-desktop-opt.md`、P033 Unknowns Report | P033 正在改 Config/Model/Context/Enterprise/CLI/Desktop/README/CHANGELOG，存在真实重叠 | High |
| Capability matrix | `design-docs/capability-traceability.md` | 主链为 Context→Model↔Tool；Memory 自动主链与 OS sandbox 明确未完成 | High |
| Composition root | `src/enterprise.rs` | 仅注册 4 个 workspace tools；Memory 使用默认内存 store；`run_with_approval` 直接执行 Context→Model→Tool | High |
| Config domain | `core-agent-config/src/domain.rs` | Memory 只有 `enabled: bool`，没有 scope、retention、recall/write 控制 | High |
| Memory runtime | `core-agent-memory/src/*` | 已有分类、索引、召回、忘记、SQLite、审计和策略合同，可复用 | High |
| Existing integration | `tests/memory_context_integration.rs` | `MemoryContextProvider` 已证明 Memory→Context 可工作，但未接入 Enterprise 主链 | High |
| Code search | production Rust/TS/Vue/config files | 没有 AGENTS/Skill loader；没有生产 MCP/Hook/Subagent 实现 | High |
| Prior implementation notes | P030/P031 | MCP/Hooks/LSP/后台任务被显式延期；read-only/checkpoint 已有安全边界 | High |
| OpenAI official docs | [AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md)、[Memories](https://learn.chatgpt.com/docs/customization/memories)、[Skills](https://learn.chatgpt.com/docs/build-skills)、[MCP](https://learn.chatgpt.com/docs/extend/mcp)、[Hooks](https://learn.chatgpt.com/docs/hooks)、[Sandbox](https://learn.chatgpt.com/docs/sandboxing) | 分层规则、Memory 与规则分离、Skill progressive disclosure、MCP、生命周期 Hook、Sandbox/Approval 分离是当前基线 | High |
| OpenAI enterprise docs | [Managed configuration](https://learn.chatgpt.com/docs/enterprise/managed-configuration)、[Compliance API](https://learn.chatgpt.com/docs/enterprise/compliance-api) | 企业级还包括不可覆盖的本地策略、allowlist 与审计导出 | High |
| Anthropic official docs | [Memory](https://code.claude.com/docs/en/memory)、[Permissions](https://code.claude.com/docs/en/permissions)、[Subagents](https://code.claude.com/docs/en/sub-agents)、[Hooks](https://code.claude.com/docs/en/hooks) | CLAUDE.md/auto-memory、受限工具、managed policy、Hooks、隔离子 Agent 是 Claude Code 当前基线 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| 产品主链当前只有四个内置工作区工具 | `register_workspace_tools` | 缺少代码 Agent 高频的文件匹配、内容搜索、增量编辑 |
| `EnterpriseAgent` 持有的 Memory 重启即丢失 | `MemoryManager::builder().build()` 默认 `InMemoryMemoryStore` | 与跨 Session Memory 目标直接冲突 |
| Enterprise Context 没有注册 Memory provider | `run_with_approval` 调 `ContextRuntime::build` | `memory.enabled=true` 目前只影响 `/memory` 状态文案，不影响回答 |
| 组合根直接进行 Model↔Tool 循环 | `run_with_approval` | Planning/Execution/Agent/Platform 多数是“已实例化”，不是每个请求的产品闭环 |
| Memory Runtime 已有足够的底层能力 | remember/recall/update/archive/forget/snapshot + SQLite | 应做组合与策略，不应重写一个平行 Memory 系统 |
| AGENTS/Skills 没有生产实现 | code search 无命中 | 用户要求的是新能力，而不是修一处接线 |
| MCP/Protocol 名称存在不代表 MCP client 已实现 | Protocol/Extension domain 只有描述合同 | 不能把协议类型当作第三方工具已可用 |
| 当前 `auto` 仍不是 sandbox | README 与能力矩阵明确说明 | OS 原生隔离必须独立设计、测试和声明 |
| P033 与 P034 都会触及 `src/enterprise.rs`、README、CHANGELOG | P033 Unknowns Report | 并发直接编辑会造成覆盖或语义回退 |
| 官方实现都把强制规则和可选 Memory 分开 | OpenAI/Anthropic official docs | AGENTS 不能依赖概率召回，Memory 也不能冒充 policy |

## Critical Unknowns

优先级使用 `Impact × Probability × Irreversibility × Late discovery cost`。

| Unknown | Category | Evidence / Reasoning | I | P | R | L | Score | Disposition | Recommended resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| 本轮是交付核心闭环，还是一次性覆盖全部企业差距 | Known unknown | MCP/Hooks/Sandbox/Subagent/Admin 各自都是独立平台能力 | 5 | 5 | 4 | 5 | 500 | Blocker | 选择 P0、P0+P1 或全路线；推荐先 P0 核心闭环 |
| 如何避免与 P033 共享文件冲突 | Known unknown | P033 明确会改 Config/Model/Context/Enterprise/CLI/README/CHANGELOG | 5 | 5 | 4 | 5 | 500 | Blocker | P034 先建独立模块与测试；共享文件仅在 P033 稳定后手术式集成并逐段核对 |
| 什么内容允许自动进入 Memory | Known unknown | 原始 prompt/tool output 可能含凭据、隐私和瞬时噪声 | 5 | 5 | 4 | 5 | 500 | Decision | 推荐“自动召回 + 受控 `remember_memory` 工具写入 + secret redaction + 用户可审计/forget”；不做原文后台总结 |
| Memory scope 如何映射 | Known unknown | 用户要求项目、Session、所有会话；现有 Runtime 只有 namespace 字符串 | 5 | 5 | 3 | 5 | 375 | Decision | 冻结 `user/project/session` namespace；Conversation 继续以 Session message 为权威，不重复长期存储全文 |
| 是否兼容 `CLAUDE.md` | Known unknown | 目标同时对标两者，但用户明确点名全局 `AGENTS.md` | 4 | 4 | 2 | 4 | 128 | Decision | 推荐本期 canonical `AGENTS.md`/override；保留可插拔 fallback，不默认同时加载两套冲突规则 |
| Skill 是否可自行执行脚本 | Unknown known | Skill 常包含 scripts；仓库当前没有 Skill trust 管理 | 5 | 4 | 4 | 5 | 400 | Decision | Skill 仅提供指令/资源；脚本必须由受控 Tool 显式执行并经过 Permission/Observation |
| 新 Tool 的最小集合与名称 | Known unknown | 现有四个工具能完成任务但低效且覆盖写风险高 | 4 | 5 | 2 | 4 | 160 | Decision | P0 增加 `find_files`、`search_files`、`apply_patch`；Web/MCP/Notebook/LSP 后置 |
| 是否让所有请求强制经过 Planning | Unknown known | 企业链路图包含 Planning，但简单问答不需要计划 | 4 | 4 | 3 | 4 | 192 | Decision | Planning 按请求类型/显式命令触发；所有请求必须经过 Context/Memory/Permission/Observation |
| P0 是否包含 OS 原生 sandbox | Known unknown | 当前只有应用层边界；跨 Windows/macOS/Linux 执行隔离不可用单一 Rust path guard 替代 | 5 | 5 | 5 | 5 | 625 | Decision | 推荐独立 P2；本期只冻结 `SandboxExecutor`/capability 合同且继续如实标注非 sandbox |
| MCP/Hooks 先做哪一个 | Known unknown | 两者都扩展攻击面，且 managed allowlist/trust 是企业必需条件 | 5 | 4 | 4 | 5 | 400 | Decision | 推荐 P1 先 Hook 事件合同与 trust，再 MCP client/allowlist；均不塞进 P0 |
| Memory/Skill/Instruction 容量如何避免挤占模型上下文 | Unknown unknown candidate | Skills 多、Memory 多、规则深时会挤掉任务正文 | 4 | 5 | 2 | 4 | 160 | Decision | 每类独立预算、metadata progressive disclosure、stable truncation、超限事件与测试 |
| 污染/投毒如何恢复 | Unknown unknown candidate | 仓库文件或模型可能诱导写入错误规则/Memory | 5 | 4 | 3 | 5 | 300 | Decision | 规则只读加载；Memory 保留 provenance/version、list/forget；Skill 项目源受 workspace trust 约束 |
| Observation 写入失败是否让成功任务失败 | Existing architectural unknown | 外部副作用可能已完成，盲目重试会重复 | 5 | 3 | 3 | 5 | 225 | Decision | 副作用完成后观测 fail-open 但发出 degraded marker；权限/规则/Memory policy 评估继续 fail-closed |

## Competing Solution Models

### Model A — 推荐：P0 核心闭环

- `Instructions`：全局 + project-root→cwd 的 AGENTS chain，override、provenance、size budget。
- `Skills`：system/user/project discovery，metadata 常驻、完整内容按需加载、无隐式执行。
- `Memory`：SQLite 项目持久化，自动召回；受控工具写入/查询/遗忘；scope、redaction、provenance 和事件。
- `Tools`：补齐 `find_files/search_files/apply_patch`，复用当前安全与 checkpoint。
- `Chain`：在 Enterprise 请求中接入 instructions/memory/skill catalog/observation；Planning 条件触发。
- 不改 P033 Desktop 页面；Terminal/Desktop 通过共享 Runtime 自动获得相同行为。

**优点：** 直接修复 034 最核心、最有产品价值的差距，范围可真实 E2E。  
**代价：** MCP/Hooks/Sandbox/Subagent 仍是明确后续项。

### Model B — P0 + P1 扩展闭环

在 Model A 上再加入 lifecycle Hooks 与 MCP client/allowlist、STDIO/HTTP transport、凭据和超时治理。

**优点：** 第三方工具生态接近 Codex/Claude Code。  
**代价：** 安全、配置、进程生命周期和 P033 冲突显著增加；应拆成独立设计与验收。

### Model C — 一次性交付全部企业路线

再加入 OS sandbox、后台任务、子 Agent/worktree、managed settings、IAM/RBAC、审计导出和远程控制。

**优点：** 路线完整。  
**代价：** 不能在一个可审计的小版本内真实完成；高概率产生“类型已存在但产品未打通”的重复问题。

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| P033 拥有所有 Desktop UI 文件 | 用户已明确另一 AGENT 实现 033 | P034 不编辑 `agent-desktop/**` |
| 强制规则只来自文件，不来自 Memory | 官方基线与安全语义一致 | 后续可增加 managed policy provider，但仍高于 Memory |
| Conversation 正文继续由 Session Runtime 管理 | 已有持久化、恢复和 Context 逻辑 | Memory 只保存提炼条目，不复制 transcript |
| 新工具继续走统一 Tool Runtime | 现有审批/记录/checkpoint 可复用 | Provider 可替换，不改模型 tool-call contract |

## Recommended Implementation Boundary

### Implement now（Model A / P0）

- 新增独立 instructions 与 skills 领域/加载模块，先保持与 P033 低耦合。
- 新增有界、安全、可测试的 `find_files/search_files/apply_patch` provider。
- Enterprise Memory 切换到 SQLite，接入 recall→Context 和 governed write/list/forget tools。
- 在统一请求中传播 request/session/workspace/source，补齐 instruction/memory/tool/permission observations。
- 条件式连接 Planning/Execution，不给简单问答制造形式化噪声。
- Rust 单元断言 + 跨 Runtime/CLI E2E；Desktop 只验证共享 bridge 合同，不改 P033 页面。
- 完成后更新 034、capability matrix、README、CHANGELOG、Implementation Notes、Post-Implementation Review。

### Do not implement now

- P033 UI/配置/统计/上下文压缩/耗时相关实现。
- MCP、Hooks、LSP、Web/Computer Use、后台任务、子 Agent/worktree、OS sandbox、集中 IAM/Compliance。
- 原始对话后台总结、自动网络同步 Memory 或执行 Skill 自带脚本。

### Interfaces or data contracts to freeze

- `InstructionSource/InstructionChain`：scope、path、precedence、content hash、bytes、loaded_at。
- `SkillDescriptor/LoadedSkill`：name、description、scope、path、content hash、resources；metadata/full-content 分离。
- `MemoryScope`：user/project/session；namespace 稳定映射、provenance、redaction、retention、forget。
- Tool schema：find/search/patch 输入上限、输出上限、workspace path、expected hash 和 permission category。
- Observation：request_id、session_id、workspace key、stage、source、decision、degraded/error，不保存敏感正文。

### Areas that must remain reversible

- Instructions/Skills provider 可从 Context pipeline 移除，不改 Session 数据。
- Memory 使用独立 SQLite；禁用/损坏时不删除 Session，迁移事务化。
- 新工具独立注册；移除不会破坏已有四工具合同。
- P033 共享文件集成通过小型 adapter/constructor 完成，避免重写其 schema 和 UI。

## Verification Plan

### Automated

- **Instruction unit assertions:** precedence、override、nested cwd、fallback、UTF-8、size、symlink、empty、hash/provenance。
- **Skill unit assertions:** discovery scopes、frontmatter、duplicate precedence、lazy load、resource boundary、损坏/超限、无隐式 script。
- **Tool unit assertions:** glob/grep stable ordering、binary/UTF-8、ignore rules、regex/limit、patch CAS、path traversal、sensitive files、checkpoint。
- **Memory unit assertions:** scope namespace、redaction、eligibility、recall ranking、forget、SQLite reopen、audit columns/indexes/no FK。
- **Enterprise E2E:** AGENTS+Skill metadata+recalled memory 进入一次模型请求；模型调用 search/patch/remember；审批、checkpoint、事件、Session 终态正确；重开后 recall，forget 后不再出现。
- **Permission/failure E2E:** read-only 命令看不到 patch/memory-write；拒绝/超时/损坏 store/观察写失败不产生错误终态或重复副作用。
- **Final gates:** `cargo fmt --all -- --check`、`cargo test --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、P033 合并后的 `npm test`/build、`git diff --check`。

### Manual

- 在用户级与嵌套项目级放置不同 AGENTS，确认来源和覆盖顺序。
- 安装同名 system/user/project Skill，确认 precedence 与按需加载。
- 两个 Session 验证 project memory 跨会话召回、session memory 隔离和 forget。
- 大仓库用 search/find，确认响应速度、截断标记、ignore 与敏感路径。
- patch 后手工改文件再重复 patch，确认 CAS 冲突拒绝覆盖。
- 同时观察 P033 工作区变更，确认 P034 不覆盖 Desktop/Config/Telemetry 代码。

### Three-pass review required after implementation

1. **Correctness/architecture:** precedence、scope、主链接线、持久化、幂等、Planning 条件。
2. **Security/compatibility:** prompt injection、secret redaction、symlink/path、Skill trust、permission/read-only、P033 diff。
3. **Performance/maintainability:** context budget、大仓搜索、Memory growth、事件噪声；仅做手术式小优化。

## Rollback and Recovery

- 新模块注册可撤销，保留原四工具与 Session DB。
- Memory 数据库迁移使用事务；打开失败不修改旧文件；forget 使用 tombstone/索引清理，不硬删审计证据。
- AGENTS/Skill 加载失败默认不执行不可信内容，并产生可见诊断；不静默降级到越界文件。
- 与 P033 冲突时，以 P033 的 Config/Model/Context/UI 语义为基线，重新应用 P034 adapter，不覆盖其文件版本。

## Decisions Required

| Decision | Options | Recommendation |
|---|---|---|
| 本轮范围 | P0 核心闭环 / P0+P1 / 全路线 | P0 核心闭环 |
| Memory 写入 | governed tool / 后台总结 / 仅用户显式 | governed tool + 自动 recall |
| 文档/包版本 | 0.34.0 / 0.33.x / 自定义 | 0.34.0，避免与 P033 版本语义冲突 |

## Handoff

- [x] Intent / non-goals
- [x] Repository and official-doc evidence
- [x] Confirmed facts and gap ranking
- [x] Competing solution models
- [x] P033 conflict boundary
- [x] Verification / rollback / three-pass review
- [ ] User confirms scope, Memory write policy and version
- [ ] Implementation notes started
- [ ] Implementation and full verification completed
