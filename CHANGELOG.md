# CHANGELOG

## [Unreleased]

### P032: Unified Desktop Workspace Experience

- Desktop 新增系统目录选择器与进程内工作区切换：按新目录重新解析有效配置、隔离 Runtime 数据、清空旧 UI session，并默认拒绝旧 Runtime pending approval；不启动额外 Runtime 子进程。
- Console 新增共享 `/` 命令候选和 `@` 文件/文件夹模糊候选；至少 3 个字符才查询核心预索引，`↑/↓` 选择、`Tab/Enter` 只补全、Shift+Enter 换行，项目树可直接 `Add @`。
- 用户原始消息继续在发送前显示，并为用户/Agent 消息增加显式复制；候选逻辑提取为可断言纯函数，Desktop 不实现第二套磁盘扫描或命令语义。

### P031: Read-only Plan and Durable File Checkpoints

- `/plan`、`/review`、`/explain`、`/commit`、`/pr` 增加 Runtime 强制只读边界：工具声明移除写能力，执行前再次拒绝写调用及非白名单命令。
- `write_file` 新增 session/request 级持久化 Checkpoint 和崩溃可恢复 pending journal；同轮同文件保留首个 before 与最终 after，历史/文件数/体积全部有界。
- 核心注册 `/undo`、`/redo`，Terminal/Desktop 复用同一路由；整组文件恢复执行 SHA-256 CAS，手工修改、越界、符号链接和损坏快照均 fail-closed，不触碰 Git index，也不声称回退 shell/网络副作用。

### P030: Full-screen Terminal Experience

- 将 `agent chat` 从裸 `stdin` 行循环升级为 Ratatui 全屏终端应用：新增 Core Agent ASCII 品牌区、自适应 Conversation、Message 输入框、状态栏、内存输入历史、滚动和忙碌反馈；TTY 使用视觉 TUI，脚本/非 TTY/`--no-color` 保持纯文本兼容。
- `/` 命令面板直接读取核心 `InteractionCommandRegistry`；`@` 使用启动时预建的最多 20,000 文件 git-aware 安全索引，至少 3 字符才在内存模糊过滤文件/文件夹，最终内容仍由核心 resolver 解析。
- 新增 channel/oneshot Terminal 审批适配器，模型后台运行时在 TUI 内展示工具、风险、原因与参数，允许一次或默认拒绝，继续复用 `EnterpriseApprovalHandler` 和统一权限引擎。
- 新增 UTF-8 输入编辑、选择候选后继续输入、已发送原文展示、最近 Agent/错误消息复制、大/小终端 resize 和审批 modal 断言测试；退出采用 RAII 恢复 raw mode、光标和 alternate screen。

### P029: Extensible Global Configuration and Unified Interaction

- 新增独立 `core-agent-config`：核心消费版本化强类型配置，`ConfigProvider`/`SecretResolver` 为稳定扩展接口；内置默认、用户 YAML/JSON、项目覆盖、环境变量与环境密钥引用只是可替换策略，优先级固定且可验证。
- 默认发现 `~/core-agent/core-agent-config.yaml|yml|json`，模型与 API Key 配置一次即可用于任意项目；Terminal 不再要求 `agent init`，项目初始化只保存入口/工作区覆盖。配置冲突、超大、符号链接和错误密钥引用 fail-closed，所有输出与 Debug 脱敏。
- 新增核心统一交互层：可注册 `/` 命令定义、解析、路由和 Agent Prompt 展开由 Terminal/Desktop 共享；`/help`、`/new`、`/clear`、`/sessions`、`/status`、`/tools`、`/config` 等零模型命令与 `/plan`、`/review`、`/test` 等 Agent 命令统一打通。
- 新增共享 `@file`/`@folder` Context resolver：文件夹确定性展开，复用工作区越界/敏感路径策略，拒绝符号链接并限制 mention、文件数、目录深度、单文件和总字节；正文仅进入本轮 Context，Session 保留原始输入，事件只记录路径、大小与 SHA-256。
- 新 chat 默认新 session，同一 chat 持续复用；Desktop 按规范化项目路径哈希隔离 Runtime 数据，读取同一全局配置并显示脱敏来源。新增配置策略合并、密钥脱敏、双入口命令/mention、项目隔离和真实 DeepSeek/Terminal 启动端到端验证。

### Unified Embedded Runtime Entry

