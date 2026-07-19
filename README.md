# core-agent

## v0.3.0：统一模型配置与请求观测

Terminal 与 Desktop 现在读取同一个用户配置文件。设置页可维护多个 OpenAI-compatible 模型，`name` 不区分大小写唯一，并选择一个全局 `activeModel`；每个模型单独配置 `baseURL`、API Key 和最大上下文，默认窗口为 **128K Token**。旧版 `version: 1` 单模型配置仍可读取，第一次从 Desktop 保存时原子迁移为 `version: 2`。

Desktop 采用对话优先的四区布局，支持最近项目、历史会话、右侧文件上下文、亮/暗主题与中英文切换。输入区圆环展示当前 Context 占用，设置页提供压缩策略、Token 消耗日历、输入/输出趋势和最近请求耗时。Terminal 与 Desktop 都会实时显示本次请求 wall time；审批等待会从 active time 中单独扣除。观测数据只保存模型/Token/阶段耗时等无正文元数据，不保存 prompt、response、工具参数或 API Key。

推荐配置参见 [core-agent-config.example.yaml](core-agent-config.example.yaml)。API Key 可以明文写入受保护的本地用户配置，也可使用 `apiKeyRef: env:变量名`；Desktop 不回显已保存的密钥。模型设置保存采用 fingerprint/CAS 与同目录原子替换，若 Terminal 或其他窗口已修改文件，会要求重新加载而不会静默覆盖。

core-agent 是一个单进程、模块化的企业级 Agent Runtime。用户只需要选择 Terminal 或桌面端；Session、Context、Model、Tool、Workspace、Planning、Execution、Memory 等 Runtime 都由同一个 `EnterpriseAgent` 在进程内组合和管理，不需要逐个启动子服务。

```text
Terminal ─┐
          ├─ EnterpriseAgent（单进程）─ Session → Context → Model → Tool
Desktop ──┘                         └─ 其余 Runtime 内部模块
```

## 一次配置，到处使用

复制 [core-agent-config.example.yaml](core-agent-config.example.yaml) 到用户目录，只替换 `apiKey` 即可：

- Windows：`C:\Users\<用户名>\core-agent\core-agent-config.yaml`
- Linux/macOS：`~/core-agent/core-agent-config.yaml`

推荐配置默认使用 `deepseek-v4-flash`、`risk-based` 权限、项目 Memory、新 chat 新 session，并限制 `@` 上下文的数量、目录深度和总体积。也支持同目录的 `.yml` 或 `.json`，但同一级只能存在一种格式；`CORE_AGENT_CONFIG` 可显式选择文件，`CORE_AGENT_HOME` 可覆盖配置目录。

配置来源按 `builtin < user file < project file < environment` 合并。核心只依赖强类型 `AgentConfig`；YAML、JSON、环境变量和密钥引用都是可替换的 `ConfigProvider`/`SecretResolver` 策略，未来切换远程配置或系统凭据库不会改变 `EnterpriseAgent`。

可以直接写 `apiKey` 获得最短启动路径，也可以使用 `apiKeyRef: env:CORE_AGENT_API_KEY` 避免明文。配置与 Debug 输出始终脱敏；请限制用户配置文件权限并勿提交密钥。仓库已忽略 `core-agent-config.yaml|yml|json`、`.agent/` 与 `*.dpapi`。若密钥曾出现在聊天、日志或终端历史中，应在 Provider 控制台轮换。

## 快速体验：Terminal

准备 Rust 1.94+。打开任意项目目录即可运行，不需要先执行 `init`：

```powershell
cargo run -p agent-cli --bin agent -- run "分析当前项目并给出下一步建议"
cargo run -p agent-cli --bin agent -- chat
```

`agent chat` 在真实交互终端中启动全屏 TUI：品牌/项目状态区、滚动会话区、带边框输入框、`/` 命令面板、`@` 工作区文件/文件夹候选、忙碌状态和风险审批卡片集中在同一个布局。工作区打开时预建最多 20,000 文件的 git-aware 安全索引；`@` 后至少输入 3 个字符才在内存中模糊过滤，因此大项目不会每按键重复扫描磁盘。`↑/↓` 选择候选，`Tab/Enter` 只补全并继续输入，再次 Enter 才发送；`Alt+Enter` 换行、`Ctrl+Shift+C` 复制最近 Agent/错误消息、`PgUp/PgDn` 滚动、`Ctrl+D` 退出。非 TTY、`run` 等脚本入口和 `--no-color` 继续使用稳定纯文本输出。

CLI 直接在当前进程加载全部 Runtime，不需要启动 Agent Server。TUI 是 Runtime-thin 视觉适配层：命令候选来自核心 `InteractionCommandRegistry`，`@` 最终解析仍由核心 Context resolver 完成，审批通过 `EnterpriseApprovalHandler` 回到同一权限引擎。会话和 Runtime 数据保存在项目 `.agent/`，全局模型密钥不复制进项目。`agent init` 仍可选：它只生成 `server/workspace` 项目覆盖、Context 和 Memory 目录，不再重复模型配置。

