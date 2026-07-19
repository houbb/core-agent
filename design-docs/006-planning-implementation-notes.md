# P5 Planning Runtime — Implementation Notes

> 开始日期：2026-07-18

## 范围

- 新增独立 `core-agent-plan` crate。
- 实现 Intent/Goal/Plan/Task/Step/Action、PlanningGraph、生命周期、Manager、Strategy/Builder/Reviewer、Catalog/Snapshot、Policy/Interceptor/Observer。
- 实现内存与 SQLite Catalog，严格使用五张规划表。
- 在根组合 crate 增加 Workspace/Tool → PlanningContext adapter。
- 补齐单元断言、Runtime E2E 和跨 Runtime E2E。

## 已固定的边界

- Planning 生成和管理计划，但从不调用 Tool。
- 根组合层负责读取现有 Runtime 的公开实体，Planning crate 不反向依赖它们。
- Intent 嵌入 Goal；Graph 是规划结构和依赖的校验合同，不是 Scheduler。
- 创建计划的成功终态为 Ready；执行相关状态仅作为 P6 可复用的生命周期合同。

## 实现中发现

- `Reviewing` 同时承担执行前与执行后审查；P5 生成流程固定为 `Created → Planning → Reviewing → Ready`，P6 才能进入 Executing。
- Tool key 的合法上限应与 P3 的完整 `provider/name@version` 合同一致，不能按单段 128 字节截断。
- 仅在 Manager 做“先读后写”不足以防止并发丢更新，内存与 SQLite Catalog 均改为提交时 CAS；Plan 更新同时原子保存旧版本快照。
- Planning Graph 必须严格等于实体层级和依赖边集合，否则未来 UI 与 Execution 可能读取出两套相互矛盾的结构。
- Builder 生成的 Tool Action 必须引用当前 PlanningContext 中真实存在的 tool/capability；生成后与恢复后都再次经过 Policy。
- Workspace/Session 绑定属于 Goal 身份边界，PlanningContext 缺失或不一致时在 Builder 调用前失败。
- Snapshot 必须只插入不可覆盖；Intent 与 Goal 的嵌入引用在 Plan 保存和冷恢复时交叉校验。

## 偏差与未解决风险

- SQLite Catalog 仍在 async trait 内执行同步 rusqlite 调用；高并发阶段应统一迁移到 blocking executor/异步数据库层。
- Observer 当前保证单次操作结果、成功/失败和大阶段，但 Update/Restore 的内部失败阶段仍不细分到 Build/Review/Persist。
- Restore/Resume 会重新经过 PlanningPolicy，但 API 不携带最新 Tool Catalog；P6 执行前仍必须重新解析 Tool、能力与权限。
- LLM/Workflow Builder、独立 Intent Runtime、Task Scheduler、并行执行、Human Approval 与自动重规划按设计延后。
