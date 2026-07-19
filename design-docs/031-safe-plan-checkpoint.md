# P031：强制只读 Plan 与文件 Checkpoint

## 目标

补齐 Coding Agent 的两个 P0 安全基线：计划/审查类命令在 Runtime 层强制只读；Agent 每轮通过 `write_file` 完成的文件变化可按 session 安全撤销和重做。

## 强制只读

- `/plan`、`/review`、`/explain`、`/commit`、`/pr` 标记为只读 Agent 命令。
- 模型请求不暴露 `filesystem.write` 工具。
- `run_command` 仅允许现有安全白名单中的只读检查命令。
- 即使模型臆造写工具或副作用命令，执行循环仍在工具调用前拒绝。
- 普通消息、`/fix`、`/refactor`、`/test` 保持原有受权限控制的执行能力。

## Checkpoint

- 每个用户请求形成一个 session 内的文件变更组；同一文件多次写入只保留请求前与请求后的状态。
- 仅追踪通过内置 `write_file` 成功写入的 UTF-8 文件；内容上限沿用 256 KiB。
- Checkpoint 保存在项目 Runtime 数据目录，不依赖 Git、不暂存、不重置用户已有变更。
- `/undo` 在当前内容仍匹配 Agent 写入后的 SHA-256 时回退整组变化；不匹配则 fail-closed。
- `/redo` 在当前内容仍匹配撤销后状态时恢复整组变化；新写入会清空 redo 栈。
- 新建文件的 undo 会删除该文件；仅当内容哈希仍等于 Agent 创建内容时允许。
- Checkpoint 数据损坏、越界路径、符号链接或持久化失败均拒绝执行。

## 非目标

- 不宣称撤销 `run_command`、网络、数据库、部署或其他外部副作用。
- 不调用 `git reset`、`git checkout` 或修改 Git index。
- 不在本 P 做会话消息 rewind；只回退文件状态并保留审计可见的命令记录。

## 验收标准

- Plan 类请求的模型工具声明中没有 `write_file`。
- Plan 中的臆造写调用在审批前被拒绝。
- 一轮创建/修改多个文件后，`/undo` 原子恢复，`/redo` 原子重做。
- 用户在 Agent 写入后手工修改文件时，undo/redo 拒绝覆盖。
- Terminal 与 Desktop 通过统一 `/undo`、`/redo` Runtime 命令使用同一实现。
- 重启 Runtime 后 checkpoint 仍可读取；损坏快照 fail-closed。
