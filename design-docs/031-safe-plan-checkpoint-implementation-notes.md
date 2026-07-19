# P031 强制只读 Plan 与文件 Checkpoint — Implementation Notes

## Implemented

- `/plan`、`/review`、`/explain`、`/commit`、`/pr` 在核心 invocation 上标记只读；模型工具声明隐藏写工具，执行循环再次拒绝写入和非白名单命令。
- `write_file` 使用崩溃可恢复 pending journal，把同一请求内同文件的多次写入聚合为首个 before/最终 after。
- Checkpoint 按 session 持久化在项目 Runtime 数据目录；每轮 16 MiB/256 文件、每 session 20 轮有界。
- 核心 `/undo`、`/redo` 对整组文件执行哈希 CAS；用户手工修改、损坏数据、越界、符号链接均 fail-closed。
- Checkpoint 只覆盖内置 UTF-8 `write_file`，不声称撤销 shell、网络、Git index 或远程副作用。

## Verification evidence

- 核心 11 个断言测试通过，包含持久化重开、多文件 undo/redo 与手工修改冲突。
- 企业 Agent 4 个 E2E 通过，真实覆盖 Plan 臆造写拒绝、人工批准写入、checkpoint event、`/undo`/`/redo`。
- 完整 workspace 测试、零警告 Clippy、Terminal/Desktop 定向回归全部通过。

## Three-pass review

1. 架构：Checkpoint 只包裹统一 `write_file`，命令注册在核心，不依赖 Git 或入口 UI。
2. 安全/恢复：加入写前 pending journal、内容/数量/历史上限、路径/符号链接校验、整组预校验和失败补偿。
3. 并发/审计：undo/redo 与 Agent run 共用操作锁，哈希冲突 fail-closed，并追加不含文件正文的 session event。