- 新增根组合入口 `EnterpriseAgent`，在单进程内统一构造并持有全部 Runtime；Session、Context、Model、Workspace 使用持久化存储，Kernel/Platform/Protocol 和其余领域模块由组合根连接。
- 打通 Session → Context → Model → Tool 主链：同一请求贯穿持久化消息、Context 快照、真实模型 Provider、Tool 调用、Runtime 事件和终态。
- Terminal 默认使用 `embedded` 模式直接调用组合根；保留显式 `remote` 兼容模式，不再要求本地用户启动多个服务或子 Agent。
- Tauri Desktop 在应用进程中直接持有同一个 `EnterpriseAgent`；Console、Studio、Collaboration、Enterprise、Ecosystem 统一通过本地 Runtime bridge 访问内部模块。
- 新增统一入口端到端断言测试、桌面 Tauri 启动脚本和面向用户的 Terminal/Desktop 快速体验文档。
- 修正 Extension→Tool 必须使用版本化完整 key、协作通知仅投递事件发生时 audience、Multi-Agent handover 测试不再假设随机 Member ID 顺序，以及 Tool 拒绝/失败必须生成 Agent 终态失败事件；前端测试依赖审计为 0 漏洞。
- Model Provider 新增 OpenAI-compatible 工具声明、关联 tool call/result 和最多 8 轮的有界回填循环；模型现在能够真实发现、读取、编辑已打开工作区并执行受控命令。
- 新增工作区 `list_files`、`read_file`、`write_file`、`run_command`：限制路径、敏感目录、符号链接、正文体积、命令时长/输出，并以 SHA-256 乐观并发保护覆盖写入；命令子进程移除常见模型密钥环境变量。
- 新增 `strict`、`risk-based`（默认）、`auto` 三种权限模式和一次性批准账本；Terminal 提供交互审批且非交互默认拒绝，Desktop 提供五分钟超时自动拒绝的原生审批对话框。
- 新增权限分类、路径/符号链接逃逸、并发覆盖、人工批准编辑、自动批准编辑和真实 DeepSeek 读取未知文件的端到端测试；模型配置 Debug 强制脱敏，真实凭据仅从进程环境读取，未写入仓库。

### Phase 23: AgentOS Internal Protocol 0.1

- 新增 `core-agent-protocol`，提供版本化 Resource/Document，以及 Runtime/Capability/Agent/Workflow/Memory/Event/Trace/UI/Marketplace/SDK/Command 十一类 typed spec。
- 新增进程内 Discovery Registry：精确 kind/key/version 引用、dependency-first 注册、同版本内容 hash 不变、幂等重放、kind/capability discover 与 schema 查询。
- 新增 Compatibility Test Kit，校验 Internal Contract 版本、标识符、schema/endpoint 安全边界、Workflow/UI 结构、文档大小和引用完整性。
- 根组合层新增真实 Kernel Runtime、Visual Descriptor、Marketplace Package → Protocol 投影，并在统一 Registry 中完成跨模块 discovery。
- 明确当前为实践驱动的 Internal 0.1，不宣称 Public Specification v1.0；公开协议需多语言 SDK、第三方互操作与行为 CTK。
- 新增 round-trip、版本漂移、安全拒绝、缺失引用、Workflow 与跨 Runtime Protocol 测试；进入全项目统一验证阶段。

### Phase 22: AgentOS Ecosystem

- 新增 `core-agent-ecosystem`，实现 Publisher、Agent/Capability/Template/SDK Package、Publication Review、Rating 和精确版本依赖安装计划。
- 生态操作接入 P13 default-deny Policy/Audit；Package Owner 不得自审，只有通过独立 Review 的 Listed 版本可被解析安装。
- 新增 SHA-256/checksum、外部 signing key id、缺失/自依赖/环拒绝和确定性依赖拓扑；Catalog 不保存私钥或绕过 P12 Extension 安全边界。
- 根组合层新增 Marketplace required capability → P12 Extension inventory 缺口适配；P15 最终产品阶段校正为 `AgentEcosystem` 并兼容旧序列化名。
- Desktop 新增 Marketplace/My Agents/Capabilities/Templates/Developer/Publishing/Community/Cloud Workspace 与真实 Install/Submit API 动作。
- 新增发布/审核/安装/评分、默认拒绝、跨 Runtime inventory 与 Vue Controller 测试；统一验证待最终协议 P 完成后执行。

### Phase 21: Enterprise AgentOS Governance

- 新增 `core-agent-governance`，在 P13 Platform 之上实现外部 Identity Binding、统一 AI Asset Registry、风险/数据分类、独立审批证据和受控 Production/Suspend/Retire 生命周期。
- 所有企业写操作先通过 Platform default-deny Policy 并进入 Audit；资产 Owner 禁止自审，审批主体必须已绑定且 Active。
- 新增 `event_key` 幂等、`u64` micros/Token 的精确 Cost Ledger 与按货币整数汇总，不引入浮点金额或 Billing 结算声明。
- Desktop 新增 Enterprise Dashboard/Organization/Identity/Assets/Governance/Policies/Cost/Audit/Operation/Settings，以及真实 Approve/Promote/Suspend API 动作。
- 新增资产完整治理、自审拒绝、成本幂等、Platform 默认拒绝审计与 Vue Controller 测试；统一验证待剩余 P 完成后执行。

### Phase 20: Collaborative Agent Platform

- 新增 `core-agent-collaboration`，实现团队 Project/membership、共享 Agent/Workflow 引用、Task 状态/进度、Review/Approval、Knowledge 与不可变 Activity。
- Review 决策与 Task/Activity 原子变更；Reviewer role 强制、自我审批拒绝、状态迁移/重复 review/Activity 幂等 fail-closed。
- 根组合层新增 P11 Multi-Agent Outcome → Project Activity Stream 投影，Notification 按项目 audience 过滤。
- Desktop 新增 Collaboration Home/Projects/Agents/Team/Tasks/Reviews/Approvals/Knowledge/Activity/Notifications Workspace 与真实审批 API 动作。
- 新增协作完整流程、自我审批/Reject、Activity 幂等、跨 Runtime Outcome 和 Vue Controller 测试；统一验证待剩余 P 完成后执行。

