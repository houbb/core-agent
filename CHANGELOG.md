# CHANGELOG

## [0.42.0] - 2026-07-21

### P042 Extension Ecosystem — Agent 能力扩展层

实现 `design-docs/042-core-ablity-p3-extensition.md` 定义的 P3 Extension Ecosystem Runtime，让 Agent 具备标准化扩展能力。

#### 新 crate：core-agent-mcp

- **McpClient** — stdio-based JSON-RPC 传输层，支持 MCP 协议 `2025-06-18`
- **McpToolProvider** — 实现 `ToolProvider` trait，将 MCP Server 的远程工具包装为本地 `Tool`
- **McpServerConfig** — 分层配置发现（global + project），支持 32 台服务器
- **安全控制** — 环境变量过滤（自动屏蔽敏感信息）、配置校验、超时/取消支持
- **分页发现** — 通过 `tools/list` 自动发现远程工具，支持游标分页

#### 新 crate：core-agent-plugin

- **PluginManifest** — YAML/JSON 格式的插件清单（name/version/author/tools/skills/agents）
- **PluginLifecycle** — 完整生命周期：Install → Enable → Disable → Uninstall
- **PluginManager** — 基于 `core-agent-extension` 实现，将插件注册为 Extension
- **插件隔离** — 状态转换验证 + 乐观并发控制

#### 新 crate：core-agent-skill

- **SkillCatalog** — 从文件系统发现 Skill（SKILL.md + YAML frontmatter）
- **SkillDescriptor** — 技能元数据（name/description/scope/tool_count）
- **SkillRoot** — 分层技能根目录，支持 precedence 覆盖（system → user → project）
- **懒加载** — 只加载元数据，完整内容按需加载（`load()`）
- **metadata_prompt** — 渐进式披露，按预算裁剪技能描述

#### 新 crate：core-agent-slash

- **SlashCommandRegistry** — 独立的 slash 命令注册表，支持 CLI/TUI/Desktop/Web/API
- **SlashCommand trait** — 统一的命令接口（metadata → validate → execute）
- **SlashCommandObserver** — 命令生命周期观察者（start/success/failure）
- **tokenize 解析器** — 支持引号转义、参数拆分
- **SlashCategory** — 14 种分类体系

### P043 Multi-Agent Runtime — Multi-Agent 协同系统

实现 `design-docs/043-core-ablity-p2-multi-agent.md` 定义的 P2 Multi-Agent Runtime，让 Agent 从 Single Agent 升级为 Agent System。

#### 新 crate：core-agent-subagent

- **AgentInstance** — 完整的 SubAgent 生命周期模型：Created → Initialized → Running → Waiting → Completed/Failed → Destroyed
- **AgentRole** — 6 种角色：Planner、Executor、Researcher、Reviewer、Monitor、DecisionMaker
- **InstanceType** — Manager / Worker 实例类型 + parent/supervisor 关系链
- **SubAgentManager** — 创建/启动/停止/销毁/查询/按条件过滤
- **SubAgentLifecycle** — 严格的状态转换机（不允许非法跳转）
- **SubAgentObserver/Interceptor** — 生命周期事件监听和拦截
- **SQLite 持久化** — agent_instance 表 + 3 个索引 + 乐观并发控制

#### 新 crate：core-agent-message

- **AgentMessage** — 结构化消息：Request/Response/Event/Broadcast 4 种类型
- **消息字段** — from/to_agent_id、correlation_id（对话链）、intent（结构化意图）、payload
- **MessagePriority** — 4 级优先级：Low/Normal/High/Critical
- **MessageStatus** — 完整的生命周期：Pending → Delivered → Read / Failed
- **MessageManager** — send/receive/broadcast/reply_to/mark_read/list_inbox
- **Mailbox 模式** — 每个 Agent 独立收件箱，按优先级和创建时间排序
- **DefaultMessageBus** — send 持久化 + receive 拉取 + broadcast 批量发送
- **SQLite 持久化** — agent_message 表 + 4 个索引

#### 新 crate：core-agent-orchestrator

- **Orchestration** — 编排任务模型（goal/strategy/status/workers/result）
- **4 种策略** — Sequential（串行）、Parallel（并行）、Supervisor（监管者模式）、Debate（辩论模式）
- **SupervisorStrategy** — MVP 策略：Supervisor 创建 Worker → 发送任务消息 → Worker 执行 → 聚合结果
- **DefaultResultAggregator** — 多结果合并 + 置信度计算
- **OrchestratorManager** — create/start/add_worker/get_result/supervise 完整 API
- **AgentInstanceRef/WorkerResult/AggregatedResult** — 编排结果数据模型
- **SQLite 持久化** — orchestration 表 + 2 个索引

#### 新增 Slash 命令（8 个）

| 命令 | 用法 | 功能 |
|------|------|------|
| `/subagent list` | `/subagent list` | 列出所有子 Agent 实例（名称/角色/状态/ID/父子关系） |
| `/subagent spawn` | `/subagent spawn <role> <task>` | 创建子 Agent 实例（role: planner/executor/researcher/reviewer/monitor/decisionmaker） |
| `/subagent status` | `/subagent status <id>` | 查看子 Agent 完整状态信息 |
| `/subagent destroy` | `/subagent destroy <id>` | 销毁指定子 Agent |
| `/orchestrate` | `/orchestrate <strategy> <goal>` | 启动多 Agent 编排任务（支持 sequential/parallel/supervisor/debate） |
| `/orchestrate status` | `/orchestrate status <id>` | 查看编排任务状态和结果 |
| `/message send` | `/message send <to> <text>` | 向 Agent 发送消息 |
| `/message inbox` | `/message inbox [agent_id]` | 查看消息收件箱 |

#### RCA Demo

