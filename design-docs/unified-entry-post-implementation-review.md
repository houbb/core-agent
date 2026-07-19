# Unified Embedded Runtime Entry — 三轮 Review

> 日期：2026-07-19  
> 范围：`EnterpriseAgent`、Terminal embedded adapter、Tauri Runtime bridge、用户文档和端到端验证。

## Review 1：架构与边界

结论：PASS。

- Terminal/Desktop 只依赖根组合入口，不需要启动 Runtime 子进程。
- 各 `core-agent-*` crate 保留领域隔离，但实例由 `EnterpriseAgent` 统一持有。
- 默认 CLI 配置只声明 `server.mode: embedded`；远程 URL 仅在显式 `remote` 模式出现。
- Kernel 管理实现了生命周期契约的 Platform；其余模块由组合根按依赖顺序构造。

## Review 2：正确性与安全

结论：PASS（本地单进程产品边界）。

- Session→Context→Model→Tool 主链使用真实 Runtime API 和持久化 Store。
- OpenAI-compatible tool schema、call/result 关联和最多 8 轮回填已打通；真实 DeepSeek 能读取初始上下文未知的工作区标记文件。
- `strict` / `risk-based` / `auto` 权限模式统一由组合根判定；批准 ID 由进程内账本消费一次，模型参数不能伪造批准。
- 工作区读写强制 canonical boundary、敏感名拒绝且目录列举不跟随符号链接；覆盖写入要求 SHA-256；命令有超时、输出、密钥环境变量和显式破坏操作边界。
- 模型配置的 Debug 输出只暴露密钥是否配置，不输出密钥正文；仓库扫描未发现疑似 API Key 文件。
- Tool 拒绝、失败或非 Success 终态不会被误报为 Agent 成功，并记录 `execution_failed`。
- Extension→Tool 使用完整 `provider/name@version` key，避免短名称猜测和歧义。
- Collaboration 通知保持事件发生时 audience，不向后来加入的成员回放旧通知。
- 前端依赖升级后 `npm audit` 为 0 vulnerabilities。

生产边界仍按能力矩阵保留：外部 IAM、OS/容器 sandbox、企业模块完整持久化、HA/cluster 不在本轮“统一本地入口”声明中；`auto` 明确不是 sandbox。

## Review 3：用户体验、文档与验证

结论：PASS。

- README 只提供 Terminal 和 Desktop 两个入口，明确不需要 Agent Server。
- CLI embedded adapter 和根组合分别有确定性 Model Provider 端到端测试。
- Desktop Vue/Vitest、TypeScript/Vite build、Tauri Rust 编译均纳入统一验证。
- Terminal 在 TTY 中逐项批准、非交互 fail-closed；Desktop 审批对话框展示风险/参数并在五分钟后默认拒绝。
- Desktop 仅在后端确认一次性决定后关闭对话框，并在等待回执时禁用重复提交。
- 权限分类、shell 控制字符、路径逃逸、覆盖冲突、人工批准编辑、自动批准编辑均有断言；真实 Provider E2E 为显式 opt-in，避免 CI 意外产生费用。
- 全 Rust workspace 单元断言/跨 Runtime/E2E、严格 Clippy、格式检查均作为交付门禁。
