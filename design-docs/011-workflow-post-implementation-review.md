# P10 Workflow Runtime 实现后审查

## 结论

**PASS**。P10.0 顺序 Workflow Runtime 已按文档边界完成，专项测试、跨 Runtime E2E、严格 Clippy、格式检查和全工作区回归均通过。

## 第一轮：架构与边界审查

- 确认 Workflow 只负责业务编排与治理，副作用通过 `WorkflowEngine` 委托给 Execution。
- 确认依赖方向为根组合层 `Workflow → Execution`，Workflow crate 不反向依赖 Tool/Model/Context。
- 确认只实现顺序调度，没有提前引入 DAG、并行、条件、触发器、审批、补偿或 UI。
- 修复超时取消语义：只有下游终态取消被确认后才落 Failed；取消未确认时保留 Running 并返回 `OutcomeUnknown`。

## 第二轮：并发、恢复与可测试性审查

发现并修复：

- 并发 resume 可能覆盖 live control：改为 `HashMap::entry` 不覆盖占位，并增加单元回归。
- `Created/Scheduled` 崩溃窗口无恢复路径：`resume` 支持启动阶段冷恢复，并覆盖 Created 恢复 E2E。
- 生命周期可脱离 Timeline 更新：Store 强制状态变化与匹配记录同事务提交。
- 内存库与 SQLite Definition 校验不一致：两者均要求 Instance 内嵌 Definition 与 Catalog 一致。
- Stage/Activity 聚合状态可伪造：领域校验重新推导并拒绝不一致内容。

同时补充 live cancel、未知结果恢复、Interceptor panic 隔离、SQLite Snapshot/Timeline 重开与篡改测试。

## 第三轮：静态与全量回归审查

- 生产代码未发现 `unwrap/expect/panic!/todo!/unimplemented!`。
- SQLite 恰好五张 P10 表，全部具备审计字段、注释、索引且无外键。
- P10 严格 Clippy 零 warning；格式和 diff 检查通过。
- 全 workspace 测试通过；根 crate 仅保留此前已有的 8 条 ambiguous glob re-export warning，本 P 未扩大该问题。

## 遗留风险

- Workflow/Execution 双 Store 之间不存在原子事务，prepare orphan 需要未来 outbox/回收机制。
- Workflow 结果提交失败后的下游调用是 at-least-once，生产 Engine 必须保证幂等或可查询。
- 同步 SQLite 在高吞吐 async 场景可能阻塞 worker；P10 适用范围限定为单进程、小规模顺序 Workflow。

这些风险已记录，不影响 P10.0 的验收边界。