`/orchestrate supervisor "订单服务 500"` 端到端 RCA 链路：
1. Supervisor Agent 自动创建 3 个 Researcher SubAgent（Log-Agent、Metric-Agent、Trace-Agent）
2. Supervisor 通过 MessageManager 向每个 Worker 发送 TASK_ASSIGNMENT 消息
3. Worker 并行执行分析任务
4. DefaultResultAggregator 聚合结果，输出 Root Cause 和置信度

#### 集成

- 3 个新 crate 注册到 Workspace members
- EnterpriseRuntimes 新增 `subagents`、`messages`、`orchestrator` 三个字段
- EnterpriseAgent 初始化时创建 3 个 SQLite 持久化 store（p2_subagent.db / p2_message.db / p2_orchestration.db）
- lib.rs 新增 re-export 块
- P2CommandPlugin 插件模式注册（复用 society_plugin.rs 的 enum-wrapper 模式）

#### 测试

- **core-agent-subagent**: 15 tests（7 unit + 8 e2e）
- **core-agent-message**: 13 tests（6 unit + 7 e2e）
- **core-agent-orchestrator**: 12 tests（3 unit + 9 e2e）
- **总计**: 40 tests ✅

## [0.40.2] - 2026-07-21

### P042 Intelligence Runtime — 从 Reactive 升级为 Proactive Agent

实现 `design-docs/042-core-ablity-p1-plan.md` 定义的 P1 Intelligence Runtime，让 Agent 从"响应式"升级为"规划式"。

#### 🔴 新 crate：core-agent-question (P1)

- **Question 模型** — 支持 CHOICE/CONFIRM/INPUT/APPROVAL/REVIEW 五种类型
- **QuestionManager** — 基于 oneshot channel 的异步 ask/answer 模式
- **QuestionOption** — 选项列表，支持默认值标记
- **Question 校验** — 类型/选项/内容长度严格校验

#### 🔴 新 crate：core-agent-todo (P1)

- **Todo 模型** — 用户可见进度项，PENDING/IN_PROGRESS/COMPLETED/CANCELLED 状态
- **TodoManager** — 增删改查、批量创建、按 session 分组
- **TodoList** — 排序、完成计数、进度统计
- **同步机制** — `sync_from_step()` 支持与执行步骤状态同步

#### 🔴 新 crate：core-agent-reflection (P1)

- **Reflection 模型** — 评分 0-100、issues、suggestions、criteria
- **ReflectionManager** — 规则评估器（MVP），支持存储/查询
- **阈值检查** — `passes_threshold()` 判断是否达标
- **Retry 控制** — `can_retry()` 限制最大重试次数

#### 🟡 LLMPlanBuilder 增强 (P1)

- **LLMPlanBuilder** — 新增 `core-agent-plan` 的 PlanBuilder 实现，key="llm"
- **from_json()** — 从 LLM 生成的 PlanDraft JSON 解析并校验
- **复用现有 validate()** — 生成后通过 Plan::validate() 校验结构完整性

#### 🟡 集成到 EnterpriseAgent (P1)

- **EnterpriseRuntimes** — 新增 question/todo/reflection 三个字段
- **with_model_and_telemetry()** — 初始化三个新 Runtime
- **CLI 渲染器** — 支持 `todo_list` 和 `reflection_completed` 事件格式化展示

#### 🛠 架构变更

- **3 个新 crate** — `core-agent-question`、`core-agent-todo`、`core-agent-reflection`
- **1 个增强 crate** — `core-agent-plan` 新增 `LLMPlanBuilder`
- **Cargo.toml** — workspace members 和 dependencies 新增三个 crate
- **模块复用** — Planner 复用 core-agent-plan 的 PlanBuilder/PlanDraft；Task 复用 core-agent-execution 的 ExecutionManager

#### ✅ 验证

- `cargo test -p core-agent-question` — 3 个测试全部通过
- `cargo test -p core-agent-todo` — 3 个测试全部通过
- `cargo test -p core-agent-reflection` — 4 个测试全部通过
- `cargo test -p core-agent-plan` — 11+10 个测试全部通过
- 新增 `tests/p1_intelligence_e2e.rs` — 4 个 E2E 测试覆盖完整 P1 流程

## [0.40.0] - 2026-07-21

### P040 Plan + Ask 模块澄清补全 — 对标 Claude Code Plan Mode

实现 `design-docs/040-plan-and-ask.md` 中定义的 Plan + Ask 模块补全，将 Plan 从后端自动机制升级为用户可感知、可交互、可审批的 Plan Mode。

#### 🔴 Plan 审批流程增强 (P0)

- **`/plan-reject` 命令** — 拒绝计划，将 Plan 状态置为 `Cancelled`，带面板输出
- **`/plan-replan` 命令** — 从被拒绝计划的 Goal 重建新计划，带 Re-plan 面板
- **`/plan-approve` 增强** — 面板式输出（带 ⬜/✅ 标记的任务列表 + 执行状态）
- **`/plan-show` 增强** — 展示 Goal→Task→Step 完整层级结构

#### 🟡 Plan Mode 模式切换 (P1)

- **Plan Mode 状态管理** — `EnterpriseAgent` 新增 `plan_mode: RwLock<bool>` 字段
- **`set_plan_mode()` / `plan_mode()` 方法** — 公开 API 管理 Plan Mode 状态
- **只读约束** — Plan Mode 激活时，所有工具调用强制只读（继承已有的 `tool_allowed_in_read_only` 过滤）
- **Plan Mode 入口/出口** — `/plan` 自动进入 Plan Mode，`/plan-approve` 和 `/plan-reject` 退出 Plan Mode
- **`[Plan Mode]` 标识** — 响应中添加 Plan Mode 状态指示

#### 🟡 Todo 连接 PlanningManager (P1)