常用命令：

```powershell
cargo run -p agent-cli --bin agent -- sessions
cargo run -p agent-cli --bin agent -- status
cargo run -p agent-cli --bin agent -- tools
cargo run -p agent-cli --bin agent -- project
```

交互式 `chat` 默认创建新 session，同一次 chat 的后续消息持续使用它；`session.resumeLast: true` 可恢复最近 session。常用内置命令包括 `/help`、`/new`、`/clear`、`/sessions`、`/status`、`/tools`、`/config`、`/plan`、`/review`、`/test`、`/fix`、`/undo` 和 `/redo`。命令定义、解析、路由及 Agent Prompt 展开来自核心注册表，Terminal 与 Desktop 不维护两套实现。

使用 `@` 显式补充文件或文件夹上下文：

```text
解释 @README.md 的启动流程
对照 @"design docs/spec.md" 检查 @src 文件夹
/plan 根据 @design-docs 和 @Cargo.toml 制定迁移方案
```

文件夹按稳定顺序递归展开普通 UTF-8 文件。所有入口复用核心 resolver 与工作区策略：禁止越界、敏感文件、`.git`、`.agent`、依赖/构建目录和符号链接，并执行 mention 数、文件数、深度、单文件与总字节上限。Session 只保存用户原始输入，解析出的正文只进入本轮 Context 快照。

如需连接兼容的远程部署，可在 `.agent/config.yaml` 中显式设置 `server.mode: remote` 和 `server.url`；这不是本地使用的前置条件。

## 快速体验：桌面端

准备 Node.js 20+、Rust 1.94+ 和 Tauri 2 的系统依赖，然后执行：

```powershell
cd agent-desktop
npm install
npm run tauri dev
```

桌面端在 Tauri 进程中直接持有同一个 `EnterpriseAgent`，无需另开 Terminal 或后台 Agent 服务。点击顶部 `Open folder` 即可选择项目；应用按新目录重新解析配置，并按规范化工作区路径的 SHA-256 将 Runtime 数据隔离到应用数据目录的 `projects/<hash>/runtime`。Console 与 Terminal 共用 `/` 命令注册表和预索引 `@file`/`@folder` 模糊查询：至少输入 3 个字符后出现候选，Enter/Tab 只补全，Shift+Enter 换行。项目树可用 `Add @` 加入选中路径，用户消息和 Agent 回复都可显式复制；设置页显示权限模式与脱敏配置来源。

可使用以下环境变量作为最高优先级覆盖：

- `CORE_AGENT_WORKSPACE`
- `CORE_AGENT_MODEL_PROVIDER`
- `CORE_AGENT_MODEL_ENDPOINT`
- `CORE_AGENT_MODEL`
- `CORE_AGENT_MODEL_PROFILE`
- `CORE_AGENT_MODEL_MAX_CONTEXT_TOKENS`
- `CORE_AGENT_API_KEY`
- `CORE_AGENT_PERMISSION_MODE`（`strict` / `risk-based` / `auto`）

需要人工审批时，桌面端会显示工具名、风险等级、原因和参数，并只接受“本次允许”或“拒绝”；等待五分钟没有决定时自动拒绝。

## AGENTS、Skills 与持久化 Memory

Runtime 会确定性加载用户目录和项目路径上的 `AGENTS.override.md`（优先）或 `AGENTS.md`，按用户 → 项目根 → 当前工作目录顺序合并。规则文件必须是有界 UTF-8 普通文件，符号链接、越界和超限会 fail-closed；来源、优先级、大小与 SHA-256 会进入本轮无正文观测。

Skills 从 `~/core-agent/skills`、项目 `.agents/skills` 和 `.codex/skills` 发现。系统 Prompt 只常驻 `name/description`；模型需要某项能力时调用 `load_skill`，Runtime 才校验哈希并加载完整 `SKILL.md`。Skill 不会绕过 Tool/Permission 静默执行脚本。

Memory 使用独立 `memory.db` 持久化 project/session namespace。每轮只召回与当前问题相关的有界条目；写入必须显式调用 `remember_memory` 并经过权限，`recall_memory` 可查询，`forget_memory` 使用版本检查并清除正文/索引。疑似 API Key、Token、密码或私钥的内容拒绝进入持久化 Memory；`AGENTS.md` 规则也不会降级保存为可遗忘 Memory。

## 工作区工具与审批模式

模型通过同一条有界工具循环使用 `list_files/read_file/write_file`、Glob 风格 `find_files`、正则 `search_files`、精确增量 `apply_patch` 和 `run_command`。文件路径必须留在已打开的工作区；`.git`、`.agent`、`.env*`、凭据、私钥、构建目录和依赖目录不可读取或修改。写入和补丁携带当前 SHA-256，歧义替换与并发变更会拒绝，并继续进入 session Checkpoint。

