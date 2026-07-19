# P033 Desktop Opt — Unknowns Report

## Metadata

- **Task / Feature:** Desktop 主交互、共享多模型配置、Usage/耗时统计、Context 可视化与压缩配置
- **Mode:** Deep
- **Date:** 2026-07-19
- **Prepared by:** Codex
- **Scope:** `core-agent-config`、`core-agent-model`、`core-agent-context`、根 `EnterpriseAgent`、Terminal TUI、Tauri/Vue Desktop、SQLite、本地文档与测试

## Intent

### User-visible problem

当前 Desktop 仍以 Runtime 面板为中心，项目、Session、对话、文件、Trace 和 Execution 混在同一 Console；模型只能通过单模型配置启动，设置页不能编辑；Terminal/Desktop 虽读取同一配置解析链，但模型 Usage 分别落在各自项目 Runtime 中。上下文只在 Trace 中显示一次构建结果，用户在输入区看不到容量；现有压缩器不可由页面选择；请求进行中仅有 busy/spinner，完成后展示的耗时又只是单次模型调用延迟，而不是用户一次请求的真实端到端耗时。

### Desired behavior change

- Desktop 主界面形成“功能窄栏 / 项目与 Session / 对话 / 当前项目文件树”四区结构，Trace、Changes、Execution 不再直接干扰主对话，但高级入口仍保留。
- 最近项目可添加和切换；每个项目列出可新建、可选择、可继续的 Session，并恢复 MAIN Conversation 历史。
- 输入框提供与既有 `@`、`/` 完全同源的 `+` 和 `/` 可视入口。
- 设置页支持同一份 Terminal/Desktop 用户配置中的多个模型；`name` 唯一，可新增、编辑、删除、选择当前模型，并校验 `baseURL`、API Key 和上下文上限。
- 按实际响应模型 `name` 记录输入、输出、缓存和总 Token；Desktop 提供日历和趋势图。
- 输入框旁使用圆形容量指示器显示当前上下文估算占用，悬浮显示精确数值和组成。
- 页面配置内置压缩策略，并保留受控的 ContextReducer 扩展合同；配置由 Terminal/Desktop 共用。
- Terminal/Desktop 从请求提交开始实时显示已耗时，完成后保存请求级终态耗时和可用阶段明细。
- Desktop 支持亮/暗主题、`zh-CN`/`en`，偏好可恢复。

### Affected users and workflows

- Desktop 用户：项目/Session 切换、历史恢复、上下文补充、权限与模型选择、统计查看、主题和语言切换。
- Terminal 用户：继续使用同一配置，在 TUI 忙碌状态实时看到本次请求耗时，并产生与 Desktop 可汇总的统计。
- Runtime/扩展开发者：从稳定的 Context 压缩 SPI 和非敏感观测 DTO 扩展策略，不接触 API Key 或跨工作区数据。
- 配置维护者：Desktop 写入的内容必须能被下一次 Terminal 启动读取；项目/环境覆盖仍需可解释。

### Success criteria

- Terminal/Desktop 对同一用户配置解析出相同的模型集合、当前模型和压缩配置；写入前完整校验，写入使用原子替换和并发冲突检测。
- 配置中两个模型 `name` 相同、必填项为空、URL 非法、容量为零或未知压缩策略时均 fail-closed，旧单模型配置可确定性迁移。
- API Key 不进入 Debug、错误、Tauri 响应、Usage、Context snapshot 或日志；设置页只显示“已配置”，不回传原文。
- 每个模型调用继续记录 Provider Usage；一个用户请求的所有模型调用可按实际 `name` 汇总，Terminal/Desktop 的本地统计口径一致。
- 每个用户请求具有稳定 `request_id`、开始/终止时间、状态和耗时；成功、失败、拒绝、取消和重启中断都有明确终态。
- 实时计时与持久化终值使用同一请求边界；UI/TUI 显示值允许刷新误差，但最终值不得拿模型 `latency_ms` 代替。
- Context 指示器使用当前模型容量作为分母，显示“估算”语义；悬浮可见总量、上限、占比和槽位分布。
- 压缩策略配置真正进入 `ContextPipeline/ReducerConfig`；不再像现在一样每次请求硬编码 `enable_summary=false`。
- 主 Console 不再直出 Trace/Changes/Execution；项目和 Session 切换不会让旧异步结果写回新项目。
- Rust 单元断言/集成/E2E、Vue 单元/App 流程 E2E、Terminal TUI 断言、前端构建、全 workspace 测试与零警告 Clippy 全部通过。