- **TodoAddTool** — 新增 `planning` 字段，支持 `plan_id` 参数，从 Plan Task 列表创建 Todo
- **TodoListTool** — 新增 `planning` 字段，支持 `plan_id` 参数，读取 Plan Task 作为 Todo 列表（带 ⬜/✅/⏳ 标记）
- **TodoUpdateTool** — 新增 `planning` 字段，支持 `plan_id`/`task_id` 参数，同步 Todo 状态到 Plan
- **todo-runtime provider** — 注册三个带 PlanningManager 的工具，遵循 `plan-runtime` 注册模式

#### 🟡 Auto-Reflection 集成 (P1)

- **`auto_reflect_if_needed()` 方法** — `/plan-approve` 执行完成后自动调用 `/reflect` 认知命令
- **Reflection 事件** — `reflection_completed` 事件发出，包含 Reflection 内容
- **Reflection 输出** — 追加到响应末尾（`---\n\n## Reflection\n\n...`）

#### 🛠 架构变更

- **`EnterpriseAgent`** — 新增 `plan_mode` 字段 + `auto_reflect_if_needed` 方法
- **`todo.*` 工具** — 从 stub 升级为带 PlanningManager 的实现
- **`core-agent-tool/src/builtin/todo/`** — 新增 `*_with_planning` 工厂函数
- **`src/interaction.rs`** — 注册 `plan-reject`、`plan-replan` 两个新 Runtime 命令

#### ✅ 验证

- `cargo check` 编译通过
- `cargo test -p core-agent-plan` — 11+10 个测试全部通过
- `cargo test -p core-agent-tool --lib` — 110 个测试全部通过
- `cargo test --lib` — 94 个测试全部通过
- `cargo test --test planning_runtime_integration` — 通过

## [0.39.1] - 2026-07-20

### P039 Phase 5: `@` 上下文引用 UI 全面增强 — 对标 Claude Code

实现 `design-docs/039-at-context-vs-cc.md` 定义的 `@` 上下文引用前端增强，补齐输出侧渲染和交互体验与 Claude Code 的差距。

#### 🔴 输出侧文件路径可点击 + 文件跳转 (P0)

- **Desktop 消息渲染** — `App.vue` 消息内容中的文件路径（`src/main.rs:42`）自动解析为可点击链接，点击通过系统默认编辑器打开文件
- **Tauri opener 插件** — 新增 `tauri-plugin-opener` 依赖 + `agent_open_file` 命令，支持 `path` 和 `line` 参数
- **CLI TUI 文件路径高亮** — `tui.rs` 新增 `parse_file_paths()` 函数，使用 `regex` 库匹配文件路径并以蓝色+下划线高亮显示

#### 🔴 Context Chip 组件 (P0)

- **`ContextChip.vue`** — 新组件，输入框上方显示当前引用的上下文（文件/选择/消息），支持图标、行号、删除按钮
- **`ContextReference` 类型** — 前端类型定义，覆盖 FILE/SELECTION/MESSAGE 三种引用
- **集成到 App.vue** — 输入框区域引入 Context Chip，支持删除和打开引用

#### 🟡 选中代码引用 UI (P1)

- **选中代码浮动菜单** — `App.vue` 监听 `selectionchange` 事件，选中代码后弹出浮动菜单，点击 "Add to context" 调用后端 `add_reference` 持久化
- **`agent_add_reference` Tauri 命令** — 新增 Tauri 命令，桥接 `ContextRuntime::add_reference` API
- **`AddReferenceRequest` 类型** — `domain.rs` 新增 DTO 类型，支持 FILE/SELECTION 两种引用

#### 🟡 历史消息引用 UI (P1)

- **消息 Quote 按钮** — 每条消息 header 新增 Quote 按钮，点击插入 `@message:id` 引用到输入框
- **`@message:id` 引用格式** — 复用已有的 `@` 补全体系，消息引用作为上下文提交

#### 🟢 代码块来源标注 (P2)

- **Desktop 代码块解析** — `parseContentSegments` 新增 ````lang:path 格式解析，代码块上方显示来源文件路径，`FileCode` 图标 + 可点击跳转
- **CLI 代码块高亮** — `parse_file_paths` 检测 ```` 标记，以 GOLD 颜色高亮语言和文件路径

#### 🟢 引用样式优化 (P2)

- **Context Chip 分色** — FILE=蓝色、SELECTION=粉色、MESSAGE=绿色，按引用类型自动区分
- **入场动画** — `chip-slide-in`（Chip 滑入缩放）和 `selection-fade-in`（浮动菜单淡入）动画效果

#### 🟢 引用 Token 统计 (P2)

- **Context Chip 消耗显示** — `ContextChip.vue` 新增 `totalTokens` prop，显示 `📊 12.5K` 格式的上下文消耗估计值
- **集成到 App.vue** — 从 `contextUsage.totalTokens` 取值传入 Context Chip

#### 🛠 架构变更

- **Desktop API** — `DesktopApi` 接口新增 `openFile(path, line?)` 和 `addReference()` 方法，`TauriDesktopApi` 和 `HttpDesktopApi` 分别实现
- **Controller** — `controller.ts` 新增 `openFile`、`addReference` 方法和 `contextReferences` 状态
- **CLI 依赖** — `agent-cli/Cargo.toml` 新增 `regex` 依赖
- **测试更新** — `controller.test.ts` 的 `FakeApi` 新增 `openFile`、`addReference` 桩方法

#### 🔬 验证

- `cargo check` 编译通过（agent-cli + agent-desktop）
- `vue-tsc --noEmit` 类型检查通过
- `vitest run` 8 个测试文件 22 个测试全部通过
- `cargo test` 全部通过

## [0.39.0] - 2026-07-20

