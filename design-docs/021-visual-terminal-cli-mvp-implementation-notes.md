# P16 Terminal CLI MVP 实现说明

## 范围

新增官方 `agent` 终端 Client，提供 `init`、`chat`、`run`、`status`、`sessions`、`config`、`resume`、`cancel`。CLI 只负责交互、配置、本地 session 指针和 Agent API 调用，不复制 Runtime。

## 结构

- 新增 workspace crate `agent-cli`，同时提供可复用 library 和 `agent` binary。
- `AgentClient` 稳定定义 send/stream/resume/cancel/status/sessions；`HttpAgentClient` 实现 REST + SSE。
- `Renderer` 与 `TerminalRenderer` 分离，黑底终端采用金色 ANSI 强调，非 TTY/CI 自动输出无颜色稳定文本。
- `CliApplication` 负责命令用例、事件消费和本地状态提交，只有收到 terminal event 后才保存 session。

## 本地项目与恢复

- `agent init` 生成 `.agent/config.yaml`、`context.yaml` 和 `memory/`，已有配置时拒绝覆盖。
- `.agent/sessions.json` 只保存当前/最近 session ID，使用临时文件 rename 更新，不保存 Prompt、Token 或凭据。
- `run` 创建新 session；`chat` 复用当前 session；`resume/status/cancel` 可显式指定或使用最近 session。

## 传输与安全

- 默认服务为 `http://127.0.0.1:8080`，只发送 workspace 路径元数据和用户目标，不上传源码正文。
- SSE Decoder 使用有界字节缓冲，支持 CRLF、UTF-8 和跨 chunk 事件、多 data 行及尾帧。
- HTTP 非成功状态、非法响应、流在 terminal event 前结束均显式失败，不伪造执行成功。

## 测试覆盖

- 单元/Runtime E2E：命令解析、初始化不覆盖、分块 SSE、run→state→resume、流提前终止不落状态。
- 二进制 E2E：真实执行 `agent init`，验证文件系统结果和重复初始化非零退出。
- HTTP 服务端尚未在当前设计阶段提供，因此 Agent Loop E2E 使用协议级 Mock Client；测试命令在全部剩余 P 后统一运行。

## 已知边界

- 兼容 REST/SSE 服务启动前，联网命令会明确连接失败。
- MVP 本地 session 指针没有多进程文件锁；全屏 TUI、命令面板和项目智能留给下一 P。