### Phase 19: Agent Studio and Visual Runtime

- 新增 `core-agent-visual` 声明式 Visual Descriptor/Panel/Field/Action 协议、revision CAS Registry 与确定性 Studio Panel Catalog。
- Visual endpoint 限制为安全相对 `/api/` 路径，拒绝任意前端代码/远程组件；危险与 DELETE action 强制审批。
- 根组合层新增 Platform Health/Audit Visual Descriptor，打通 Runtime → Visual Registry → 自动 Studio Panel。
- Desktop 新增 Home/Agent/Workflow/Prompt/Memory/Capability/Knowledge/Trace/Model Studio；Agent Designer 真实创建版本化 API 资产。
- 新增 Visual Registry/安全边界/跨 Runtime Catalog 与 Studio Controller/创建 Agent/导航测试；统一验证待全部剩余 P 完成后执行。

### Phase 18: Desktop Workspace

- 新增 Tauri2 + Vue3 `agent-desktop`，提供 Console/Project/Changes/Trace/Tools/Memory/Sessions/Settings 八 Workspace 与默认 Runtime 可视化工作台。
- 新增集中式 Desktop REST/SSE Controller、2 MiB 响应边界、Chat/Trace 实时更新、面板级空态和全局离线恢复，不填充伪业务数据。
- 新增黑金 Apple 层级、pill/三级按钮、响应式 Workspace/Panel 组件、可访问 Sidebar 与移动端收敛布局。
- Rust Bridge 新增仅限 UI 的 SQLite Preference Store，包含审计字段、索引、无外键、CAS、敏感值拒绝、重开与篡改检测。
- 新增 Rust Store E2E、Vue Controller/八工作区/可访问性测试；Cargo/Vitest/typecheck/Vite build 统一验证待全部剩余 P 完成后执行。

### Phase 17: Professional CLI

- 在 `agent-cli` 新增有界 Project/Git marker 采集、Project Index、Profile、统一 slash Command Registry、补全/帮助与隐私收敛命令历史。
- 新增 `project/profile/tasks/history/review/plan/explain/test/fix/refactor/commit/pr/tools/memory` top-level 与 chat slash 命令，共享同一解析和执行入口。
- 新增 `ProfessionalAgentClient` 及 Project/Review/History/Memory/Task/Tool/Command HTTP 合同；智能分析保持服务端所有权，CLI 不伪造结果。
- 新增 Project 识别、命令注册/引号解析、Profile → Index → Review → History E2E 与隐私边界测试。
- 统一验证待全部剩余 P 完成后执行。

### Phase 16: Terminal CLI MVP

- 新增官方 `agent-cli` library/`agent` binary，支持 `init/chat/run/status/sessions/config/resume/cancel`，CLI 保持 Runtime-thin。
- 新增可替换 `AgentClient`、真实 REST + 分块 SSE Client、`Renderer`/金色 Terminal Renderer 与可测试 `CliApplication`。
- `agent init` 生成最小 `.agent` 配置/上下文/Memory 目录且拒绝覆盖；session ID 有界、原子保存，terminal event 前断流不落成功状态。
- 新增命令解析、UTF-8 跨 chunk SSE、run→resume、失败恢复与真实 binary init E2E；服务端不存在的边界显式记录。
- 统一验证待全部剩余 P 完成后执行。

### Phase 15: Visual Product Roadmap Contract

- 新增 `core-agent-app` 共享应用层合同，强类型表达 Terminal MVP → Professional CLI → Desktop → Studio → Team → Enterprise → Agent OS 七阶段。
- 定义 CLI/Desktop/Web/IDE 产品表面与各阶段必需能力；新增确定性 readiness evaluator，报告未完成前置阶段和缺失能力。
- 根 crate 统一导出路线图合同，供后续视觉 P 复用；本 P 不提前实现具体 UI。
- 新增路线图顺序、缺口报告与完成态单元断言；统一验证待全部剩余 P 完成后执行。

### Phase 14: Runtime Kernel

- 新增独立 `core-agent-kernel`，提供 Runtime Registry、依赖 DAG、同 major 最低版本校验、统一 init/start/stop/reload、Health、Hook 与 Kernel Event 契约。
- 生命周期按依赖拓扑确定性启动、反向停止；启动失败会反向恢复本次已启动 Runtime，缺失依赖、循环和版本不兼容均在副作用前拒绝。
- 新增带单调 revision、敏感内容拒绝和体积/深度上限的 Configuration，以及类型安全、重复 key 拒绝的 Service Registry。
- 根组合层新增 `PlatformKernelRuntime`，真实打通 Kernel → Platform 生命周期、配置 reload 与健康检查。
- 新增 P14 单元、Runtime E2E 与 Kernel → Platform 跨 Runtime E2E；统一验证待全部剩余 P 完成后执行。

### Phase 13: Platform Runtime

