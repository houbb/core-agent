# P030：Terminal 产品体验与安全交互

## 目标

把 `agent chat` 从裸行循环升级为可长期使用的全屏终端界面，同时保持 Terminal、Desktop 共用同一个 Agent Runtime、命令注册表、上下文解析器和权限引擎。

## 用户体验

- 启动时显示 Core Agent 标识、工作区、模型和权限模式。
- 会话区、输入框、状态栏具有稳定视觉布局，并随终端尺寸自适应。
- 输入 `/` 显示来自核心命令注册表的命令面板；输入 `@` 显示工作区文件候选。
- 支持键盘选择、补全、会话内输入历史、滚动和忙碌状态。
- 风险操作在 TUI 内显示审批卡片，用户可允许一次或拒绝。
- 非交互管道、脚本命令和 `--no-color` 行为保持兼容，不强制进入全屏界面。

## 架构约束

1. TUI 不重新实现 `/` 命令语义；候选和执行均调用核心注册表。
2. TUI 不解析 `@` 内容；候选只帮助输入，最终解析仍由 `ContextMentionResolver` 完成。
3. TUI 不决定权限；它只实现 `EnterpriseApprovalHandler` 的交互适配器。
4. Agent Runtime 仍在当前进程内组合，不启动一组子 Runtime 或子 Agent 进程。
5. 终端恢复必须具备失败保护，退出后不得遗留 raw mode 或 alternate screen。

## 安全不变量

- API key 不进入界面、消息历史或子命令环境。
- 文件候选限制在工作区，跳过符号链接、敏感目录和构建产物。
- 忙碌期间不允许并发提交第二个请求。
- 审批默认拒绝；`Esc`、`n` 和审批通道断开均视为拒绝。
- TUI 的输入历史仅保存在当前进程内，避免把自由文本提示持久化到磁盘。

## 验收标准

- `agent chat` 在交互终端中进入全屏 TUI。
- 首屏包含品牌区、Conversation、Message、模型、权限和工作区信息。
- `/` 和 `@` 候选能够键盘选择并补全。
- 普通消息和内置命令都能从 TUI 到达现有统一实现。
- 写文件或执行高风险命令时，TUI 内出现审批卡片并能返回 Allow once / Deny。
- Terminal 关闭、错误或 panic 后恢复屏幕状态。
- TUI 状态/渲染有断言测试，真实 DeepSeek Agent E2E 和完整工作区测试通过。

## 后续优先级

- P0：只读 Plan 模式、会话级文件检查点与 undo/redo。
- P1：流式 token/tool 事件、上下文压缩、细粒度 allow/ask/deny 规则。
- P2：MCP、Hooks、自定义 Agent、LSP、后台任务和多 Agent 产品入口。

## 对标依据

- OpenCode TUI：<https://opencode.ai/docs/tui/>
- OpenCode Permissions：<https://dev.opencode.ai/docs/permissions/>
- Claude Code CLI：<https://code.claude.com/docs/en/cli-usage>
- Claude Code Permission Modes：<https://code.claude.com/docs/en/permission-modes>
- Claude Code Checkpointing：<https://code.claude.com/docs/en/checkpointing>
