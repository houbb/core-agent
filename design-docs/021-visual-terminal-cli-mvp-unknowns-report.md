# P16 Terminal CLI MVP Unknowns Report

## Scope

实现官方 `agent` CLI Client：`init/chat/run/status/sessions/config/resume/cancel`、项目配置、非敏感本地 session 指针、REST 发送、分块 SSE 事件流、Renderer 与 Client 扩展契约。不会在 CLI 内复制 Agent Runtime，也不会虚构当前仓库尚不存在的 HTTP 服务端。

## Material decisions

| 优先级 | 未知项 | 决策 |
|---|---|---|
| P0 | REST/SSE 服务端是否已存在 | 未发现对应服务端；实现真实协议 Client + 可替换 trait，E2E 使用 Mock Client，明确连接失败而非静默本地假成功 |
| P0 | `run` 如何保存/恢复 Session | 服务端返回 session ID 后原子更新 `.agent/sessions.json`；`resume` 默认使用最近一次 session，也可显式指定 |
| P0 | SSE chunk 边界 | Decoder 使用字节缓冲，支持 UTF-8/事件跨 chunk、CRLF、多 data 行和尾部 flush |
| P0 | CLI 是否上传源码 | 只发送 workspace 根路径元数据和目标，不扫描或上传源码正文；默认 localhost |
| P1 | Chat 和脚本输出 | `run` 输出稳定纯文本且适合 CI；`chat` 逐行提交，Renderer 共享同一事件模型 |
| P1 | 危险操作确认 | CLI 只传递 goal，不自行执行危险操作；审批属于 Runtime/Tool Policy，不在客户端绕过 |
| P1 | 配置格式 | 按文档生成 YAML；本地状态 JSON 不保存 token、Prompt 正文或服务端凭据 |
| P1 | TUI | MVP 使用流式行 Renderer；ratatui 面板留给 Professional CLI，避免让脚本模式依赖全屏终端 |

## Acceptance

- 所有命令可由 clap 严格解析，`agent init` 可真实生成最小 `.agent` 结构且不覆盖已有配置。
- HTTP send/status/sessions/resume/cancel 与 SSE stream 有稳定协议类型。
- 分块 SSE、事件渲染、run→state→resume 主链路有单元和 E2E。
- 连接/协议/配置错误返回非零退出，不伪造成功。

## Residual risks

- HTTP 服务端需由后续应用/API 组合层实现，当前 CLI 只有在兼容服务启动后才能完成真实 Agent Loop。
- MVP session 状态文件没有多进程文件锁；并发 CLI 实例可能后写覆盖最近 session 指针。
