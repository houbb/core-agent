# core-agent

<p align="center">
  <strong>企业级 Agent 运行时 · 单进程 · 模块化 · 75+ 内置工具</strong>
</p>

<p align="center">
  <a href="#-快速体验">快速体验</a> ·
  <a href="#-内置工具">内置工具</a> ·
  <a href="#-配置">配置</a> ·
  <a href="#-架构">架构</a> ·
  <a href="#-命令">命令</a> ·
  <a href="#-验证">验证</a>
</p>

---

## 📦 简介

**core-agent** 是一个单进程、模块化的企业级 Agent Runtime。Session、Context、Model、Tool、Workspace、Planning、Execution、Memory 等全部 Runtime 模块由同一个 `EnterpriseAgent` 在进程内组合和管理，不需要逐个启动子服务。

用户只需选择 **Terminal** 或 **Desktop** 两种接入方式，底层共享同一套能力。

```text
Terminal ─┐
          ├─ EnterpriseAgent（单进程）─ Session → Context → Model → Tool
Desktop ──┘                         └─ 其余 Runtime 内部模块
```

---

## 🚀 快速体验

### Terminal 模式

```bash
# 打开任意项目目录，直接运行
cargo run -p agent-cli --bin agent -- chat

# 单次执行
cargo run -p agent-cli --bin agent -- run "分析当前项目并给出下一步建议"
```

`agent chat` 启动全屏交互式 TUI，包含品牌/项目状态区、滚动会话区、输入框、`/` 命令面板和 `@` 文件上下文补全。

常用命令：

```bash
cargo run -p agent-cli --bin agent -- sessions   # 查看会话列表
cargo run -p agent-cli --bin agent -- status     # 查看运行时状态
cargo run -p agent-cli --bin agent -- tools      # 查看可用工具
cargo run -p agent-cli --bin agent -- project    # 查看项目信息
```

### Desktop 模式

需要 Node.js 20+、Rust 1.94+ 和 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)：

```bash
cd agent-desktop
npm install
npm run tauri dev
```

桌面端在 Tauri 进程中直接持有同一个 `EnterpriseAgent`，无需另开 Terminal 或后台服务。点击顶部 **Open folder** 选择项目即可开始。

---

## 🛠️ 内置工具

core-agent 内置 **75 个工具**，覆盖日常开发到企业运维的完整场景。每个工具自带 JSON Schema 输入校验、能力路径（`ToolCapability`）和默认权限。

### 基础工具（44 个）

| 类别 | 工具 | 默认权限 |
|------|------|---------|
| 📁 **File (11)** | read, write, edit, patch, glob, grep, delete, move, copy, info, list | 只读类 Allow，删除/移动 Ask |
| 💻 **Shell (3)** | exec, script, bg | exec Ask，script Deny |
| 🔧 **Git (7)** | diff, status, log, commit, branch, checkout, push | 只读 Allow，提交/切换 Ask |
| 🌐 **Web (2)** | fetch, search | Allow |
| 💬 **Ask (3)** | user, confirm, select | Allow |
| ✅ **Todo (3)** | add, update, list | Allow |
| 🤖 **Agent (3)** | spawn, send, list | spawn Ask |
| 📋 **Plan (3)** | create, update, review | Allow |
| ⏰ **Cron (3)** | create, list, delete | 创建/删除 Ask |
| 📝 **LSP (6)** | definition, references, hover, completion, diagnostics, symbols | Allow |

### 代码智能工具（6 个）

| 类别 | 工具 | 说明 |
|------|------|------|
| 🔍 **AST (2)** | search, replace | 语言感知的代码搜索与替换，支持 20+ 语言 |
| 📊 **Code Index (2)** | index, query | 符号索引，提取类/方法/字段 |
| 📦 **Dependency (1)** | inspect | 依赖分析（Maven/Cargo/npm/pip） |
| 🔄 **Decompiler (1)** | decompile | Java 反编译（.class / .jar） |

### 工程理解工具（4 个）

| 工具 | 说明 |
|------|------|
| `project.analyzer` | 项目结构分析，识别构建系统与框架 |
| `architecture.graph` | 架构依赖图，支持 JSON/text 输出 |
| `callgraph.query` | 函数调用链分析 |
| `api.analyzer` | REST API 端点扫描（Spring Boot / Express / Actix） |

### 运维工具（5 个，需配置外部系统）

| 工具 | 说明 |
|------|------|
| `log.query` | 日志查询（ELK / Loki / ClickHouse） |
| `metric.query` | 指标查询（Prometheus） |
| `trace.query` | 链路查询（Jaeger / SkyWalking） |
| `cmdb.query` | CMDB 查询 |
| `k8s.query` | Kubernetes 查询 |

### 企业工具（5 个，需配置外部系统）