- 新增独立 `core-agent-platform`，实现 Tenant/Organization 隔离、确定性默认拒绝 Policy、原子幂等 Quota、不可变 Audit、Health/Metrics 扩展契约与 Runtime 生命周期。
- 新增 `tenant`、`organization`、`policy`、`audit`、`quota` 五张 SQLite 表；全部具备审计字段、注释、索引、无外键和结构化列/JSON 冷读篡改检测。
- 配额按 Tenant + 可选 Organization + Key 精确寻址，通过 CAS、有界请求账本及 Audit 单事务提交避免跨范围串用和重复扣量。
- 根组合层新增 `PlatformToolPolicy`/`ToolGovernanceResolver`，把企业策略和配额 fail-closed 接入真实 Tool Runtime。
- 新增 P13 单元、Runtime E2E 与 Platform → Tool 跨 Runtime E2E；按批量实现约定，统一验证待所有剩余 P 完成后执行。

### Phase 12: Extension Runtime

- 新增独立 `core-agent-extension`，统一 Manifest、Capability、Provider、Extension 生命周期与 Host 隔离边界，实现本地 install/load/enable/execute/disable/offline-upgrade/uninstall。
- Manifest 使用不可变 revision；Capability 成为上层稳定依赖，Provider 按 priority/key/id 确定性解析，Extension 不依赖 Agent、Workflow 或 Planning。
- 默认 Local Loader 仅接受安全 `file:` URI并真实校验 artifact SHA-256；默认 Policy fail-closed 拒绝 Network/File/Process/Environment 权限，不把同进程 Host 宣称为安全沙箱。
- invocation 在 Host 执行前持久化 request/capability/provider/内容 hash；OutcomeUnknown 保留 Running，完全相同请求可冷恢复，生命周期与调用使用不覆盖 live guard 消除竞态。
- 新增 `extension`、`extension_manifest`、`extension_state`、`capability`、`provider` 五张 SQLite 表，全部具备审计字段、注释、索引且无外键，并严格交叉校验声明归属。
- 根组合层新增 ToolExtensionHost/ExtensionToolResolver，打通 Extension Capability → Tool Runtime；单元、Runtime E2E、SQLite 篡改与跨 Runtime E2E 已加入，统一验证待全部剩余 P 实现后执行。

### Phase 11: Multi-Agent Runtime

- 新增独立 `core-agent-multi`，实现 `Organization → Team → Role → Agent Member → Collaboration`，支持版本化组织/角色、Team 生命周期、成员加入/离开和严格归属校验。
- 新增 AgentDirectory、AgentRouter、AgentDispatcher、Policy、Lifecycle、Interceptor、Observer 与 Store 扩展契约；默认 Router 按角色、能力、Workspace、live 可用性和稳定 member ID 确定性选择。
- 新增 typed Agent Message 与两阶段分派协议；稳定 dispatch ID、binding 先持久化、Waiting/OutcomeUnknown 冷恢复复用、显式 handover 和有界通信 transcript 提供可审计协作。
- 根组合层新增 RuntimeAgentDirectory/RuntimeAgentDispatcher/AgentAssignmentResolver，真实打通 Team → Agent → Planning → Execution → Tool，同时保持 Multi-Agent crate 无下层 Runtime 依赖。
- 新增 `organization`、`team`、`agent_member`、`role`、`collaboration` 五张 SQLite 表，全部具备审计字段、注释、索引且无外键；Team/Collaboration/Member 使用原子 CAS 提交并严格冷读取交叉校验。
- 已加入稳定 dispatch、安全边界、确定性路由、resume、handover、未知结果、Observer 隔离、SQLite 篡改与跨 Runtime E2E；统一验证将在剩余 P 全部实现后执行。

### Phase 10: Workflow Runtime

- 新增独立 `core-agent-workflow`，实现 `Workflow → Stage → Activity → Action` 四层业务模型、不可变 Definition 版本和 Instance 固定快照；P10.0 仅提供确定性顺序调度，不越界实现 DAG、并行、条件、触发器、审批、补偿、DSL 或 UI。
- 新增 `WorkflowManager`、Scheduler、Engine、Policy、Lifecycle、Interceptor、Observer、Registry、Store、Snapshot 与 Variable 扩展契约；支持 Created/Scheduled 冷恢复、Waiting/Paused/Running 恢复、在线暂停/取消和稳定 dispatch/binding 复用。
- 根组合 crate 新增 `ExecutionWorkflowEngine` 与 `WorkflowPlanResolver`，真实打通 Workflow → Planning Plan → Execution；Workflow 不直接执行 Tool，执行结果未知时保留 Running 并禁止盲目重放。
- 新增 `workflow`、`workflow_definition`、`workflow_instance`、`workflow_snapshot`、`workflow_state` 五张 SQLite 表，全部具备审计字段、注释、索引且无外键；事务 CAS 强制 Definition/Snapshot 所有权、聚合进度与 lifecycle timeline 一致性。
- 三轮 review 修复超时取消误判、并发 resume 控制令牌覆盖、Created/Scheduled 崩溃卡死、状态变化缺少 timeline、内存/SQLite Definition 校验分歧和层级进度伪造。
- P10 共 4 个单元断言、15 个 Runtime E2E、1 个跨 Runtime E2E 通过；严格 Clippy、格式/diff 检查和全工作区回归通过。

