# P032：桌面端统一工作区体验

## 目标

让桌面端具备和 Terminal 一致的“打开目录 → 补充上下文 → 对话/分析/执行 → 人工审批”主链，不复制核心命令、上下文或权限语义。

## 用户体验

- 顶部 `Open folder` 使用系统目录选择器切换工作区，重新加载该项目隔离的 Runtime 数据。
- 输入 `/` 时显示核心 `InteractionCommandRegistry` 的命令定义。
- 输入 `@` 后至少 3 个字符才查询启动时建立的工作区索引，支持文件/文件夹模糊排序。
- `↑/↓` 选择，`Tab/Enter` 只补全候选并保留输入；再次 Enter 才发送，Shift+Enter 换行。
- 项目树选中项可一键加入 `@` 上下文；已发送消息和 Agent 回复均可复制。
- 风险操作继续使用桌面审批对话框，工作区切换会拒绝旧 Runtime 尚未完成的审批。

## 架构约束

1. 文件索引、模糊搜索和 3 字符阈值来自核心 `ContextCandidateIndex`，Desktop 不扫描磁盘。
2. `/` 候选来自核心注册表，执行仍调用 `EnterpriseAgent::execute_command`/`run_with_approval`。
3. 工作区切换替换单进程内的 `EnterpriseAgent` 组合实例，不启动 Runtime 子进程。
4. 有效配置按新工作区重新解析，Runtime 数据按规范化项目路径隔离。
5. 系统目录选择是 Tauri capability allowlist 中唯一新增的原生权限。

## 安全不变量

- 索引排除 `.git`、`.agent`、`.env*`、凭据、依赖、构建目录和符号链接。
- 候选只辅助输入，最终 `@` 解析仍执行核心越界、大小和敏感路径校验。
- 切换工作区前不泄露旧工作区会话；UI 会清空当前 conversation/session 并重新加载。
- Clipboard 仅由用户显式点击或 Terminal 快捷键触发，不自动复制内容。

## 验收标准

- 桌面端可通过系统对话框打开任意本地目录并使用该目录的 Agent Runtime。
- `/` 和 `@` 候选与 Terminal 共享来源；不足 3 字符不执行文件搜索。
- 选中候选不会发送消息，发送后用户原文可见且可复制。
- 文件/文件夹候选可在大型工作区索引中模糊过滤。
- Vue 单元测试、Tauri Rust 测试、前端构建及真实 Agent E2E 通过。