### Non-goals

- 不做云配置同步、账户系统、跨设备 Usage 合并或远程计费结算。
- 不默认加载任意脚本、动态库或网络压缩插件；扩展必须走受控注册合同。
- 不声称估算 Token 等于供应商 tokenizer 的精确计数；响应 Usage 仍以 Provider 返回为准。
- 不自动翻译模型回复、项目文件、命令输出和 Runtime 动态数据。
- 不做多项目 Runtime 后台并发、Session 删除/重命名/云同步。
- 不移除 Studio、Collaboration、Enterprise、Ecosystem 或高级 Trace/Changes/Execution 功能。

## Evidence Reviewed

| Source | Location | What it confirms | Confidence |
|---|---|---|---|
| Updated design | `design-docs/033-desktop-opt.md` | 新布局、多模型共享配置、Usage、Context、压缩、请求耗时、主题、i18n 和测试要求 | High |
| Prior configuration design | `design-docs/029-config-opt*.md` | 全局用户配置、优先级、plaintext/API Key 引用、脱敏和 symlink/体积 fail-closed 已有决策 | High |
| Prior Desktop design | `design-docs/032-desktop-unified-workspace*.md` | 单活动 Runtime、项目数据隔离、共享 `/`/`@` 与审批安全边界 | High |
| Configuration domain | `core-agent-config/src/domain.rs` | schema v1 只有单个 `model`；Model 无上下文上限，Context 无压缩配置；Patch 只反序列化 | High |
| Configuration providers | `core-agent-config/src/providers.rs`, `manager.rs` | `env > project > user > builtin`；支持 YAML/JSON 与 env secret ref，但没有写入器 | High |
| Model domain/store | `core-agent-model/src/domain/*`, `persistence/*` | Profile 上下文按 Token，默认 128K；Usage 已含输入/输出/cache/total/latency/cost，SQLite 可列出但只支持逐条读取 | High |
| Context runtime | `core-agent-context/src/*` | `ContextReducer` 已公开；Context 有 TokenDistribution/build_duration；默认 reducer 是确定性裁剪，组合根关闭 summary | High |
| Enterprise composition | `src/enterprise.rs` | 每个项目 `model.db` 同时承担 Catalog/Usage；请求最多 8 次模型循环；事件无时间戳，请求无持久化指标 | High |
| Terminal | `agent-cli/src/tui.rs`, `app.rs` | 请求后台执行、约 60ms 重绘，只有 busy/spinner；可本地实时计时，但无最终请求指标 | High |
| Desktop backend | `agent-desktop/src-tauri/src/lib.rs`, `store.rs` | 单活动 Runtime；项目数据按 hash 隔离；Session 历史后端能力存在；偏好使用 SQLite CAS | High |
| Desktop frontend | `agent-desktop/src/App.vue`, `controller.ts`, components | 当前 Console 直接放 Trace/Changes/Execution；单 Runtime/session；静态英文；两套主题均为暗色 | High |
| Existing tests/docs | Rust/Vue tests, `README.md`, `CHANGELOG.md` | 有单元、Happy DOM、SQLite/Runtime E2E 基础；尚无真实浏览器/Tauri-driver 基础 | High |

## Confirmed Facts