### P039 Phase 5: Agent 全面增强 — 对标 Claude Code / ChatGPT / OpenCode

实现 `design-docs/039-agent-vs-cc.md` 定义的 Agent 全面增强，扩展 SubAgent Profile 从 3 个 → 9 个，新增工具集可配置和最大轮数可配置，新增 3 个 Slash 命令。

#### 🧠 SubAgent Profile 扩展（3 → 9）

| Profile | 工具集 | 最大轮数 | 用途 |
|---------|--------|:--------:|------|
| **General** | filesystem.read, guidance.read, memory.read, process.read | 4 | 通用委托任务 |
| **Explore** | filesystem.read, guidance.read | 4 | 只读探索 |
| **Review** | filesystem.read, guidance.read, memory.read | 4 | 代码审查 |
| **Test** 🆕 | filesystem.*, process.*, guidance.read, memory.read, git.* | 8 | 测试分析/生成 |
| **Debug** 🆕 | filesystem.read, process.*, git.*, memory.read | 8 | 错误诊断/根因分析 |
| **SecurityReview** 🆕 | filesystem.read, guidance.read, memory.read, git.* | 6 | 安全漏洞审计 |
| **Doc** 🆕 | filesystem.*, guidance.read, network.read, git.*, memory.read | 6 | 文档生成/更新 |
| **Migration** 🆕 | filesystem.*, guidance.read, git.*, memory.read | 6 | 代码迁移/升级 |
| **Architecture** 🆕 | filesystem.read, guidance.read, memory.read, git.* | 6 | 架构分析/设计评审 |

#### 🛠 核心架构变更

- **工具集可配置化** — `allowed_categories()` 方法支持通配符匹配（`filesystem.*`、`process.*`、`git.*`），不再硬编码 4 类只读工具
- **最大轮数可配置化** — `max_turns()` 方法将硬编码 4 轮改为 Profile 级配置（Test/Debug 8 轮，SecurityReview/Doc/Migration/Architecture 6 轮）
- **Profile 枚举扩展** — `SubAgentProfile` 从 3 个 → 9 个，保持 `Clone + Copy + Serialize/Deserialize`
- **delegate_task tool 更新** — JSON Schema 的 profile enum 同步扩展为 9 个值

#### 🆕 新 Slash 命令

| 命令 | 路由 | 功能 |
|------|------|------|
| `/test` | SubAgent(Test) | 测试分析/生成/诊断 |
| `/debug-agent` | SubAgent(Debug) | 错误诊断/根因分析 |
| `/security-review` | SubAgent(SecurityReview) | 安全漏洞审计 |

#### 📁 新增文件

- `src/slash/commands/test.rs` — Test Agent 命令实现
- `src/slash/commands/debug_agent.rs` — Debug Agent 命令实现
- `src/slash/commands/security_review.rs` — Security Review Agent 命令实现
- `src/slash/agent_plugin.rs` — Agent 命令注册插件（类似 `society_plugin.rs`）

#### ✅ 测试验证

- 7 个单元测试覆盖：所有 Profile 的 prompt 非空、工具集包含 filesystem 访问、max_turns 正确、parse 拒绝未知值、通配符匹配
- 全量 lib 测试 94 个全部通过

## [0.38.4] - 2026-07-20

### P038 Phase 4: Agent Cognitive Runtime — 6 个认知命令

实现 `design-docs/038-05-slash-p4-agent-conitive.md` 定义的 Phase 4 Agent Cognitive Runtime，
让 Agent 从"任务执行器"升级为"具备分析、反思、决策能力的智能体"。

#### 新命令

| 命令 | 用法 | 功能 |
|---|---|---|
| `/reason` | `/reason [question]` | 问题分析，收集证据、识别原因、输出推理摘要 |
| `/think` | `/think <task>` | 复杂任务分析，识别约束、生成选项、评估推荐 |
| `/hypothesis` | `/hypothesis [topic]` | 假设管理，支持证据和反证 |
| `/critic` | `/critic [target]` | 自我批判，发现弱点、安全问题、评分 |
| `/reflect` | `/reflect [task]` | 反思学习，记录经验教训 |
| `/decision` | `/decision [topic]` | 决策记录，自动生成 ADR 到 `docs/adr/` |

#### 新增模块

- `src/cognitive.rs` — Cognitive Engine，包含：
  - `CognitiveCommand` 枚举 + 6 个命令的 Prompt 模板
  - `AdrEntry` 数据模型 + Markdown 渲染 + 文件写入
  - `CognitiveOutput` 结构化输出处理
  - `/decision` → ADR 自动生成（`docs/adr/NNNN-title.md`）

#### 架构变更

- `src/slash/mod.rs` — 新增 `SlashCategory::Cognitive` 枚举变体
- `src/interaction.rs` — 注册 6 个认知命令到 `with_builtins()`，新增 `cognitive_command()` 方法，`model_prompt()` 支持认知命令专用模板
- `src/enterprise.rs` — `run_with_approval_inner` 中增加认知命令后处理（ADR 生成事件）
- `src/lib.rs` — 导出 cognitive 模块

#### 关键设计

- 所有认知命令通过 `InteractionCommandRoute::Agent` 路由，调用模型分析
- `/reason`/`/think`/`/hypothesis`/`/critic` 为只读命令，不触发文件变更
- 结构化输出格式（非 CoT 思维链暴露）
- `/decision` 自动创建 `docs/adr/` 目录并写入 Markdown 格式 ADR

#### 测试

- 16 个单元测试全部通过（Cognitive Engine: 12, ADR: 4）
- 所有已有交互测试通过

## [0.38.3] - 2026-07-20

### P038 Phase 3: Agent Society Slash Commands — 5 个新命令

