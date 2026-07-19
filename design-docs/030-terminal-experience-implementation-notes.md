# P030 Terminal 产品体验 — Implementation Notes

## Metadata

- **Task / Feature:** 全屏 Terminal TUI、统一候选、内嵌审批
- **Date:** 2026-07-19
- **Related design:** `030-terminal-experience.md`

## Material discoveries

1. 原 Terminal 不是 TUI，只是 `stdin.lines()`；渲染器只能给最终事件加标签。
2. Embedded client 在 `send()` 内执行完整模型循环，旧审批处理器同时阻塞读取 stdin；进入 raw mode 后会和 UI 争抢输入。
3. 核心 `/` 注册表和 `@` resolver 已能作为稳定语义来源，视觉层无需复制执行逻辑。
4. OpenCode 当前由 OpenTUI 驱动；Gemini CLI 使用 React + Ink。共同模式是“UI framework + 产品自有状态/事件/组件”，而不是依赖 shell 自带布局。

## Decisions

- 使用 Rust 原生 Ratatui/Crossterm，避免为 Rust CLI 引入 Node/TypeScript sidecar。
- 只在 `chat + TTY + color` 进入 alternate-screen TUI；所有自动化入口保持纯文本合同。
- Agent 请求放到 Tokio task；审批通过 unbounded request channel + oneshot decision 回到原权限调用栈。
- 自由文本历史仅在内存中保留 100 条；不把可能包含隐私的提示落盘。
- 工作区启动时优先由 ripgrep 预索引最多 20,000 文件并推导有界文件夹候选；不足 3 字符不搜索，按键只在内存中模糊排序，最终访问仍通过核心 resolver。
- 不捕获鼠标，保留终端原生选择；`Ctrl+Shift+C` 显式复制最近 Agent/错误消息。

## Verification evidence

- `cargo test --workspace --all-targets`：完整工作区通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `agent-cli --all-targets`：TUI/CLI 31 个执行测试通过，1 个 opt-in live test 在普通回归中忽略。
- 真实 Windows Console resize smoke：`120x36 → 28x8 → 80x20 → 20x6` 渲染断言及真实窗口多次缩放均未崩溃。
- 真实全局配置 Terminal + DeepSeek E2E：通过。
- 最新 `agent.exe chat` 已在独立 Windows PowerShell 中进入交互运行态并保持响应。

## Remaining gaps

- Embedded Runtime 目前在请求完成后提供整批事件，尚未做到 token/tool 级实时流式更新。
- 工作区索引当前在打开时构建，尚未监听文件系统做增量刷新。
- 细粒度 permission rules、MCP/Hooks/LSP/后台任务仍是后续产品优先级。

## Three-pass review

1. 架构：确认 TUI 只适配 UI，命令、Context、审批均复用核心。
2. 安全/边界：补齐敏感目录、索引上限、无鼠标捕获、退出 RAII 与小窗口布局。
3. 交互/回归：修正候选 Enter 误发送、原始消息不可见、resize 与复制，并完成真实启动验证。