| Fact | Evidence | Relevance |
|---|---|---|
| “同一配置”目前只意味着读取链相同 | Terminal 与 Desktop 都调用 `standard_config_manager`，但无 writer | 页面保存需要新增共享写入合同，而不是仅加 Vue 表单 |
| 当前 schema 不能表达多个模型 | `AgentConfig.model: ConfigModel`, `CONFIG_SCHEMA_VERSION = 1` | 必须有版本迁移和 active model 语义 |
| `name`、Profile key、Provider key、wire model name 当前是不同概念 | `ModelProfile` 和 `EnterpriseModelConfig` | 用户只要求一个 `name` 时必须冻结映射，避免统计和路由错位 |
| 模型上下文硬限制单位是 Token | `ModelLimits.context_tokens = 128_000` | “默认 200kb”不能静默转换为 200K Token |
| Context mention 字节上限不等于模型上下文上限 | `ConfigContext.max_total_bytes` 与 `ModelLimits.context_tokens` | 配置字段必须区分文件注入字节预算与模型 Token 窗口 |
| Provider Usage 已经覆盖输入/输出/缓存/总 Token | `ModelUsage`, `model_usage` | 不应重复发明 Token 计量；应补聚合查询和共享存储边界 |
| 当前 Usage 不是 Terminal/Desktop 共享 | Terminal `.agent/runtime/model.db`；Desktop `app_data/projects/<hash>/runtime/model.db` | 直接画图会漏掉另一入口和其他项目 |
| 一次用户请求可能触发 1–8 次模型调用 | `run_with_approval` tool loop | 日历既要支持调用级明细，也要避免把“请求数”和“模型调用数”混为一谈 |
| 模型 `latency_ms` 只覆盖单次 Provider 调用 | `ModelUsage.latency_ms` | 不能满足“每一次请求处理耗时” |
| Context 已有构建耗时和分槽 Token | `Context.build_duration_ms`, `TokenDistribution` | 可直接作为请求阶段明细和 tooltip 数据源 |
| 工具结果有各自时长信息，但 Enterprise 事件没有统一阶段时间戳 | Tool/Execution contracts 对比 `EnterpriseAgentEvent` | 若要活动耗时分解，需要在组合根统一计时 |
| `ContextReducer` 已能读取 Context segments | reducer trait | “暴露上下文访问信息”不必等同于加载任意插件；需要明确是 SPI 还是持久访问历史 |
| Session 消息可从现有 SQLite 恢复 | `EnterpriseAgent::sessions()` 和 Session API | Session 列表不需要新业务表 |
| 最近项目只能安全地使用单活动 Runtime | Desktop operation lock/approval broker | 继续采用列表切换，不引入后台并发 |
| UI preference 的第二次写入当前可能因缺少 expected version 冲突 | Preference CAS store 与调用方 | 主题/i18n/最近项目实现前必须修正 CAS 调用 |

## Critical Unknowns

优先级使用 `Impact × Probability × Irreversibility × Late discovery cost`。