### Phase 0: Session Runtime 增强

- 补齐 `READY → RUNNING → PAUSED → RUNNING/ARCHIVED → DELETED` 公开生命周期入口，并发布真实 old/new 状态事件。
- 新增 `SessionLifecycle`、`SessionSerializer`、`JsonSessionSerializer`、`SessionObserver` 扩展点。
- Session、Manifest、默认 MAIN Conversation 通过 SQLite 事务原子创建；Manifest 统计随 Conversation 和 Message 变更同步。
- SQLite 五张表补齐 `create_time`、`update_time`、`create_user`、`update_user`，启动时兼容迁移 0.1.0 数据库。
- 持久化遇到损坏的 UUID、时间、枚举或 JSON 时明确报错，不再静默回退或丢行。
- 增加生命周期、迁移、事务回滚、持久化恢复和 Session Runtime 端到端测试；P0 共 36 个单元断言与 4 个端到端用例通过。

### Phase 1: Context Runtime 增强

- `max_messages` 读取最新消息并保持时间顺序，`max_tokens` 与 Slot 预算进入每次 Pipeline 执行；必须保留的内容超预算时明确报错。
- Composer 完整保留 System、Environment、Workspace、Memory、Conversation、Tool、Plugin、User 八类 Slot，并提供可直接交给后续 Runtime 的完整 Context API。
- Context 哈希改为基于完整语义内容且排除构建 ID/时间，Pipeline 记录真实构建耗时并支持 Slot 启停与观察器。
- 补齐 `ContextSerializer`、`JsonContextSerializer`、`ContextCache`、`ContextObserver` 扩展契约。
- `context_snapshot` 增加审计字段兼容迁移、严格行解析、内容/列哈希一致性校验及完整快照恢复。
- 增加预算、最新消息、Slot 保真、稳定哈希、迁移、损坏数据、扩展点及 Context Runtime 端到端测试；P1 共 52 个单元断言与 4 个端到端用例通过。

### Phase 2: Model Runtime

- 新增独立 `core-agent-model`，统一 Generate、Stream、Embedding、Vision 请求/响应；Tool Call 仅返回、不执行。
- 新增 Model Profile、Catalog、Capability Registry 与确定性 Router，支持手动、自动、最低成本、最低延迟和受约束 fallback。
- 中央 Engine 统一总超时、有限重试、限流与 fallback；仅首输出前允许流式 fallback，严格拒绝截断 SSE。
- 新增真实 OpenAI-compatible HTTP/SSE Provider，覆盖文本、多模态、Embedding、Usage 与 Tool Call wire format。
- 新增 Interceptor、Usage Collector、Retry Policy、Rate Limiter、Observer 扩展点；Observer panic 隔离，审计失败不隐藏已成功且已计费的推理。
- 新增 `model_provider`、`model`、`model_usage` 三张 SQLite 表，补齐审计字段、注释、索引、兼容迁移与严格解析；API Key 不持久化，Usage metadata 使用 allowlist。
- 增加路由、能力、重试/fallback、流式超时、真实 HTTP/SSE、审计归属、迁移与安全边界测试；P2 共 30 个单元断言与 11 个端到端用例通过，全工作区回归通过。

### Phase 3: Tool Runtime

- 新增独立 `core-agent-tool`，统一 Tool identity/schema/capability/request/result/permission/lifecycle，不依赖 Session、Context 或 Model。
- 新增 ToolManager、live Registry、durable Catalog、Provider、Executor、Validator、Result Mapper、Lifecycle、Interceptor、Observer、Policy 扩展点。
- JSON Schema 参数校验禁用 HTTP/file 外部引用并使用线性正则引擎；Schema、参数、Catalog 和 metadata 均有大小/敏感键边界。
- 默认权限为 Ask，Ask/Deny 不执行；SQLite 规则支持 tool/capability/subject/priority，等价冲突按 Deny → Ask → Allow 收敛。
- 新增总超时、current-process cancel、单一终态、Observer panic 隔离和 content-free Execution audit；重复 request ID 不重放、不覆盖旧审计。
- 新增 FunctionTool 与 StaticToolProvider，提供安全 Builtin 接入但不越界实现 P4 Filesystem/Terminal/Git。
- 新增 `tool_provider`、`tool`、`tool_execution`、`tool_permission` 四张 SQLite 表，补齐审计字段、注释、索引、迁移与严格解析，无外键且不保存参数/输出正文。
- 增加 capability、schema、权限、生命周期、超时/取消、Provider、审计、幂等、迁移与恢复测试；P3 共 18 个单元断言与 10 个端到端用例通过，全工作区回归通过。

### Phase 4: Workspace Runtime