实现 `design-docs/038-04-slash-p3-agent-society.md` 定义的 Phase 3 Agent Society 层，让 Agent 从"单打独斗"升级为"多 Agent 社会协作"。

#### 新命令

| 命令 | 用法 | 功能 |
|---|---|---|
| `/agents` | `/agents` | 查看 Agent Society 全景：组织、角色、团队、成员状态 |
| `/delegate` | `/delegate <task> [--role <role>] [--priority <p>]` | 任务委派到指定角色/团队，支持优先级控制 |
| `/team` | `/team start\|status\|list\|activate\|complete\|archive [args]` | 团队全生命周期管理 |
| `/roles` | `/roles` | 查看所有可用角色及其能力要求 |
| `/collaborate` | `/collaborate [team-id]` | 查看团队协作过程和 Collaboration 状态 |

#### 架构变更

- 新增 `SlashCategory::Society` 分类，归类到 society 类别
- 所有命令通过 `InteractionCommandRoute::Runtime` 路由（零模型调用，即时响应）
- 每个命令 struct 持有 `Arc<MultiAgentManager>` 状态注入
- `EnterpriseAgent::execute_command()` 中直接路由到对应命令实现

#### 核心设计

- **Stateful 命令模式**：每个命令 struct 通过构造函数注入 `Arc<MultiAgentManager>`
- **UML 风格输出**：`🧠 Planner (ready)` / `🛠 Coder (running)` 图标化显示
- **与 `core-agent-multi` 完整打通**：Organization → Role → Team → Member → Collaboration 全链路

#### 变更文件

- `src/slash/mod.rs` — 新增 `SlashCategory::Society` 枚举变体 + `society_plugin` 模块
- `src/slash/commands/agents.rs` — `/agents` 命令实现（含 3 个单元测试）
- `src/slash/commands/delegate.rs` — `/delegate` 命令实现（含 5 个单元测试）
- `src/slash/commands/team.rs` — `/team` 命令实现（含 7 个单元测试）
- `src/slash/commands/roles.rs` — `/roles` 命令实现（含 2 个单元测试）
- `src/slash/commands/collaborate.rs` — `/collaborate` 命令实现（含 3 个单元测试）
- `src/slash/society_plugin.rs` — SocietyCommandPlugin 注册入口
- `src/interaction.rs` — 注册 5 个 Society 命令到内置命令表
- `src/enterprise.rs` — 实现 5 个命令的完整执行逻辑
- `src/lib.rs` — 导出 SocietyCommandPlugin

#### 测试

- 20 个单元测试全部通过（agents: 3, delegate: 5, team: 7, roles: 2, collaborate: 3）
- `cargo check` 编译通过

## [0.38.6] - 2026-07-20

### P038 Phase 6: Agent Observability & Evaluation Runtime — 6 个新命令

实现 `design-docs/038-07-slash-p6-observe.md` 定义的 Phase 6 Observability & Evaluation 命令，让 Agent 从 Black Box 变为 Observable Intelligent System。

#### 新命令

- `/trace-agent [trace-id]` — Agent 执行链追踪，查看完整时间线和步骤
- `/evaluate <trace-id>` — 多维度任务质量评估（Correctness/Safety/Efficiency/Maintainability）
- `/benchmark [agent-id]` — 能力基准测试，5 个内置任务（coding/doc/arch/security/testing）
- `/debug <trace-id>` — Agent 调试，定位失败根因并给出修复建议
- `/replay <trace-id>` — 基于事件溯源的历史执行回放
- `/score [agent-id]` — Agent 健康度仪表盘（成功率/平均评分/成本/延迟）

#### 架构特点

- 新增 `SlashCategory::Observability` 分类
- SQLite 持久化存储（5 表：agent_trace / trace_step / tool_execution / evaluation / benchmark_result）
- TraceCollector 自动采集 Agent 执行事件
- EvaluationEngine 多维度规则评分（无需 LLM 调用）
- DebugEngine 错误分类与根因分析（Permission/NotFound/Timeout/Invalid/RateLimit/Network）
- ReplayEngine 事件溯源回放
- CLI 新增 6 个子命令（trace-agent / evaluate / benchmark / debug / replay / score）
- 与 EnterpriseAgent 的 execute_command 深度集成

## [0.38.5] - 2026-07-20

### P038 Phase 5: Workflow Runtime Slash Commands — 6 个新命令

实现 `design-docs/038-06-slash-p5-workflow.md` 定义的 Phase 5 Workflow 命令，让 Agent 从 Interactive Agent 向 Autonomous Agent 演进，具备工作流管理、事件触发、定时任务、手动执行、运行观察和失败恢复能力。

#### 新增命令

| 命令 | 用法 | 功能 |
|---|---|---|
| `/workflow` | `/workflow [show <key>]` | 工作流管理：列表所有 Workflow 定义（名称/版本/状态），或 `show <key>` 查看详情 |
| `/trigger` | `/trigger [create <name>]` | 事件触发管理：列出支持的触发器类型，`create <name>` 占位 |
| `/schedule` | `/schedule [create <name> [cron <expr>]]` | 定时任务管理：列出调度任务，`create <name>` 占位 |
| `/run` | `/run <workflow-key>` | 手动执行 Workflow，通过 `WorkflowManager::start()` 启动，返回 Instance ID |
| `/observe` | `/observe <instance-id>` | 运行观察：展示 Stage/Activity/Action 完整进度和状态 |
| `/retry` | `/retry <instance-id>` | 失败恢复：先创建 Snapshot 检查点，再通过 `WorkflowManager::resume()` 恢复 |

#### 架构变更