| Unknown | Category | Evidence / Reasoning | I | P | R | L | Score | Disposition | Recommended resolution |
|---|---|---|---:|---:|---:|---:|---:|---|---|
| API Key 由页面以何种方式持久化 | Known unknown | P029 允许 plaintext，但设置页扩大了写入面；env ref 无法由页面完整托管，系统密钥库会扩大跨平台范围 | 5 | 5 | 4 | 5 | 500 | Blocker | 用户确认；最低改动是沿用 plaintext + 严格脱敏/文件保护，也保留 `apiKeyRef` 高级方式 |
| “最大上下文”最终按 128K Token 还是 128 KiB 字节 | Known unknown | 用户已接受默认 128K，但回复仍写作 128KB；现有模型硬限制只认 Token，Context 文件预算才认 bytes | 5 | 4 | 4 | 5 | 400 | Blocker | 用户确认单位；推荐沿用现有模型合同的 128K Token，文件注入字节预算继续使用独立 Context 配置 |
| 多模型选择是全局 active、每 Session 固定还是自动路由 | Known unknown | 只定义了多个唯一 name，没有定义一次请求选择哪个 | 5 | 5 | 4 | 5 | 500 | Blocker | 推荐全局 `activeModel`，Desktop 切换后重建活动 Runtime；Terminal 下次请求/启动读取同一值 |
| 配置页面写哪个层级 | Known unknown | 项目/环境优先于用户文件；写用户文件后可能仍被高优先级覆盖 | 5 | 4 | 4 | 5 | 400 | Blocker | 推荐只写全局用户文件；项目/env 只读显示为“覆盖来源”，不由页面静默修改 |
| “暴露上下文访问信息”是扩展 SPI、访问历史，还是动态插件 | Known unknown | 现有 Reducer 已拿到 segments；访问频率需新持久化，动态代码有执行安全问题 | 5 | 5 | 4 | 5 | 500 | Blocker | 推荐内置策略 + Rust 注册 SPI + 非敏感 snapshot/observer；不做任意动态插件或持久访问画像 |
| 统计是否必须跨 Terminal/Desktop/项目全局汇总 | Known unknown | 需求强调同名访问消耗且配置全局，但当前 DB 项目隔离 | 5 | 5 | 4 | 5 | 500 | Blocker | 推荐用户级本地观测库，按实际 model name 全局汇总，并保留 workspace/session 维度过滤 |
| 请求耗时是否包含人工审批等待 | Known unknown | 用户体感耗时包含等待，但性能分析通常要排除等待 | 5 | 5 | 3 | 5 | 375 | Blocker | 推荐同时记录 `wall_duration_ms` 与 `active_duration_ms`，主界面实时显示 wall time，tooltip 给出已知阶段 |
| 旧 schema v1 如何迁移且能回滚 | Known unknown | 写入器一旦把单 model 重写为 models 列表，旧 binary 无法读取 v2 | 5 | 4 | 4 | 5 | 400 | Decision | 读取兼容 v1/v2；首次保存升级 v2；写前校验+CAS+同目录临时文件，失败保持原文件；文档声明旧版回退限制 |
| 配置重写是否必须保留 YAML 注释和字段顺序 | Unknown known | serde 重写会规范化并丢注释；完整 round-trip writer 会显著增大范围 | 4 | 4 | 3 | 4 | 192 | Decision | 推荐结构化、确定性重写并在设置页明确提示；不改未知受支持字段，写前提供差异预览/冲突错误 |
| 请求在崩溃/强退时如何终止 | Unknown unknown candidate | 开始记录后进程可能未写 completion | 4 | 4 | 3 | 4 | 192 | Decision | 启动时把本进程遗留 RUNNING 标记为 INTERRUPTED；保留 started_at 和已知 elapsed，不伪造成功 |
| Usage 持久化失败是否使已成功模型请求失败 | Prior known unknown | P2 已决定 successful inference fail-open，避免重复计费 | 5 | 3 | 3 | 5 | 225 | Decision | 保持既有决定；UI 显示统计不完整标记，不能重试已经成功的 Provider 请求 |
| Context 圆环显示上一轮还是发送前实时预测 | Known unknown | 真正 Context 只有 build 后存在；输入中只能估算 | 4 | 5 | 2 | 4 | 160 | Decision | 显示“最近快照 + 当前输入增量估算”；发送完成后以新 snapshot 校正，tooltip 标记估算 |
| 多模型同名在大小写/Unicode 下如何唯一 | Unknown unknown candidate | 文件配置和 Provider 名称跨平台；简单字符串相等容易产生视觉重复 | 4 | 3 | 3 | 4 | 144 | Decision | `trim` 后按 Unicode 原值区分但禁止控制字符；建议 key 使用 ASCII-safe 归一化，UI 对大小写近似冲突给错误 |
| 全局 SQLite 被 Terminal/Desktop 同时写入 | Unknown unknown candidate | 当前 store 未显式设置 WAL/busy timeout | 5 | 4 | 3 | 5 | 300 | Decision | 全局观测库启用 WAL、busy timeout、有界重试和事务；并发 E2E 必须覆盖 |
| 统计日历的日期时区和失败请求口径 | Unknown known | UTC 存储，本地日历可能跨日；失败调用也可能产生费用 | 3 | 4 | 2 | 3 | 72 | Accept | UTC 持久化、按设备本地日历分组；Token 图包含成功及 Provider 已返回 Usage 的失败调用，状态可筛选 |
| 自动清理统计历史 | Unknown unknown candidate | 长期本地数据会增长，但自动删除不可逆 | 3 | 3 | 4 | 3 | 108 | Defer | 本期不自动删除；分页/聚合查询，后续增加显式保留策略和清除确认 |
| 权限选择作用域 | Existing blocker | Desktop 切换若写共享配置会改变 Terminal；临时模式重启不保留 | 4 | 4 | 3 | 4 | 192 | Blocker | 推荐只覆盖当前活动 Runtime，不改共享文件；切换工作区/重启回到配置值 |
| 前端图表库是否新增 | Unknown known | 当前依赖中无图表包；简单日历/堆叠 SVG 可原生实现 | 2 | 4 | 1 | 2 | 16 | Accept | 使用 Vue + 原生 SVG/CSS，不引入大依赖 |

## Competing Solution Models

### Model A — 推荐：共享配置 + 共享本地观测 + 单活动 Runtime

- schema v2 使用 `models[] + activeModel`，页面只写用户级配置；项目/env 覆盖只展示来源。
- Terminal/Desktop 每次构造 Runtime 都从同一解析器取 effective model/压缩配置；Desktop 保存后原子重建当前 Runtime。
- 项目 Session/Context/Workspace DB 继续隔离；模型调用 Usage 和请求级指标写入用户级本地观测库。
- 请求同时记录 wall/active；界面本地 timer 实时显示，完成后以后端终值校正。
- 压缩只允许内置策略或进程内注册的 reducer，不加载任意代码。

**优点：** 满足跨入口一致性，保留项目数据隔离，修改边界清楚；可逐步扩展。  
**代价：** 需要 config v2、原子 writer、共享 SQLite 并发、请求级观测服务和 Runtime 重建。

### Model B — 每项目配置与统计