- 新增独立 `core-agent-workspace`，将 Workspace 建模为 `identity + provider + URI + projects + environment + resources + graph + lifecycle`，不把它退化为目录路径。
- 新增 WorkspaceManager、Registry、Catalog、Provider、Resource/Project/Environment Manager、Lifecycle、Indexer、Snapshot、Policy、Interceptor、Observer 扩展点；Runtime 不依赖 Session、Context、Model 或 Tool。
- Local Provider 使用 canonical `file:` URI，受限扫描不跟随符号链接，忽略常见构建目录；资源数量与深度上限均明确失败，不生成静默残缺索引。
- 自动发现 Cargo、Maven、Gradle、Node、Python 与 Generic 项目，并通过文件扩展名推断语言、Runtime、包管理器和 Git 仓库；环境变量只保存少量名称，绝不读取值。
- 新增基础 Workspace Graph 与确定性搜索，统一 Workspace、Project、Environment、Resource 节点及关系，为后续 Module/Symbol/Git Index 预留稳定合同。
- 新增非破坏性 overlay Snapshot/Restore：恢复快照文件但保留快照后新增文件；拒绝越界/符号链接目标，非法状态在复制前失败，Catalog 提交失败时补偿清理快照文件和元数据。
- 新增 `workspace`、`project`、`resource`、`environment`、`workspace_snapshot` 五张 SQLite 表，全部包含审计字段、注释和索引且无外键；恢复时严格交叉校验结构列、JSON aggregate、Graph 与子实体。
- 根组合 crate 新增 Workspace/Environment → Context adapter，以有界结构化数据填充 P1 占位合同，Workspace crate 保持依赖方向独立。
- 增加生命周期、URI 凭据、Provider/Policy/Interceptor、项目/环境发现、资源上限、Graph 搜索、Snapshot 补偿、SQLite 冷恢复/损坏数据及 Context 集成测试；P4 共 14 个单元断言、13 个 Runtime E2E 与 1 个跨 Runtime E2E 通过，全工作区回归通过。

### Phase 5: Planning Runtime

- 新增独立 `core-agent-plan`，实现 `Intent → Goal → Plan → Task → Step → Action`；Planning 只生成、审查和管理计划，不调用 Model、Tool 或 Scheduler。
- 新增 PlanningManager、Goal/Task/Step Manager、Strategy、Builder、Reviewer、Lifecycle、Policy、Interceptor、Observer、Catalog 与 Snapshot 扩展合同；默认 Rule Builder 可确定性生成 Coding/RCA/Report/General 计划。
- 统一执行前/执行后 Review 生命周期：P5 生成路径为 `Created → Planning → Reviewing → Ready`，并为 P6 预留 Executing/Completed 合法状态合同；未批准计划绝不进入 Ready。
- Planning Graph 严格校验完整层级、依赖引用、精确边集合与无环；Task/Step 使用稳定 key 和 Plan 命名空间 UUID v5，P5 不抢跑 DAG 调度或并行执行。
- Action/Metadata/Context 增加体积、嵌套深度、敏感键与凭据 URI 边界；Tool Action 必须来自当前 PlanningContext 的真实 tool/capability，生成后与恢复后均重新经过 Policy。
- Goal 与 PlanningContext 严格校验 Session/Workspace 身份；根组合 crate 仅接入可用 Workspace 和启用 Tool，不持久化文件正文、Tool Schema 或环境变量值。
- 新增 `goal`、`plan`、`task`、`step`、`plan_snapshot` 五张 SQLite 表，全部包含审计字段、注释和索引且无外键；结构列/JSON/Intent/子实体冷恢复严格交叉校验。
- 内存与 SQLite Catalog 使用提交时 CAS 防止并发丢更新；Plan 变更原子保存旧版本快照，Snapshot 不可覆盖，取消/恢复和手工 restore 均保持单调版本。
- 增加生命周期、Graph、安全边界、Builder/Reviewer/Policy/Interceptor、并发 CAS、Snapshot、SQLite 损坏恢复及 Workspace/Tool 集成测试；P5 共 11 个单元断言、10 个 Runtime E2E 与 1 个跨 Runtime E2E 通过，全工作区回归通过。

### Phase 6: Execution Runtime

- 新增独立 `core-agent-execution`，以不可变的已批准 Plan 为执行定义，实现 `Plan → Action → Command → Executor`；Execution 不生成或改写 Planning 状态。
- 新增确定性顺序依赖调度、Execution/Action 状态机、Lifecycle、Policy、Interceptor、Observer 与协作式控制；支持安全边界暂停/恢复、在线取消和冷恢复，结果未知的在途副作用命令绝不自动重放。
- 新增集中式有限重试、线性/指数策略扩展、SHA-256 完整性 Checkpoint capture/restore、反向显式补偿；Checkpoint 仅允许恢复最新安全边界，Rollback 不伪装成通用事务。
- 新增 `execution`、`checkpoint`、`execution_state`、`retry`、`rollback` 五张 SQLite 表，全部包含审计字段、注释、索引且无外键；聚合与状态/检查点/重试/回滚原子 CAS 提交，五表冷恢复均严格交叉校验结构列和 JSON。
- 根组合 crate 新增 `ToolActionExecutor`，把 Tool 作为 Command 实现接入 P3；执行前重新校验 live capability，传递已批准 capability/target，桥接 ToolManager cancel，并仅持久化有界结果摘要。
- 三轮 review 修复 live policy 绕过、取消操作者审计、任务中止 live registry 泄漏、成功副作用被 after hook 误判、retry-cancel 状态不一致、rollback observation 关联错误和子表篡改漏检。
- 增加状态机/命令身份/重试断言，以及顺序执行、重试、补偿、暂停/Checkpoint 恢复、取消、策略拒绝、崩溃未知结果、SQLite 篡改和 P5→P6→P3 Tool 集成测试；P6 共 4 个单元断言、11 个 Runtime E2E、2 个跨 Runtime E2E 通过。

