# core-agent

core-agent 是一个单进程、模块化的企业级 Agent Runtime。用户只需要选择 Terminal 或桌面端；Session、Context、Model、Tool、Workspace、Planning、Execution、Memory 等 Runtime 都由同一个 `EnterpriseAgent` 在进程内组合和管理，不需要逐个启动子服务。

```text
Terminal ─┐
          ├─ EnterpriseAgent（单进程）─ Session → Context → Model → Tool
Desktop ──┘                         └─ 其余 Runtime 内部模块
```

## 快速体验：Terminal

准备 Rust 1.94+，并启动一个 OpenAI-compatible 模型服务。默认配置使用 Ollama：

```powershell
ollama pull qwen3
ollama serve
```

在项目根目录初始化并运行：

```powershell
cargo run -p agent-cli --bin agent -- init
cargo run -p agent-cli --bin agent -- run "分析当前项目并给出下一步建议"
cargo run -p agent-cli --bin agent -- chat
```

`agent init` 会创建 `.agent/config.yaml`。默认 `server.mode: embedded`，CLI 会直接加载 Runtime；不需要启动 Agent Server。模型、工作区和会话数据均由这一个进程管理，会话数据保存在 `.agent/runtime`。

也可直接使用 DeepSeek 等 OpenAI-compatible 服务。API Key 只通过环境变量注入，配置文件只保存变量名；`.agent/` 已加入 `.gitignore`：

```yaml
model:
  provider: deepseek
  endpoint: https://api.deepseek.com
  name: deepseek-v4-flash
  profile: default
  api_key_env: CORE_AGENT_API_KEY
permissions:
  mode: risk-based
```

```powershell
$env:CORE_AGENT_API_KEY = "<your-api-key>"
cargo run -p agent-cli --bin agent -- run "分析当前目录，并说明项目入口"
Remove-Item Env:\CORE_AGENT_API_KEY
```

常用命令：

```powershell
cargo run -p agent-cli --bin agent -- sessions
cargo run -p agent-cli --bin agent -- status
cargo run -p agent-cli --bin agent -- tools
cargo run -p agent-cli --bin agent -- project
```

如需连接兼容的远程部署，可在 `.agent/config.yaml` 中显式设置 `server.mode: remote` 和 `server.url`；这不是本地使用的前置条件。

## 快速体验：桌面端

准备 Node.js 20+、Rust 1.94+ 和 Tauri 2 的系统依赖，然后执行：

```powershell
cd agent-desktop
npm install
npm run tauri dev
```

桌面端在 Tauri 进程中直接持有同一个 `EnterpriseAgent`，无需另开 Terminal 或后台 Agent 服务。默认模型为 `qwen3`，默认端点为 `http://127.0.0.1:11434/v1`。可使用以下环境变量覆盖：

- `CORE_AGENT_WORKSPACE`
- `CORE_AGENT_MODEL_PROVIDER`
- `CORE_AGENT_MODEL_ENDPOINT`
- `CORE_AGENT_MODEL`
- `CORE_AGENT_MODEL_PROFILE`
- `CORE_AGENT_API_KEY`
- `CORE_AGENT_PERMISSION_MODE`（`strict` / `risk-based` / `auto`）

需要人工审批时，桌面端会显示工具名、风险等级、原因和参数，并只接受“本次允许”或“拒绝”；等待五分钟没有决定时自动拒绝。

## 工作区工具与审批模式

模型通过同一条有界工具循环使用 `list_files`、`read_file`、`write_file` 和 `run_command`。文件路径必须留在已打开的工作区；`.git`、`.agent`、`.env*`、凭据、私钥、构建目录和依赖目录不可读取或修改。读取和写入正文均限制为 256 KiB；覆盖文件必须携带最近一次读取返回的 SHA-256，避免静默覆盖并发变更。命令限制为 120 秒和 1 MiB 输出，子进程不会继承常见模型 API Key。

| 模式 | 人工审核 | 适用场景 |
|---|---|---|
| `strict` | 每次编辑、执行命令前都审核 | 安全优先、陌生代码库 |
| `risk-based`（默认） | 编辑、执行项目代码及高风险命令审核；极少数只读命令自动通过 | 日常开发的安全/效率平衡 |
| `auto` | 软审批自动通过；越界路径、敏感文件和明确破坏性命令仍拒绝 | 可信临时工作区、无人值守任务 |

Terminal 在交互式会话中使用 `[y/N]` 审批；管道、CI 等非交互输入默认拒绝。`auto` 不是 OS 沙箱：对不完全可信的仓库，优先使用 `strict` 或 `risk-based`，并在容器/虚拟机中运行。

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

真实云模型测试默认忽略，避免 CI 意外产生费用；显式设置 `CORE_AGENT_API_KEY` 后可运行：

```powershell
cargo test -p core-agent --test live_deepseek_e2e -- --ignored --nocapture
```

设计到实现的审计矩阵见 [design-docs/capability-traceability.md](design-docs/capability-traceability.md)。
