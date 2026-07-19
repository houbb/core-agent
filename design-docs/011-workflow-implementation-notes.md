# P10 Workflow Runtime 实现说明

## 实现范围

本次按 `011-workflow.md` 的 P10.0 边界实现确定性的顺序 Workflow Runtime。核心业务层级为：

`Workflow → Stage → Activity → Action`

已实现 Definition 版本管理、Instance 生命周期、顺序调度、暂停/恢复/取消、Snapshot、Timeline、扩展契约、SQLite 持久化，以及根 crate 到 Execution Runtime 的真实适配。未实现 DAG、并行、条件分支、定时/事件触发、人工审批、补偿编排、DSL 解释器和可视化编辑器。

## 架构

- 独立 crate：`core-agent-workflow`，不依赖 Tool、Model、Context 等业务 Runtime。
- `WorkflowManager`：注册、启动、恢复、暂停、取消、归档、快照和查询的统一入口。
- `WorkflowEngine`：两阶段端口。`prepare` 返回可持久化 binding，`execute` 只使用既有 binding，确保冷恢复不重复创建下游 Execution。
- `ExecutionWorkflowEngine`：位于根组合层，通过 `WorkflowPlanResolver` 获取已批准 Plan，再委托 `ExecutionManager`；Workflow 本身不执行 Tool。
- `WorkflowScheduler`：P10 使用确定性顺序 Scheduler；其余 Policy、Lifecycle、Interceptor、Observer、Registry、Store 均可替换。

## 一致性与恢复

- Definition 不可变版本化，Instance 内嵌并固定完整 Definition 快照。
- 每个 Action 使用基于 Instance/Action 的稳定 UUID v5 dispatch ID。
- binding 在执行前持久化；Waiting、Running 和进程重启后均复用同一 binding。
- `Created`、`Scheduled`、`Paused`、`Waiting`、`Running` 均有公开恢复路径。
- 未确认的执行或取消结果返回 `OutcomeUnknown`，保留 Running 状态，禁止盲目重放。
- 生命周期变化与 Timeline 记录在同一 Store 事务提交；初始记录强制为 `None → Created`。
- Stage/Activity 状态必须由子项聚合推导，禁止持久化伪造进度。
- live control 使用不覆盖的原子占位，竞争恢复不会替换真正执行任务的暂停/取消令牌。

## 持久化

SQLite 严格使用五张表：

- `workflow`
- `workflow_definition`
- `workflow_instance`
- `workflow_snapshot`
- `workflow_state`

所有表均包含 `id/create_time/update_time/create_user/update_user`、注释和索引，不使用外键。读取时交叉校验结构化列、JSON、Definition 所有权、Snapshot 所有权和 Timeline 连续性；写入使用事务与乐观版本 CAS。

## 最小使用方式

1. 构建 `WorkflowDefinition`，定义 Stage、Activity 和 Action。
2. 使用 `WorkflowManager::register` 注册版本。
3. 使用 `WorkflowManager::start(StartWorkflowRequest)` 启动。
4. 返回 Waiting/Paused/Running 时，使用持久化 Instance ID 调用 `resume`；Runtime 会复用原 binding。
5. 使用 `snapshot`、`list_states`、`find_instance` 获取恢复点和审计信息。

## 验证证据

- `cargo test -p core-agent-workflow`：4 个单元断言、15 个 Runtime E2E 全部通过。
- `cargo test -p core-agent --test workflow_execution_integration`：1 个真实 Workflow → Execution 集成 E2E 通过。
- `cargo clippy -p core-agent-workflow --all-targets -- -D warnings`：通过，零 warning。
- `cargo test --workspace`：全工作区回归通过。
- `cargo fmt --all -- --check`、`git diff --check`：通过。

## 已知边界

- Workflow Store 与 Execution Store 是两个持久化边界，`prepare` 成功但 binding 提交失败时可能留下未启动的 READY Execution，需要后续运维回收或跨 Store outbox；P10 不引入分布式事务。
- 下游副作用完成后若 Workflow 提交结果失败，恢复执行具有 at-least-once 语义；Engine 必须按稳定 dispatch/binding 提供幂等或结果查询能力。
- SQLite 实现使用同步 `rusqlite`，适合 P10 单进程小规模运行；高并发容量治理留待后续持久化演进。