### Phase 7: Agent Runtime

- 新增独立 `core-agent-agent`，实现 Agent/Profile/Capability/Policy/Lifecycle/Coordinator/Observer/Interceptor/Factory/Snapshot/Registry 扩展合同；单 Agent 可连续接受多个 Goal，且不越界实现 Model、Tool 或 Context。
- 真实打通 `Agent -> Planning -> Execution -> Tool`：Coordinator 先持久化 Goal/Plan/READY Execution，再启动副作用；部分失败保留全部已知 lower-runtime ID，禁止跨 Runtime 假回滚。
- 实现 `Created -> Ready -> Running -> Waiting/Paused/Failed -> Completed/Destroyed`，支持 actor-aware create/start/run/stop/finish/destroy、并发独占、冷 reconcile 与 outcome-unknown 防重放。
- P6 增加兼容的 `prepare/start` 与共享 `ExecutionControl` start/resume；Prepare 和副作用时 Start 分别授权，Planning/启动前/执行中/resume 窗口的 stop 均不会丢失。
- 新增 Profile/Policy 不可变快照、toolset fail-closed 上界、敏感配置拒绝、Ask 默认拒绝，以及安全边界 Snapshot/current-version restore。
- 新增 `agent`、`agent_profile`、`agent_snapshot`、`agent_state`、`agent_policy` 五张 SQLite 表，全部含审计字段、注释、索引且无外键；CAS、owner/唯一性、版本不变量和结构列/JSON 冷读取严格校验。
- 三轮 review 修复 live ownership/stop-resume TOCTOU、操作 actor 丢失、Start/Resume 策略绕过、多 Goal 旧引用污染、部分引用失联、失败后残留 RUNNING、UTF-8 错误截断、冷恢复卡死、snapshot store 分叉等问题；P7 25 项 Runtime E2E、P6 16 项 Runtime E2E、1 项 Agent 跨 Runtime E2E 与全工作区回归通过。

### Phase 8: Memory Runtime

- 新增独立 `core-agent-memory`，实现 Memory Event/Kind/Type/Importance/Tag/Policy、事件幂等、命名空间隔离、结构化分类/过滤/排序及可解释 Recall；明确不引入 Embedding、Vector 与 AI 总结。
- 实现 `Created -> Verified -> Indexed -> Recalled -> Updated -> Archived -> Forgotten`、CAS 更新、过期排除、Snapshot/current-version Restore；Forget 以单事务写入无内容墓碑并清除索引、标签和快照。
- 提供 Store/Classifier/Indexer/Retriever/Lifecycle/Policy/Interceptor/Observer 注入契约；拦截器和 Lifecycle 越权修改被拒绝，Observer panic 隔离，自定义 Indexer 可严格持久化恢复。
- 新增 `memory`、`memory_index`、`memory_snapshot`、`memory_policy`、`memory_tag` 五张 SQLite 表，全部含审计字段、注释、索引且无外键；聚合、索引、快照和策略冷读严格交叉校验结构列与序列化内容。
- 根组合 crate 新增 `MemoryContextProvider`，将有界 Recall 写入现有 Context Memory Slot；P8 共 3 个单元断言、10 个 Runtime E2E、1 个跨 Runtime E2E 及全工作区回归通过，严格 Clippy 无 warning。

### Phase 9: Event Runtime

- 新增独立 `core-agent-event`，提供 typed Event、Registry、Subscription、Router、Dispatcher、Policy、Lifecycle、Interceptor、Observer、Replay 与 Dead Letter 合同；Runtime 保持业务无关。
- 实现 namespace 隔离、确定性优先级 fan-out、event ID 内容幂等、有限重试及 at-least-once 投递；发布与 Replay 均持久化 Pending 计划，并可使用稳定 delivery ID/attempt 从未知结果中续投。
- Event/Replay 状态与对应 Dead Letter 原子提交；显式 Replay 保持原 Archived Event 不变，策略、actor、reason、attempt 与 payload hash 全程可审计。
- 新增 `event`、`event_subscription`、`event_replay`、`event_policy`、`event_dead_letter` 五张 SQLite 表，全部含审计字段、注释、索引且无外键，并严格交叉校验结构列、JSON 与归属关系。
- 根组合 crate 新增 typed Event → Memory handler；P9 共 3 个单元断言、13 个 Runtime E2E、1 个跨 Runtime E2E 通过，严格 Clippy、格式/diff 检查及全工作区回归通过。

## [0.2.0] - 2026-07-17

### Phase 1: Context Runtime

Context Runtime — Agent 上下文生命周期管理器。负责构建 Agent 每一次推理所需要的完整上下文。