- Desktop 修改当前项目 `.agent/config.*`；Usage/耗时继续留在项目 Runtime。
- 多模型和压缩只对项目生效。

**优点：** 复用当前数据目录，代码较少。  
**缺点：** 与“Terminal/Desktop 同一配置”和“每个 name 的整体消耗”冲突；切换项目会看到不同配置和碎片统计。  
**结论：** 不推荐。

### Model C — 动态插件 + 全局后台服务

- 独立 daemon 管配置、密钥、模型路由、统计和第三方压缩插件；Terminal/Desktop 均 RPC 访问。

**优点：** 真正热更新和跨进程统一。  
**缺点：** 引入服务生命周期、鉴权、插件沙箱、迁移和离线故障，远超 P033。  
**结论：** 本期不实施。

## Candidate Contracts to Freeze After Confirmation

### Shared configuration v2

```yaml
version: 2
activeModel: deepseek-v4
models:
  - name: deepseek-v4
    baseURL: https://api.deepseek.com
    apiKey: "..."
    maxContextTokens: 128000
context:
  compression:
    strategy: recent-window
    triggerPercent: 80
    keepRecentMessages: 20
```

- `name` 是用户可见唯一标识；推荐同时作为 OpenAI-compatible wire model name 和稳定 profile key。
- 模型最大上下文推荐只配置 `maxContextTokens`，默认 `128000`；现有 `context.maxTotalBytes` 继续只限制 `@file/@folder` 注入，不与模型窗口混为一个字段。
- `activeModel` 必须引用一个已配置且有效的 `name`。
- `apiKey` 与 `apiKeyRef` 互斥；任何读取 API 都只返回 `apiKeyConfigured`。
- v1 `model` 读取时映射为单元素 `models`；只有实际保存才升级磁盘版本。
- 配置写入采用：定位用户源 → 读取 fingerprint/version → 合并受支持字段 → 完整校验 → 同目录临时文件 → flush/atomic replace；发现源变化则返回冲突，不覆盖。
- `CORE_AGENT_CONFIG` 指向显式文件时可读取；是否允许页面写显式路径随“配置层级”确认结果冻结。

### User-level request observation

建议一张请求表和复用/迁移后的模型调用 Usage 表。所有新表遵守项目 DB 规范：`id/create_time/update_time/create_user/update_user`、表/字段注释、合适索引、无外键。

| Entity | Minimum fields | Purpose |
|---|---|---|
| `agent_request_metric` | request_id, workspace_key, session_id, entrypoint, model_name, started_at, completed_at, wall_duration_ms, active_duration_ms, approval_wait_ms, context_duration_ms, model_duration_ms, tool_duration_ms, context_tokens, status, error_kind + audit fields | 一次用户请求的真实边界、实时校正和历史耗时 |
| `model_usage` | existing request_id/provider/model/profile/prompt/completion/cache/total/latency/cost/success/error + audit fields | 每次实际模型调用的权威 Provider Usage |

- 无数据库外键；`request_id` 作为逻辑关联并建立索引。
- `request_id`、`model_name + created_at`、`workspace_key + created_at`、`status + created_at` 建索引。
- 不保存 prompt、响应正文、API Key、工具参数或完整路径；workspace 使用稳定 hash/key。
- Calendar 默认按本地日历日聚合 `prompt/completion`；趋势图按实际响应 `model_name` 堆叠；请求耗时图与 Token 图分开，避免把 Token 当费用。

### Context/compression extension boundary

- 页面只配置 registry 中存在的策略 key、触发阈值、最近消息数和 slot budget。
- 内置首期策略：`recent-window`（确定性窗口裁剪）和 `extractive-summary`（现有确定性摘要能力显式启用）；不虚构 AI summary。
- `ContextReducer` 继续是进程内 Rust trait；新增稳定只读 `ContextAccessSnapshot`/observer，只含 segment id/type、token estimate、slot、last included/decision 和阶段耗时，不含 API Key。
- 若确认需要访问历史，再设计 bounded persistence、清理与隐私开关；不把它偷偷塞进 Usage 表。

### Request timing semantics

- `wall_duration_ms = terminal_time - accepted_time`，包含模型、工具和人工审批等待，代表用户体感。
- `active_duration_ms` 排除可识别的审批等待；阶段之和小于等于 wall，无法归属部分列为 orchestration。
- Terminal/Desktop 从本地提交时立刻启动 elapsed timer；后端受理后用统一 `request_id` 关联，终态以后端单调时钟计算值校正。
- 持久化使用 UTC wall timestamps；duration 使用 monotonic clock，避免系统时间跳变。
- 本地零模型命令默认也算一次交互，但应标记 `LOCAL_COMMAND` 且 Token 为 0；是否纳入“模型消耗”图由查询过滤。