| 工具 | 说明 |
|------|------|
| `knowledge.search` | 知识库搜索（Vector DB / Wiki） |
| `ticket.create` | 工单创建（Jira / ServiceNow） |
| `notification.send` | 通知发送（Slack / 钉钉 / 邮件） |
| `browser.navigate` | 浏览器导航 |
| `browser.screenshot` | 页面截图 |

### AI 工具（5 个，需配置模型）

| 工具 | 说明 |
|------|------|
| `code.review` | 代码审查 |
| `test.generate` | 测试生成 |
| `security.scan` | 安全扫描 |
| `data.analyze` | 数据分析 |
| `vision.analyze` | 图像分析 |

---

## ⚙️ 配置

### 配置文件

复制 [core-agent-config.example.yaml](core-agent-config.example.yaml) 到用户目录，替换 `apiKey` 即可：

- **Windows**：`C:\Users\<用户名>\core-agent\core-agent-config.yaml`
- **Linux/macOS**：`~/core-agent/core-agent-config.yaml`

### 权限覆盖示例

```yaml
tools:
  overrides:
    - tool: "shell.exec"
      permission: ask
      timeout_ms: 120000
    - tool: "git.push"
      permission: deny
    - tool: "file.delete"
      permission: ask
```

### 审批模式

| 模式 | 说明 |
|------|------|
| `strict` | 每次编辑和执行命令前都审核 |
| `risk-based`（默认） | 高风险操作审核，低风险自动通过 |
| `auto` | 软审批自动通过，越界/破坏性操作仍拒绝 |

### 环境变量

以下环境变量可覆盖配置文件：

```
CORE_AGENT_WORKSPACE
CORE_AGENT_MODEL_PROVIDER
CORE_AGENT_MODEL
CORE_AGENT_API_KEY
CORE_AGENT_PERMISSION_MODE    # strict / risk-based / auto
```

---

## 🏗️ 架构

### 设计原则

- **单进程组合**：所有 Runtime 模块由 `EnterpriseAgent` 统一管理，无需独立进程
- **提供者中立**：Tool Runtime 不依赖具体 Session、Context 或 Model
- **插件化工具**：工具通过 `BuiltinToolProvider` 注册，可扩展 MCP / Plugin / Remote
- **配置驱动**：Terminal 与 Desktop 共享同一份配置和工具实现

### 模块目录

```
core-agent/
├── core-agent-tool/       # 工具运行时（发现、校验、权限、执行、审计）
├── core-agent-session/    # 会话管理
├── core-agent-context/    # 上下文构建
├── core-agent-model/      # 模型运行时
├── core-agent-plan/       # 规划引擎
├── core-agent-memory/     # 记忆持久化
├── core-agent-multi/      # 多 Agent 运行时（团队/角色/协作）
├── core-agent-collaboration/ # 协作平台（项目/任务/审查）
├── agent-cli/             # Terminal 客户端
├── agent-desktop/         # Desktop 客户端（Tauri + Vue 3）
└── src/                   # EnterpriseAgent 组合入口
```

---

## ⌨️ 命令

### 内置命令

| 命令 | 说明 |
|------|------|
| `/help` | 帮助信息 |
| `/new` | 新建会话 |
| `/clear` | 清空上下文 |
| `/sessions` | 会话管理 |
| `/status` | 运行时状态 |
| `/tools` | 工具列表 |
| `/config` | 查看配置 |
| `/plan` | 制定计划 |
| `/review` | 代码审查 |
| `/test` | 测试生成 |
| `/fix` | 修复问题 |
| `/undo` / `/redo` | 撤销/重做 |

### 认知命令（Phase 4）

| 命令 | 用法 | 功能 |
|------|------|------|
| `/reason` | `/reason [question]` | 问题分析，输出推理摘要与证据 |
| `/think` | `/think <task>` | 复杂任务分析，评估选项并推荐 |
| `/hypothesis` | `/hypothesis [topic]` | 假设管理，支持证据与反证 |
| `/critic` | `/critic [target]` | 自我批判，发现弱点并评分 |
| `/reflect` | `/reflect [task]` | 反思学习，记录经验教训 |
| `/decision` | `/decision [topic]` | 决策记录，自动生成 ADR 到 `docs/adr/` |
| `/agents` | 查看 Agent Society 成员状态 |
| `/delegate` | 委派任务到 Agent 团队 |
| `/team` | 团队创建与管理 |
| `/roles` | 查看角色能力矩阵 |
| `/collaborate` | 查看协作过程 |

### 文件上下文

使用 `@` 引用文件或文件夹：

```text
解释 @README.md 的启动流程
对照 @"design-docs/spec.md" 检查 @src
/plan 根据 @design-docs 制定迁移方案
```

---

## 🧪 验证

```bash
# 全部测试
cargo test --workspace --all-targets

# 代码检查
cargo clippy --workspace --all-targets -- -D warnings

# 前端构建
cd agent-desktop
npm test
npm run build
```

---

## 📄 许可

MIT License