- 新增 `SlashCategory::Workflow` 分类，归类到 workflow 类别
- 所有命令通过 `InteractionCommandRoute::Runtime` 路由（零模型调用，即时响应）
- 直接复用 `EnterpriseRuntimes.workflows`（`Arc<WorkflowManager>`），与现有 `core-agent-workflow` 完整打通
- `/run` 真实验证 Workflow 注册状态并启动，`/observe` 读取真实 Instance 进度数据，`/retry` 使用 Snapshot/Resume 机制

#### 设计决策

- **注入方式**：在 `execute_command` 中通过 `self.runtimes.workflows` 直接访问 WorkflowManager（已有引用）
- **Trigger/Schedule 策略**：第一阶段占位命令，提示信息并预留接口，事件引擎后端后续实现
- **Retry 策略**：先 `snapshot()` 创建检查点，再 `resume()` 恢复，不走简单重跑

#### 变更文件

- `src/slash/mod.rs` — 新增 `SlashCategory::Workflow` 枚举变体
- `src/interaction.rs` — 注册 6 个 Workflow 命令到内置命令表
- `src/enterprise.rs` — 实现 6 个命令的完整执行逻辑

#### 测试

- `cargo check` 通过（本项目无新增编译错误）

### P038 Phase 2: Memory & Knowledge Runtime — 5 个新命令

实现 `design-docs/038-03-slash-p2-memory.md` 定义的 Phase 2 Memory & Knowledge 命令，让 Agent 可以从"一次性 Coding Agent"升级为"长期进化的 Engineering Agent"。

#### 新命令

- `/memory-show [scope]` — 查看项目/会话记忆列表，支持 scope 过滤（project/session/all）
- `/memory-save <content> [--scope] [--type] [--importance]` — 快速保存记忆，支持类型和重要性标记
- `/memory-clear <scope> [--confirm]` — 清除记忆（软删除，通过 `MemoryManager.archive()` 标记为 Archived）
- `/knowledge` — 查看知识库状态和存储信息
- `/learn <path> [--recursive]` — 从文件/目录扫描知识，提取关键信息保存为记忆条目

#### 架构特点

- 全部使用 Runtime 路由（零模型调用），即时响应
- 复用 `SlashCategory::Memory` 分类
- 基于 `core-agent-memory` 完整实现（`MemoryManager`、`SqliteMemoryStore`、`StructuredMemoryRetriever`）
- 软删除机制：`/memory-clear` 使用 `MemoryManager.archive()` 而非 `forget()`
- 保持现有 `/memory` 命令向后兼容

#### 决策记录

- **输入方式**：`/memory-save` 支持直接参数模式，无参数时提示
- **删除方式**：软删除（archive），可恢复
- **知识库 MVP**：扫描文件提取关键信息存为记忆条目

#### 新增文件

- `src/slash/commands/memory_show.rs` — `/memory-show` 命令实现
- `src/slash/commands/memory_save.rs` — `/memory-save` 命令实现，支持 --scope/--type/--importance
- `src/slash/commands/memory_clear.rs` — `/memory-clear` 命令实现，支持 --confirm
- `src/slash/commands/knowledge.rs` — `/knowledge` 命令实现
- `src/slash/commands/learn.rs` — `/learn` 命令实现，支持 --recursive

#### CLI 入口

- 新增 `agent memory-show`, `agent memory-save`, `agent memory-clear`, `agent knowledge`, `agent learn` 顶层子命令

#### 测试

- 编译通过：`cargo check -p core-agent -p agent-cli` 0 error
- 全量测试通过

## [0.38.1] - 2026-07-20

### P038 Phase 1: Code Intelligence & Tool Governance Runtime — 5 个新命令

实现 `design-docs/038-02-slash-p1-search.md` 定义的 Phase 1，新增 5 个代码智能和治理命令，让 Agent 从"聊天 + 修改文件"升级为"理解整个工程 + 安全执行工程操作"。

#### 新命令

- `/search <query> [--type <lang>] [--kind <kind>]` — 代码符号搜索，基于 `code_index.query` 工具
- `/trace <function> [--depth <n>]` — 函数调用链分析，基于 `callgraph.query` 工具
- `/architecture [--format <json|text>]` — 项目架构图，基于 `architecture.graph` + `project.analyzer` 工具
- `/permissions` — 查看当前 Agent 权限状态（PermissionMode、Memory 等）
- `/approve <list|id>` — 查看和管理待审批操作

#### 架构复用

- 全部使用 Runtime 路由（零模型调用），即时响应
- 复用 Phase 0.5 的 `SlashCommand` trait + `SlashCommandRegistry` 架构
- 通过 `SlashCategory::Project` 和 `SlashCategory::Governance` 分类
- 注册到 `InteractionCommandRegistry` 保持所有入口兼容

#### 新增文件

- `src/slash/commands/search.rs` — `/search` 命令实现，支持 --type/--kind/--path 过滤
- `src/slash/commands/trace.rs` — `/trace` 命令实现，支持 --depth 深度控制
- `src/slash/commands/architecture.rs` — `/architecture` 命令实现，支持 --format 输出格式
- `src/slash/commands/permissions.rs` — `/permissions` 命令实现，只读查看
- `src/slash/commands/approve.rs` — `/approve` 命令实现，支持 list/ID 审批

#### CLI 入口

- 新增 `agent search`, `agent trace`, `agent architecture`, `agent permissions`, `agent approve` 顶层子命令
- 所有命令在 Chat 模式中通过 `/` 前缀也可使用

#### 测试

- 编译通过：`cargo check -p core-agent -p agent-cli` 0 error
- 全量测试通过：`cargo test -p core-agent -p agent-cli` 全部通过

## [0.38.0] - 2026-07-20

### P038: Phase 0.5 Slash Runtime Foundation — 统一 Slash Command Runtime + 4 个新命令

实现 `design-docs/038-00-slash-overview.md` 和 `design-docs/038-01-slash-p0-mvp.md` 定义的 Phase 0.5，建立统一的 Slash Command Runtime 基础设施，新增 4 个核心命令。