## Implicit Expectations

| Expectation | Why it may exist | How to surface it |
|---|---|---|
| 保存设置后 Terminal 确实能读到 | “同一个配置文件”不仅是 UI 展示 | Desktop 保存 → 新建 Terminal Runtime → effective config 等值 E2E |
| 多模型不是重复表单，而是可选择且统计不串名 | 唯一 name 和按 name 统计 | duplicate/rename/active reference/实际响应模型聚合测试 |
| 圆环不能在第一轮显示虚假的精确值 | Context 只有构建后才真实存在 | `estimated` 标签、首轮空态和 build 后校正测试 |
| 请求“实时耗时”在审批对话时仍继续增长 | 用户仍在等待本次请求 | TUI/Desktop fake clock + approval flow 断言 |
| 统计失败不能导致已付费请求自动重试 | 既有 P2 不变量 | 注入观测存储失败，断言响应保留且 UI 标记缺失 |
| 删除 active 模型必须被阻止或先选择替代 | 唯一 active reference | 配置领域断言和 UI 禁用/替换流程 |
| API Key 编辑后不能从前端读取原值 | 原生 IPC 也属于暴露面 | Tauri contract 快照和日志扫描 |

## Red-team Review

| Attack / failure | Consequence | Required control |
|---|---|---|
| 恶意 symlink 把用户配置指向其他文件 | Desktop 保存覆盖任意文件 | 写前后 `symlink_metadata`、同目录临时文件、regular-file 校验、atomic replace fail-closed |
| 两个进程同时保存配置 | 后保存者静默丢失前者修改 | fingerprint/CAS；冲突提示重新加载，不做 last-write-wins |
| API Key 出现在 Vue state、错误、backup 或 telemetry | 本地凭据泄露 | 后端脱敏 DTO；输入只写不回显；错误扫描；临时文件权限；不生成长期明文 backup |
| 模型 rename 后历史 Usage 消失或串到新模型 | 统计错误 | 历史按实际 wire/profile name 不可变记录；rename 是新 identity 或显式迁移，不级联覆盖历史 |
| Provider 不返回 Usage | 图表把未知当 0 | 保存 `usage_available=false`/缺失状态，UI 显示未知，不伪造 Token |
| 系统时间回拨 | duration 负数或跳变 | 单调时钟计算 duration，UTC 只做展示和分组 |
| 进程崩溃留下 RUNNING | 日历永远显示进行中 | startup recovery 标记 INTERRUPTED，保留开始时间和已知字段 |
| Context observer 暴露 segment 正文 | 项目源码/密钥泄露 | 默认只暴露 metadata/token/decision；内容访问仅在受信任 reducer 进程内 |
| 动态压缩策略执行任意代码 | 代码执行和数据外泄 | 本期不加载外部二进制/脚本；registry 未知 key fail-closed |
| 全局 DB 锁阻塞模型响应 | 用户请求卡住或错误重试 | WAL/busy timeout；观测失败 fail-open；异步有界写入并显式缺失标记 |
| 快速切项目后旧请求终态写入新 UI | Session/耗时串项目 | generation token + request/workspace identity；切换时禁用冲突操作并丢弃过期回写 |

## Decisions Required

**Status: confirmed 2026-07-19.** 用户选择全部推荐项：plaintext + `apiKeyRef` 高级方式、128K Token、全局 active model、仅写用户配置、built-ins + Rust SPI/content-free metadata、全局 Terminal/Desktop/project Usage + wall/active、权限仅当前 Runtime、版本 `0.3.0`。

| Decision | Options | Trade-offs | Owner | Deadline |
|---|---|---|---|---|
| API Key 保存 | plaintext / env ref / OS keychain ref | 开箱体验、安全、跨平台成本 | User/Security | 实现前 |
| Context 容量单位 | bytes / tokens / dual | 需求字面与模型真实限制 | User/Product | 实现前 |
| 模型选择作用域 | global active / per session / auto route | 一致性、历史可复现、路由复杂度 | User/Product | 实现前 |
| 配置写入层 | user file / effective writable source | 可预测性与项目覆盖便利 | User/Architecture | 实现前 |
| 压缩扩展层级 | built-ins + SPI / persisted access history / dynamic plugins | 可扩展性、隐私、安全和工期 | User/Architecture | 实现前 |
| 统计与耗时 | global + wall/active / project + wall / model-call only | 是否满足跨入口、用户体感与性能分析 | User/Product | 实现前 |
| 权限作用域 | temporary / global config / project config | 安全、持久化和 Terminal 一致性 | User/Security | 实现前 |
| Release version | `0.3.0` / `0.2.1` / Unreleased only | 重大功能或补丁语义 | User | CHANGELOG 更新前 |