**不做 LLM 调用，只做上下文组装。** Context ≠ Prompt。Context 是结构化的上下文数据，由 Provider 收集、Reducer 裁剪、Composer 组装后交给后续的 Model Runtime。

#### 架构

```
core-agent (workspace root)
├── core-agent-session (Session Runtime)
└── core-agent-context  (Context Runtime) ← 新增
    ├── api/          — 公开 API (ContextRuntime)
    ├── application/  — 用例编排 + ContextPipeline + SummaryReducer + DefaultComposer
    ├── domain/       — Context + ContextSegment + ContextSlot + 7 个子 Context
    ├── infrastructure/ — 4 个扩展点 trait (ContextProvider / ContextReducer / ContextComposer / ContextSnapshotStore)
    ├── persistence/  — SQLite 实现 + 4 个内置 Provider
    ├── dto/          — 输入输出 DTO
    └── error/        — 统一错误类型
```

#### 核心组件

| 组件 | 描述 |
|------|------|
| ContextBuilder | 流程编排（Pipeline Builder 模式），Collect → Reduce → Compose → Snapshot |
| ContextProvider | 4 个内置 Provider：System / Conversation / Environment / User |
| ContextReducer | SummaryReducer：摘要 + 保留最近 N 条（默认 20），超出预算时生成摘要 |
| ContextComposer | DefaultComposer：将 segments 分配到 8 个 Slot，组装完整 Context |
| ContextSnapshot | 每次 build() 后保存完整 Context JSON 到 SQLite |
| ContextPipeline | 不可变管道，链式执行各阶段，支持自定义扩展 |

#### ContextSlot 机制

8 个槽位，每个独立：Token 估算 / 优先级排序 / 启用禁用 / 预算控制。

```
System(100) > Environment(90) > Workspace(80) > Memory(70)
> Conversation(60) > Tool(50) > Plugin(40) > User(30)
```

#### Context 对象

7 个独立子结构：System / Conversation / Workspace / Memory / Environment / Plugin / User，含 TokenDistribution 和 SHA-256 哈希。

#### 持久化

- `context_snapshot` 表：id/session_id/conversation_id/created_at/content/token_count/hash/build_duration_ms
- 3 个索引：session_id / created_at DESC / hash

#### 与 Session Runtime 集成

- 依赖 `core-agent-session`（只读），通过 `Arc<dyn SessionStore>` 读取消息历史
- `ContextRuntime<S: SessionStore>` 接收 Session Store 作为依赖

#### 测试

- 33 个单元测试全部通过
- 覆盖 domain / application / dto / persistence / api 层
- 集成测试：Session → Messages → build_context → 验证裁剪

---

## [0.1.0] - 2026-07-17

### Phase 0: Session Runtime MVP

Session Runtime — Agent 生命周期管理器。负责 Agent 从出生到结束的整个生命周期。

**不做 AI，只做基础设施。** 后续所有 Runtime（Context / Model / Tool / Workspace / Planning / Execution / Memory / Permission / Plugin / Observation / Multi-Agent）全部依赖此层。

#### 架构

```
core-agent (workspace root)
└── core-agent-session (Session Runtime)
    ├── api/          — 公开 API (SessionRuntime)
    ├── application/  — 用例编排 (SessionApplicationService)
    ├── domain/       — 5+1 核心实体
    ├── infrastructure/ — 扩展点 trait (SessionStore)
    ├── persistence/  — SQLite 实现 (5 张表)
    ├── dto/          — 输入输出 DTO
    ├── event/        — EventBus (tokio::broadcast)
    └── error/        — 统一错误类型
```

#### 核心实体

| 实体 | 描述 |
|------|------|
| Session | Agent 生命周期载体，状态机：CREATED → READY → RUNNING → PAUSED → ARCHIVED → DELETED |
| Conversation | 属于 Session，类型：MAIN / PLAN / REVIEW / SYSTEM / DEBUG（MVP 只用 MAIN） |
| Message | 消息实体，状态：PENDING / STREAMING / DONE / FAILED |
| Attachment | 附件统一模型（图片/文件/日志/Diff/Terminal/PDF） |
| Manifest | Session 概要快照（名称/模型/workspace/标签/统计），左侧列表用 |
| Metadata | JSON 扩展容器，避免不断加字段 |

#### EventBus

基于 `tokio::sync::broadcast`，事件类型：
- `SessionCreated` / `SessionUpdated` / `SessionStateChanged` / `SessionDeleted`
- `ConversationCreated`
- `MessageAdded` / `MessageUpdated` / `MessageDeleted`
- `ManifestUpdated`

#### 持久化

- SQLite（rusqlite + r2d2 连接池）
- 5 张表：`session` / `conversation` / `message` / `attachment` / `manifest`
- 全部软删除，禁止外键

#### 测试

- 27 个单元测试全部通过
- 覆盖 domain / dto / event / persistence 层

#### 依赖

- Rust 1.94.0
- tokio (async runtime)
- rusqlite 0.32 (bundled SQLite)
- serde / serde_json
- uuid v4
- chrono
- async-trait
- thiserror 2