#### 新架构：Slash Command Runtime

- 新增 `SlashCommand` trait：`metadata()` → `category()` → `validate()` → `execute()` 完整生命周期接口
- 新增 `SlashCommandRegistry`：在保留 `InteractionCommandRegistry` 向后兼容的基础上，支持插件式 `SlashCommand` 注册
- 新增 `SlashCategory` 枚举：9 种分类体系（System/Session/Context/Project/Memory/Agent/Checkpoint/Governance/Developer）
- 新增 `SlashCommandObserver` trait：命令执行事件监听（start/success/failure），用于审计和指标
- 新增 `CommandMetadata`/`CommandContext`/`CommandOutput`/`CommandAction` 等核心类型

#### 新命令

- `/context` — 显示 Agent 上下文状态（当前支持占位输出，后续可对接 `ContextRuntime` 展示 token 用量）
- `/compact` — 手动触发上下文压缩，基于 `SummaryReducer` 的 last-N + extractive summary 策略
- `/resume <session-id>` — 恢复已暂停的会话，重新加载上下文（连接 `SessionRuntime::resume_session()`）
- `/checkpoint <save|list|restore>` — 创建/列出/恢复命名 checkpoint，比 undo/redo 更显式

#### CLI 入口更新

- 新增 `CliCommand::Compact` 和 `CliCommand::Checkpoint` CLI 子命令
- 新命令通过 `professional.execute_line()` 统一路由到 `EnterpriseAgent::execute_command()`
- 保持与现有 TUI/Desktop 自动补全兼容

#### 新增文件

- `src/slash/mod.rs` — Slash Command Runtime 核心基础设施
- `src/slash/commands/mod.rs` — 命令模块导出
- `src/slash/commands/context.rs` — `/context` 命令实现
- `src/slash/commands/compact.rs` — `/compact` 命令实现
- `src/slash/commands/resume.rs` — `/resume` 命令实现
- `src/slash/commands/checkpoint.rs` — `/checkpoint` 命令实现（save/list/restore 子命令）

#### 测试

- 编译通过：`cargo check -p core-agent` + `cargo check -p agent-cli` 0 error
- 全量测试通过：`cargo test -p core-agent` 75 个 + `cargo test -p agent-cli` 5 个

## [0.37.0] - 2026-07-20

### P037: Context Annotation Runtime — 上下文注解/引用能力

实现 `design-docs/037-context-comment.md` 定义的 Context Annotation 能力，用户可以选中文件、代码段或历史消息作为上下文补充，让 Agent 知道"看这里"。

#### 核心模型

- 新增 `context_reference` 领域模型：`ContextReference`、`ReferenceType`（File/Selection/Message）、`ReferenceLocator`、`ContextPackage`
- 扩展 `ContextSource` 枚举：新增 `Reference` 变体
- 扩展 `ContextSlot` 枚举：新增 `Reference` 槽位（优先级 25），位于 User 之后
- 扩展 `Context` 结构体：新增 `references: Vec<ContextReference>` 字段
- 扩展 `TokenDistribution`：新增 `reference: u64` 字段

#### 持久化

- 新增 `context_reference` SQLite 表：id/session_id/reference_type/locator/snapshot/metadata/created_at + 审计字段
- 新增 `SqliteContextReferenceStore`：save/load/list/delete/clear 完整 CRUD，遵循 r2d2 + spawn_blocking 模式

#### Provider 扩展

- `UserProvider`：解析 File 和 Selection 引用，从文件系统读取行范围内容
- `ConversationProvider`：解析 Message 引用，从 SessionStore 按 ID 获取消息
- `DefaultComposer`：处理 Reference Slot，将引用段反序列化为 `ContextReference` 写入 `Context.references`

#### API & CLI

- `ContextRuntime` 新增：`add_reference()` / `list_references()` / `delete_reference()` / `clear_references()`
- `ContextApplicationService` 新增：`with_stores()` / `add_reference()` / `list_references()` / `delete_reference()` / `clear_references()`
- 注册 `/comment` 和 `/context` 命令到 `InteractionCommandRegistry`
- 新增 DTO：`AddReferenceRequest`、`ReferenceResponse`、`ReferenceSummary`

#### 测试

- 61 个单元测试通过（新增 context_reference + reference_store 单元测试）
- 6 个端到端测试通过（新增 reference_round_trip_and_context_inclusion）
- 全工作区编译通过，0 warning

## [0.36.0] - 2026-07-20

### P036: Tools 增强 — 代码智能 + 工程理解 + 运维/企业/AI 工具

在 44 个基础工具之上，按设计文档 036-tools-enhance.md 实现 Phase 1~5 共 ~31 个新工具，工具总数达到 **75 个**：

- **Phase 1: 代码智能工具（12 个新增+增强）**
  - `ast.search` / `ast.replace` — 基于正则的 AST 感知代码搜索和替换，支持 20+ 编程语言过滤
  - `code_index.index` / `code_index.query` — 符号索引，支持 Java/Rust/Python/TS/Go 等语言的类、方法、字段提取
  - `dependency.inspect` — 依赖分析，支持 Java(Maven/Gradle)、Rust(Cargo)、Node.js(npm)、Python(pip)
  - `decompiler.decompile` — Java 反编译，支持 .class 文件和 .jar 归档，使用 javap
  - LSP 6 个工具从 stub 升级为真实实现：definition/references/hover/completion/diagnostics/symbols，使用 grep 增强搜索

- **Phase 2: 工程理解工具（4 个新增）**
  - `project.analyzer` — 项目结构分析，识别构建系统和框架
  - `architecture.graph` — 架构依赖图，支持 JSON/text 输出
  - `callgraph.query` — 函数调用链分析
  - `api.analyzer` — REST API 端点扫描，支持 Spring Boot/JAX-RS/Express/Actix