## Safe Assumptions

| Assumption | Why it is safe | Reversal plan |
|---|---|---|
| 多工作区采用最近项目列表 + 单活动 Runtime | 符合“方便切换”，保持现有锁、审批和资源边界 | 后续独立设计多 Runtime 后台任务 |
| 高级 Workspace 和 Trace/Changes/Execution 保留，只从主 Console 移除 | 不破坏已发布能力 | 后续可重新分组入口 |
| `name` 默认同时作为唯一配置 key、profile key 和 wire model name | 用户只要求一个 name，OpenAI-compatible 路径最小 | schema 未来可新增 `displayName/modelId`，历史 key 不变 |
| 图表使用原生 SVG/CSS | 无新依赖、可测试、满足展示 | 后续替换图表库不改 API |
| Token 日历默认最近一年可导航，但不自动删数据 | 避免不可逆清理 | 增加显式 retention 设置 |
| 静态 Desktop UI 全部 i18n；动态内容保持原文 | 避免篡改技术与审计文本 | 后续加显示层翻译 |
| Session 首期只新建/选择/恢复，不删除或重命名 | 满足主链且无不可逆操作 | 独立增加回收站/重命名合同 |
| 文件 icon 用现有 Lucide/内联 SVG registry | 符合 SVG 要求且无需依赖 | 可替换 icon pack |
| 配置保存后 Desktop 重建当前 Runtime，不承诺无重启热换所有在途请求 | 当前 Runtime 构造时冻结模型/权限 | 后续引入 config snapshot hot reload |

## Deferred Unknowns

| Unknown | Why deferred | Monitoring / Follow-up |
|---|---|---|
| OS vault 的跨平台统一实现 | 取决于 API Key 决策且涉及平台 credential provider | 若选择 keychain，拆成明确安全子设计后再实现 |
| 任意第三方动态压缩插件 | 需要签名、权限、沙箱和版本兼容 | 依托 Extension Runtime 单独设计 |
| Context segment 访问频率长期画像 | 需要隐私、稳定 identity、容量和清理语义 | 只有用户确认需要才进入 P033 |
| 多窗口/多 Runtime 后台并发 | 扩大审批、锁、计时和恢复模型 | 单活动模式验证不足时另立设计 |
| 云端 Usage/成本账单 | 需要身份、币种和远端一致性 | 保持本地 Provider Usage，不声称 Billing |
| 实际 tokenizer 插件 | 当前 TokenCounter 是估算器 | 后续按模型 profile 注册 tokenizer |
| 统计数据清除/导出 | 涉及不可逆删除与隐私导出 | 独立增加确认、范围和审计 |

## Recommended Implementation Boundary

### Implement now after confirmation

- config schema v2、多模型领域校验、v1 兼容读取、用户配置原子 CAS writer、脱敏 Tauri API。
- 全局 active model，Desktop 模型 CRUD/选择与覆盖来源提示；Terminal/Desktop 共用解析结果。
- 用户级本地请求观测/模型 Usage 查询，WAL 并发、按模型/日期聚合、Calendar/趋势图。
- `EnterpriseAgent` 请求级 request_id、终态、wall/active/阶段计时；Terminal/Desktop 实时 elapsed 展示和最终校正。
- Context 占用 DTO、输入区圆环/tooltip、内置 compression registry 与配置传递；按确认的扩展边界公开 metadata。
- 四区 Desktop、最近项目/Session 历史、右侧文件树与常见 SVG icon、共享 `+`/`/`、临时权限选择器。
- light/dark、`zh-CN`/`en`、Preference CAS 修复。
- README、CHANGELOG、P033 Implementation Notes、Post-Implementation Review。

### Do not implement now

- 云同步/远端统计、Billing、动态插件执行、多 Runtime 并发、Session 删除/重命名、自动翻译动态数据。
- 自动迁移或覆盖项目/env 高优先级配置，除非用户明确选择该方案。
- 把 API Key、prompt/response、工具参数或 Context 正文写入统计库。

### Areas that must remain reversible

