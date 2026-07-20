# 内置 Slash 命令参考

## 概述

项目内置了一套 slash 命令体系，由 `InteractionCommandRegistry` 统一管理，适用于 **CLI 终端** 和 **Desktop 桌面端** 两种入口。命令按路由分为三类：

| 路由 | 说明 |
|------|------|
| `Entry` | 入口层命令，由 CLI 层直接处理（无需模型） |
| `Runtime` | 运行时命令，由 `EnterpriseAgent` 直接处理（无需模型） |
| `Agent` | 代理命令，转发给 AI 模型处理 |

---

## 命令一览

### 🏠 Entry —— 入口层命令

| 命令 | 用法 | 说明 |
|------|------|------|
| `/help` | `/help` | 列出所有内置命令 |
| `/new` | `/new` | 开启一个新的聊天会话 |
| `/clear` | `/clear` | 清除当前聊天视图 |
| `/exit` | `/exit` | 退出交互式聊天 |
| `/profile` | `/profile [name]` | 查看或切换 Agent 配置档（profile） |

### ⚙️ Runtime —— 运行时命令

| 命令 | 用法 | 说明 |
|------|------|------|
| `/project` | `/project` | 索引并描述当前项目工作区 |
| `/tasks` | `/tasks` | 列出活跃的 Agent 会话 |
| `/sessions` | `/sessions` | 列出所有聊天会话 |
| `/history` | `/history [query]` | 查看项目历史（可选按关键词过滤） |
| `/config` | `/config` | 显示当前生效的配置信息 |
| `/status` | `/status` | 显示当前会话状态 |
| `/tools` | `/tools` | 列出所有已注册的可用工具 |
| `/memory` | `/memory` | 显示项目记忆状态（启用/禁用） |
| `/undo` | `/undo` | 撤销最近一次 Agent 文件变更检查点 |
| `/redo` | `/redo` | 重做最近一次撤销的 Agent 文件检查点 |

### 🤖 Agent —— 代理命令（由模型处理）

| 命令 | 用法 | 说明 | 只读 |
|------|------|------|------|
| `/plan` | `/plan <goal>` | 创建实现计划 | ✅ |
| `/review` | `/review [target]` | 审查当前变更 | ✅ |
| `/explain` | `/explain <target>` | 解释项目代码 | ✅ |
| `/commit` | `/commit` | 生成 commit 提案 | ✅ |
| `/pr` | `/pr` | 生成 PR 提案 | ✅ |
| `/test` | `/test [target]` | 运行或规划测试 | ❌ |
| `/fix` | `/fix [target]` | 修复当前问题 | ❌ |
| `/refactor` | `/refactor <target>` | 重构指定目标 | ❌ |

> **只读命令**：`/plan`、`/review`、`/explain`、`/commit`、`/pr` 被标记为 read-only，模型在处理时不能编辑文件或执行有副作用的命令。

---

## 架构说明

### 定义位置

- **核心定义**：`src/interaction.rs` — `InteractionCommandRegistry` / `InteractionCommandDefinition`
- **运行时处理**：`src/enterprise.rs` — `EnterpriseAgent::execute_command()` 处理 Entry + Runtime 路由，`run_with_approval_inner()` 处理 Agent 路由
- **CLI 封装**：`agent-cli/src/professional.rs` — `CommandRegistry` 包装核心注册表
- **CLI 本体**：`agent-cli/src/command.rs` — `CliCommand` 枚举，每个变体对应一个 CLI 子命令

### 参数限制

- 命令名：1-64 字符，仅允许小写 ASCII 字母和连字符 `-`
- 参数数量：最低 0 个，最多 32 个
- 命令长度：最大 64 KiB
- 参数支持转义引号，支持空格分隔和双引号包裹

### 扩展机制

通过 `registry.register(InteractionCommandDefinition { ... })` 可在运行时注册自定义命令。注册时指定名称、摘要、用法、参数范围、路由类型。

---

## 上下文引用（`@` 语法）

虽然不是 slash 命令，但 `@` 上下文引用是交互系统的重要组成部分：

- `@path/to/file` — 引用工作区中的文件
- `@directory/` — 引用目录（递归展开）
- `@"path with spaces"` — 引用含空格的路径
- 限制：最多 16 个引用，128 个文件，单文件 256 KiB，总计 1 MiB