`run_command` 分离 stdout/stderr，返回 exit code、超时/取消/截断状态，限制 120 秒和合计 1 MiB 输出，并从子进程环境剥离常见 Key/Token/Password。`start_command/poll_command/cancel_command` 提供最多 64 个有界后台任务；超时/取消在 Windows 使用 `taskkill /T`，Unix 使用进程组终止。`delegate_task` 可创建最多四轮的独立模型上下文，但只开放默认允许的读取类工具，不能修改文件或启动进程。

Linux 检测到 `bubblewrap` 时可对命令启用只读系统、工作区可写和默认断网的 OS sandbox；其他平台保持如实的 `none` capability。设置 `CORE_AGENT_REQUIRE_OS_SANDBOX=1` 可在无 backend 时强制拒绝命令。路径检查、审批和 `auto` 模式都不等价于 OS sandbox。

## 联网搜索、Hooks、MCP 与集中策略

配置 `CORE_AGENT_WEB_SEARCH_API_KEY`（或 `OPENAI_API_KEY`）后，Runtime 注册 OpenAI Responses `web_search` 与 `web_fetch`；可用 `CORE_AGENT_WEB_SEARCH_MODEL` 选择模型。搜索结果包含来源 URL，回答应引用这些来源；Fetch 仅允许公开 HTTPS 文本内容，逐跳重验重定向和 DNS，拒绝 localhost、私网、保留地址和云元数据地址，正文最多 1 MiB。没有 Key 时工具不会伪装可用。

Hooks 与 MCP 默认不会因打开仓库而执行。显式设置 `CORE_AGENT_ENABLE_HOOKS=1` 后读取 `~/core-agent/hooks.json` 和项目 `.core-agent/hooks.json`；Hook 通过统一 CommandRunner 接收 JSON stdin，可配置 `fail_open/fail_closed`。设置 `CORE_AGENT_ENABLE_MCP=1` 后读取同目录 `mcp.json`，支持 stdio initialize、`tools/list`、分页和 `tools/call`；server 环境默认脱敏，只有 `envVars` 点名的变量会重新继承，远程 Tool 仍逐次经过审批。

`CORE_AGENT_MANAGED_POLICY` 可指向不可经项目覆盖的 JSON 策略，集中 deny/allow Tool 或 category、限制 MCP server 和 Web domain、关闭 Memory 写入/Hooks/Web，或要求 OS sandbox。deny 始终优先于本地模式和允许规则。

| 模式 | 人工审核 | 适用场景 |
|---|---|---|
| `strict` | 每次编辑、执行命令前都审核 | 安全优先、陌生代码库 |
| `risk-based`（默认） | 编辑、执行项目代码及高风险命令审核；极少数只读命令自动通过 | 日常开发的安全/效率平衡 |
| `auto` | 软审批自动通过；越界路径、敏感文件和明确破坏性命令仍拒绝 | 可信临时工作区、无人值守任务 |

Terminal TUI 在会话内显示审批卡片（工具、风险、原因和参数），`Enter/Y` 仅允许本次，`N/Esc` 拒绝；纯文本交互使用 `[y/N]`，管道、CI 等非交互输入默认拒绝。`auto` 不是 OS 沙箱：对不完全可信的仓库，优先使用 `strict` 或 `risk-based`，并在容器/虚拟机中运行。

`/plan`、`/review`、`/explain`、`/commit`、`/pr` 由 Runtime 强制只读：模型看不到写工具，臆造的写调用或副作用命令也会在审批前拒绝。每轮通过 `write_file` 的修改会形成 session 级文件 Checkpoint；`/undo`、`/redo` 不依赖 Git，并在当前内容 SHA-256 仍匹配时整组恢复。用户后续手工编辑会触发冲突并拒绝覆盖；shell、网络、部署等副作用不在文件 Checkpoint 的回退范围内。

## 架构边界

各 `core-agent-*` 目录是 Rust workspace crate，用于隔离领域和依赖边界；它们不是要求用户单独运行的进程，也不是散落的子 Agent。统一组合入口位于 `src/enterprise.rs`，Terminal 和 Desktop 只负责输入输出。

当前主链真实执行：Session 持久化 → Context 构建/快照 → Model Provider → 有界 Tool 循环 → 人工审批 → 消息与事件落库。Planning、Execution、Agent、Workflow、Multi-Agent、Platform、Visual、Collaboration、Governance、Ecosystem 和 Protocol 由同一组合根实例化，并通过内部适配器连接。

## 验证

```powershell
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings

cd agent-desktop
npm test
npm run build
```

真实云模型测试默认忽略，避免 CI 意外产生费用。Runtime 级测试使用环境变量；Terminal 真实启动测试直接验证用户全局配置：

```powershell
cargo test -p core-agent --test live_deepseek_e2e -- --ignored --nocapture
cargo test -p agent-cli --test live_global_config_e2e -- --ignored --nocapture
```

设计到实现的审计矩阵见 [design-docs/capability-traceability.md](design-docs/capability-traceability.md)。
