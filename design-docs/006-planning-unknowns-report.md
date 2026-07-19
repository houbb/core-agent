# P5 Planning Runtime — Unknowns Report

> 日期：2026-07-18  
> 输入：`000-roadMap.md`、`006-planning.md` 与现有公开 Runtime 合同

## 结论

P5 实现独立 `core-agent-plan`，只负责把 Goal 转换为可审查、可持久化、可恢复的 Plan。它不调用 Model、不执行 Tool、不调度任务；跨 Runtime 数据由根组合 crate 转换为有界 `PlanningContext`。

## Known knowns

- 固定层级为 `Intent → Goal → Plan → Task → Step → Action`，其中 Intent 是 Goal 的上游语义引用。
- Planning 必须提供 Manager、Strategy、Builder、Reviewer、Lifecycle、Snapshot 与 Graph。
- SQLite MVP 只有 `goal`、`plan`、`task`、`step`、`plan_snapshot` 五张表，全部带审计列、注释、索引且无外键。
- P5 不包含 DAG Scheduler、并行执行、Tree Search、Reflection、自动重规划、完整 Workflow Engine 或 Tool 执行。
- 现有 Tool 与 Workspace Runtime 不依赖上层 Runtime；根 crate 已承担跨 Runtime 组合职责。

## 高影响未知项与决策

| 未知项 | 风险 | P5 决策 |
|---|---|---|
| 生命周期既含 `Executing`，流程图又要求执行前 Review | 状态合同冲突并可能抢跑 P6 | 支持两段 Review：`Planning → Reviewing → Ready`，P6 可走 `Ready → Executing → Reviewing → Completed`；P5 只生成到 Ready，不执行 |
| 设计建议独立 Intent，但数据库限定五表 | 立即加第六表会扩大范围，忽略 Intent 会破坏演进 | Intent 作为可共享 ID + 完整嵌入值保存在 Goal 中；暂不建立 Intent 表/Manager |
| “Planning Graph”与“不做 DAG Scheduler”边界 | 容易把 P6 调度提前塞入 P5 | Graph 只表达 Contains/DependsOn、校验引用与无环；不做拓扑调度或执行 |
| Context/Model/Tool/Workspace 的依赖方向 | 直接依赖会形成耦合和环 | Planning crate 只接收 provider-neutral `PlanningContext` 与 `ToolReference`；根 crate 提供 Workspace/Tool adapter |
| Action 参数可能携带凭据 | 计划快照和 SQLite 泄密 | 参数设大小上限，递归拒绝 secret/token/password/api_key/private_key 等敏感键 |
| 更新、取消、恢复的并发语义 | 丢更新或恢复到不一致版本 | Plan 使用乐观版本；Catalog 以单事务保存“旧快照 + 新 Plan” |
| Reviewer 拒绝后的状态 | 错误地把未批准计划标为 Ready | Approved 才进入 Ready；ChangesRequired/Rejected 保持 Reviewing 并返回可查询 Plan |
| Builder 的默认实现 | 没有 Model 时无法端到端验证 | 提供确定性 Rule Builder；LLM/Workflow Builder 仅保留扩展 trait |
| Observer/Interceptor 失败 | 插件 panic 污染核心状态 | panic 隔离；Interceptor 错误中止提交，Observer 只接收结果事件且不改变结果 |

## 保守假设

- Task/Step 草稿使用稳定 key；Manager 以 Plan 命名空间 UUID v5 生成确定性实体 ID。
- PlanningContext 只保存创建计划所需的有界摘要，不把完整 Workspace 文件、Tool Schema 或 Context 正文落库。
- 同一 Plan 的 Task key、全局 Step key 必须唯一；依赖必须引用同一 Plan 且不得成环。
- `cancel_plan` 自动保存取消前快照；`resume_plan` 恢复最近的取消前快照并递增版本。
- 删除、自动重规划、执行重试、并行调度、人类审批工作流均推迟到后续 P。

## 验证证据计划

- 单元断言：领域校验、敏感参数、生命周期、Graph 环/悬空引用、Rule Builder、Reviewer、乐观版本。
- Runtime E2E：Goal → create Plan → update → cancel → resume → snapshot restore；Reviewer 拒绝；SQLite 冷恢复与损坏检测。
- 跨 Runtime E2E：Workspace + ToolDefinition → PlanningContext → Ready Plan，且没有 Tool 执行。
- 统一验证：`cargo fmt --check`、P5 `clippy -D warnings`、P5 测试、全工作区测试。

## 延后项

- 独立 Intent Runtime/表、LLM PlanBuilder、Workflow PlanBuilder。
- P6 Task Scheduler、Action 执行、并行与失败恢复。
- Human-in-the-loop 审批、Reflection、Tree Search、自动重规划。
