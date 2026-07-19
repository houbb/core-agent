# P17 Professional CLI 实现说明

## 范围

在 Terminal MVP 上增加日常开发体验：有界 Project Snapshot、Project Index Client、统一 slash Command Registry、Profile、Review/Plan/Explain/Test/Fix/Refactor/Commit/PR/Tasks/History/Tools/Memory、Git 分支元数据与隐私收敛的命令历史。

## Project 与 Git

- `ProjectSnapshot::scan` 只检查根级 Cargo/Maven/Gradle/Node/Python/Go/Docker/README marker 和最多 128 个直接模块候选。
- 只读取 `.git/HEAD` 获取安全分支名，不执行 shell、不读取 diff、commit 或源码正文。
- `agent chat` 启动时先调用 `/api/project/index`；项目图、AST/Symbol/Reference、Review 与 History 分析由服务端负责。

## Command 与 Profile

- `CommandRegistry` 统一注册、解析、补全和帮助；支持引号参数、数量约束、重复拒绝和未知命令失败。
- slash command 与 top-level `agent project/profile/review/plan/...` 使用同一执行入口。
- Profile 按项目保存到 `.agent/profile.json`，名称严格限制，并作为显式字段随 Professional request 发送。
- 命令历史只保存 slash command，最多 500 条；普通 Prompt 默认不落本地 history。

## Professional API

- 新增 `ProfessionalAgentClient`：Project Index 与专业命令执行。
- HTTP 映射到 `/api/project/index|context|review|history|memory`、`/api/tasks`、`/api/tools` 和 `/api/command/{name}`。
- CLI 不在服务端不可用时伪造 Project/Review/Task 结果。

## 测试覆盖

- Project marker/module/Git branch 识别。
- Command 引号解析、补全、未知与重复注册拒绝。
- Profile → Project Index → Review → 私有命令历史 E2E。
- 普通 Prompt 不入 history、危险 Profile 拒绝。
- 测试命令按约定在全部剩余 P 后统一运行。

## 已知边界

- AST/Symbol/Reference Index、Git diff/history 分析和 Project Memory 仍需服务端 API 实现。
- 终端按键、剪贴板和全屏选择依赖真实 TTY 驱动，尚未引入 ratatui 状态机。