- v1 配置保持只读兼容；v2 writer 独立，可禁用页面保存而继续读取。
- 项目 Runtime DB 和全局观测 DB 分离；禁用统计不影响 Session/模型请求成功。
- 新布局组件化；高级 Workspace 不删除。
- 主题、语言、最近项目和临时权限偏好可删除恢复默认。
- compression strategy 通过 registry key 选择，未知 key fail-closed，默认可退回 `recent-window`。

## Verification Plan

### Automated

- **Config unit assertions:** v1→v2 映射、唯一 name、active 引用、URL/容量/API Key/ref、unknown strategy、redaction、deterministic serialization、CAS conflict、symlink/oversize/partial-write failure。
- **Model/telemetry unit assertions:** request aggregation、实际响应 model name、缺失 Usage、wall/active/approval 算法、clock rollback immunity、interrupted recovery、timezone grouping。
- **SQLite integration:** 所有 audit 字段/注释/索引/无 FK；WAL；Terminal/Desktop 两连接并发写；重开/迁移/损坏失败；分页/日历/趋势聚合。
- **Context unit assertions:** 策略注册、配置真正到达 reducer、阈值、slot distribution、圆环百分比、估算/校正、敏感 metadata 不泄露。
- **Enterprise E2E:** 一个请求多模型轮次/工具/审批，聚合 Token 与 wall/active 终值正确；成功响应遇统计写失败仍返回并标记缺失。
- **Terminal E2E/TTY assertions:** fake clock 下 spinner 旁 elapsed 增长；审批期间继续增长；成功/失败/取消终态；非 TTY 输出保持兼容。
- **Desktop Vue/App E2E:** 项目→Session→历史→`+`/`/`→模型设置→统计→Context 圆环→权限→主题/i18n；原生边界 deterministic fake。
- **Tauri Rust E2E:** 同一配置 Desktop 保存后由 CLI resolver 读取；密钥不回传；Runtime 重建；旧请求不污染新 workspace。
- **Static/final gate:** `npm test`、`vue-tsc --noEmit`、Vite build、`cargo test --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`git diff --check`。

### Manual

- 添加两个项目并往返切换，恢复历史 Session 后继续对话。
- 配置两个真实 OpenAI-compatible 模型、切换 active、分别请求，确认日历按实际 name 分开。
- Terminal 与 Desktop 同时运行，确认配置冲突提示和 Usage 并发写不丢失。
- 人工审批停留数秒，确认两端 elapsed 持续增长且最终 wall ≥ active。
- 达到压缩阈值，确认 reducer 事件、圆环和 tooltip 一致；切换策略后下一请求生效。
- 检查亮/暗、中文/英文、窄窗口、键盘导航、focus-visible 和 tooltip accessible name。
- 搜索构建产物/日志/IPC snapshot，确认没有 API Key 或配置临时文件残留。

### Three-pass review required after implementation

1. **Correctness review:** schema/迁移、请求边界、统计聚合、Context/压缩和项目/Session 竞态。
2. **Security/compatibility review:** API Key、路径/symlink、配置并发、旧 v1/CLI、全局 DB、多入口 fail-open/fail-closed。
3. **UX/performance review:** 主对话注意力、实时计时、图表空态/大数据、圆环估算语义、i18n/亮色/响应式；只做手术式小优化。

## Rollback and Recovery

- **Config:** 保存前完整校验和 fingerprint；失败不替换原文件。v2 保存后旧 binary 不兼容，因此正式升级前必须明确版本并在 README 写明；可由用户从 v2 单模型导出 v1，但不会静默降级丢模型。
- **Runtime:** Desktop 保存配置时等待/拒绝在途冲突操作，再原子替换活动 Runtime；失败保留旧 Runtime 和旧 effective config。
- **Telemetry:** 观测库不可用不隐藏已成功的模型响应；UI 标记“统计未记录”。schema migration 使用事务；损坏库不触碰项目 Session DB。
- **Request:** 崩溃遗留 RUNNING 在下次打开时标记 INTERRUPTED；不补写虚假的 completion/token。
- **UI:** 新布局组件和偏好可单独回退；高级 Workspace 和旧项目数据库不删除。

## Handoff

- [x] Updated intent and success criteria
- [x] Evidence-backed known facts
- [x] Critical unknowns ranked
- [x] Competing solution models
- [x] Security red-team review
- [x] Candidate config/data/API contracts
- [x] Verification, rollback and recovery requirements
- [x] Non-goals and deferred scope
- [x] User decisions below confirmed
- [x] Implementation notes started
- [ ] Real implementation and four-step final verification