- **Phase 3: 运维工具（5 个 stub）**
  - `log.query` / `metric.query` / `trace.query` / `cmdb.query` / `k8s.query` — 预留 ELK/Prometheus/Jaeger/CMDB/K8s 接口

- **Phase 4: 企业工具（5 个 stub）**
  - `knowledge.search` / `ticket.create` / `notification.send` / `browser.navigate` / `browser.screenshot` — 预留知识库/工单/通知/浏览器接口

- **Phase 5: AI 工具（5 个 stub）**
  - `code.review` / `test.generate` / `security.scan` / `data.analyze` / `vision.analyze` — 预留代码审查/测试生成/安全扫描/数据分析/视觉接口

- 139 个测试全部通过（113 个单元测试 + 16 个 E2E + 10 个 Runtime 集成测试）
- 新增 `RawToolOutput::json()` 方法，支持 JSON 格式输出
- 架构完全遵循已有模式：每个工具独立 struct + 单元测试，通过 BuiltinToolProvider 注册

### P035: 内置工具体系 — 44 个插件化工具，博采众长

参考 OpenCode / Claude Code / Codex 三家设计理念，在 `core-agent-tool` 已有 Runtime 基础设施之上实现了完整的**内置工具体系**：

- **44 个内置工具**全部通过 `BuiltinToolProvider` 插件化注册，覆盖 10 个类别：📁 File (11) — read/write/edit/patch/glob/grep/delete/move/copy/info/list；💻 Shell (3) — exec/script/bg；🔧 Git (7) — diff/status/log/commit/branch/checkout/push；🌐 Web (2) — fetch/search；💬 Ask (3) — user/confirm/select；✅ Todo (3) — add/update/list；🤖 Agent (3) — spawn/send/list；📋 Plan (3) — create/update/review；⏰ Cron (3) — create/list/delete；📝 LSP (6) — definition/references/hover/completion/diagnostics/symbols。
- 每个工具是独立的 `struct` 实现 `Tool` trait，自带 JSON Schema 输入校验、`ToolCapability` 能力路径、`PermissionDecision` 默认权限和超时配置。
- 新增 `ConfigDrivenPermission` 支持配置覆盖（`core-agent-config.yaml` 中 `tools.overrides`）和通配符能力组匹配（`file.*` → Allow）。
- 不修改 `ToolManager`/`ToolRegistry`/`ToolCatalog` 等任何现有核心基础设施。
- 99 个测试全部通过（73 个单元测试 + 16 个 E2E 集成测试 + 10 个已有 Runtime 集成测试），覆盖工具独立执行、ToolManager 完整链路、权限控制、能力匹配和 Tool Schema 有效性。

## [Unreleased]

### P034: Enterprise Agent Core, Tools and Extension Loop

- 新增用户/项目 `AGENTS.md`/override 指令链与 system/user/project Skills progressive disclosure；完整 `SKILL.md` 仅通过受控 `load_skill` 延迟加载并校验哈希。
- Memory 切换为独立 SQLite project/session 持久化，相关条目自动进入 Context；新增受审 `remember_memory`、有界 `recall_memory` 和版本化 `forget_memory`，拒绝疑似凭据正文。
- 新增 `find_files`、正则 `search_files` 和带 current SHA-256/歧义拒绝/Checkpoint 的 `apply_patch`；命令执行升级为结构化 stdout/stderr/exit、流式观察、超时/取消、输出上限、敏感环境剥离与进程树终止。
- 新增后台 `start/poll/cancel_command`、最多四轮且只开放读取 Tool 的隔离 `delegate_task`；Linux bubblewrap 提供 best-effort OS sandbox，策略可要求 backend 缺失时 fail-closed。
- 新增可配置 OpenAI Responses `web_search` 与 SSRF-safe `web_fetch`，返回可引用来源 URL；新增显式启用的 Hooks 与 MCP stdio initialize/tools/list/tools/call，统一经过 Command/Tool Permission。
- 新增不可被项目覆盖的 managed policy，可集中限制 Tool/category、MCP server、Web domain、Memory/Hooks/Web 与 sandbox；Enterprise 主链新增 guidance、memory 和 hook 无正文观测。
- 新增 41 个核心单元断言和跨 Runtime E2E，验证 Skill 延迟加载、工作区搜索、受审 Memory 写入、Runtime 重开自动召回及既有 Enterprise 工具/审批回归。

## [0.3.0] - 2026-07-19

### P033: Desktop Conversation, Model Configuration and Observability

- 用户配置升级到兼容读取 v1 的 v2 多模型 schema：全局 `activeModel`、大小写不敏感的唯一 `name`、`baseURL`、API Key/ref 与默认 128K Token 上下文；Desktop 通过脱敏 DTO、fingerprint/CAS 和原子替换写入 Terminal 共用的用户文件。
- `EnterpriseAgent` 增加 content-free 请求观测：稳定 request ID、入口/模型/状态、Context Token、输入/输出 Usage、wall/active/审批等待与 Context/Model/Tool 阶段耗时；全局 SQLite 使用 WAL，观测失败不遮蔽已完成的模型响应。
- Context 增加可配置的 `recent-window` / `extractive-summary`、触发阈值、保留消息数和内容无关占用快照；Desktop 输入区增加 Context 圆环/数字 tooltip，设置页增加消耗日历、趋势图和最近请求列表。
- Desktop 重构为项目/会话、主对话、文件上下文加窄导航的响应式布局，恢复历史会话，保留高级 Workspace，并新增临时权限切换、亮/暗主题与中英文偏好；Terminal/Desktop 均实时显示请求耗时。

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
